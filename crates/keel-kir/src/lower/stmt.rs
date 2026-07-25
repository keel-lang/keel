//! Statement lowering — the M0 scalar subset plus M1's `for`-over-ranges
//! and M2's named-struct literals in `let`/`return` position, `when` as
//! a statement (over a simple-enum or a str/int scrutinee with a wildcard
//! arm — see [`lower_when_stmt`]) and *as an expression* in `let`/`return`
//! position (see [`TailSink::Assign`] and [`lower_when_expr_let`]), and
//! `for x in xs` over a `list[T]` (lowered to [`keel_kir::ir::Stmt::ForEach`]):
//! `let`/assign, `if`/`else`, `while`, `for x in a..b`, `for x in xs`,
//! `return`, `when`, and bare expression statements. Everything else
//! (`try`/`catch`, `raise`, `assert`, `break`/`continue`, `self.field = ...`,
//! `for` with a `where` filter or a non-range, non-list iterable) is
//! rejected; see module docs on `lower/mod.rs`.

use keel_syntax::ast::{self, Node};
use keel_syntax::lexer::Span;

use super::{FnCtx, LowerCtx, LowerError, binding_ident, ty_expr_to_kir};
use crate::ir::{self, Block, LocalId};
use crate::span_table::SpanTable;
use crate::types::KirType;

use super::expr::{lower_expr, lower_expr_expecting, struct_spread_base};

/// What a bare tail expression (the last statement of a block, reached in
/// tail position — see [`lower_block`]) desugars into. Generalizes issue
/// #159's `is_tail: bool` so the same tail-position plumbing serves both
/// a task's own implicit return and issue #160's `when`-as-expression,
/// which needs the arm's tail value written into a temporary rather than
/// returned outright.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TailSink {
    /// Not a tail position (or the enclosing function returns `Unit`) — a
    /// bare tail expression is just evaluated for its side effect, same as
    /// any other expression statement.
    Discard,
    /// The task's own implicit return (issue #159): a bare tail expression
    /// becomes `return <expr>`.
    Return,
    /// A `when`-expression's result local (issue #160): a bare tail
    /// expression becomes `<local> = <expr>` instead of a `return` — used
    /// while lowering each arm of a `when` used as an expression, where the
    /// arm's "tail value" is the value the whole `when`-expression produces,
    /// not the enclosing function's return value.
    Assign(LocalId),
}

/// Lowers a `{ ... }` block in its own child scope. `sink` is what the
/// block's own last statement's tail position desugars into (see
/// [`TailSink`]) — only that last statement inherits `sink`; every earlier
/// statement gets [`TailSink::Discard`] regardless of the block's own
/// tailness.
pub(crate) fn lower_block(
    block: &ast::Block,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    ret_ty: KirType,
    sink: TailSink,
) -> Result<Block, LowerError> {
    ctx.push_scope();
    let last_idx = block.len().checked_sub(1);
    let mut out = Vec::with_capacity(block.len());
    for (i, stmt) in block.iter().enumerate() {
        let stmt_sink = if Some(i) == last_idx {
            sink
        } else {
            TailSink::Discard
        };
        out.extend(lower_stmt(stmt, ctx, lcx, table, ret_ty, stmt_sink)?);
    }
    ctx.pop_scope();
    Ok(out)
}

/// Lowers a single statement, possibly to more than one [`ir::Stmt`] (a
/// `let`/`return` whose value is a `when`-expression desugars to a
/// declare-only `Let` followed by an `if`-chain that assigns into it, or
/// the chain itself — see [`lower_when_expr_let`]). Does not open its own
/// scope — callers that need block scoping (`if`/`while` bodies) go through
/// [`lower_block`]; the synthetic top-level function lowers its statements
/// directly into the function's single root scope.
///
/// `sink` is only ever non-[`TailSink::Discard`] for a statement that is
/// the last statement of a block reached in tail position from the task's
/// own body (or from a `when`-expression's own tail position) — `if`/`else`
/// branches (and, transitively, `when` arms, already desugared to nested
/// `if`s by [`lower_when_stmt`]) propagate their own `sink` into both
/// branches; a `while`/`for` body is never a tail position (loops aren't
/// exhaustive) and always lowers with `sink = TailSink::Discard`.
pub(crate) fn lower_stmt(
    stmt: &Node<ast::Stmt>,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    ret_ty: KirType,
    sink: TailSink,
) -> Result<Vec<ir::Stmt>, LowerError> {
    match &stmt.kind {
        ast::Stmt::Let { binding, ty, value } => {
            let name = binding_ident(binding, &stmt.span)?;
            if let ast::Expr::WhenExpr { subject, arms } = &value.kind {
                return lower_when_expr_let(
                    name, ty, subject, arms, ctx, lcx, table, ret_ty, &stmt.span,
                );
            }
            // An explicit annotation pins the expected type up front — this
            // is what lets a struct literal (which the checker always types
            // as an unnamed structural shape, see `lower/mod.rs`'s
            // `CheckArtifacts` doc) resolve to the *named* struct the
            // annotation calls for.
            let init = if let Some(annotation) = ty {
                let expected = ty_expr_to_kir(
                    annotation,
                    lcx.structs_by_name,
                    lcx.enums_by_name,
                    lcx.lists,
                    lcx.maps,
                    lcx.sets,
                    lcx.nullables,
                )?;
                lower_expr_expecting(value, expected, ctx, lcx, table)?
            } else if let ast::Expr::StructSpreadUpdate { base, .. } = &value.kind {
                // No annotation, but the spread's own base already carries a
                // known struct type (`dev = { ...base, debug: true }`) — use
                // that instead of requiring a redundant annotation.
                let (_, expected) = struct_spread_base(base, ctx)?;
                lower_expr_expecting(value, expected, ctx, lcx, table)?
            } else {
                lower_expr(value, ctx, lcx, table)?
            };
            let declared_ty = init.ty();
            let local = ctx.declare(name, declared_ty);
            Ok(vec![ir::Stmt::Let {
                local,
                init: Some(init),
            }])
        }
        ast::Stmt::AugAssign {
            name,
            name_span,
            op,
            rhs,
        } => {
            let local = ctx.resolve(name).ok_or_else(|| {
                LowerError::new(format!("unknown identifier `{name}`"), name_span.clone())
            })?;
            let local_ty = ctx.locals[local].ty;
            let rhs_expr = lower_expr(rhs, ctx, lcx, table)?;
            let op = super::expr::convert_binop(*op);
            let result_ty = super::expr::infer_binop_ty(op, local_ty, rhs_expr.ty(), &stmt.span)?;
            if result_ty != local_ty {
                return Err(LowerError::new(
                    format!(
                        "`{name}` is `{local_ty}`; `+=`-family ops must preserve the operand type (got `{result_ty}`)"
                    ),
                    stmt.span.clone(),
                ));
            }
            let value = ir::Expr::BinOp {
                op,
                left: Box::new(ir::Expr::Local {
                    id: local,
                    ty: local_ty,
                }),
                right: Box::new(rhs_expr),
                ty: result_ty,
            };
            Ok(vec![ir::Stmt::Assign { local, value }])
        }
        ast::Stmt::Return(None) => {
            if ret_ty != KirType::Unit {
                return Err(LowerError::new(
                    format!("bare `return` in a function declared to return `{ret_ty}`"),
                    stmt.span.clone(),
                ));
            }
            Ok(vec![ir::Stmt::Return(None)])
        }
        ast::Stmt::Return(Some(value)) => {
            if let ast::Expr::WhenExpr { subject, arms } = &value.kind {
                // `return when ... { ... }` — each arm's tail value returns
                // directly (no intermediate temp needed, unlike the `let`
                // case): reuses the exact same chain-building machinery as
                // `when` used as a statement (issue #146), just with
                // `TailSink::Return` instead of whatever sink this `return`
                // itself was lowered under (a `return` is always a genuine
                // return regardless of its own tail position).
                let chain = lower_when_stmt(
                    subject,
                    arms,
                    ctx,
                    lcx,
                    table,
                    ret_ty,
                    TailSink::Return,
                    &stmt.span,
                )?;
                return Ok(vec![chain]);
            }
            let expr = lower_expr_expecting(value, ret_ty, ctx, lcx, table)?;
            Ok(vec![ir::Stmt::Return(Some(expr))])
        }
        ast::Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            let cond_expr = lower_expr(cond, ctx, lcx, table)?;
            if cond_expr.ty() != KirType::Bool {
                return Err(LowerError::new(
                    format!("`if` condition is `{}`, expected `bool`", cond_expr.ty()),
                    cond.span.clone(),
                ));
            }
            let then_branch = lower_block(then_body, ctx, lcx, table, ret_ty, sink)?;
            let else_branch = match else_body {
                Some(body) => lower_block(body, ctx, lcx, table, ret_ty, sink)?,
                None => Vec::new(),
            };
            Ok(vec![ir::Stmt::If {
                cond: cond_expr,
                then_branch,
                else_branch,
            }])
        }
        ast::Stmt::While { cond, body } => {
            let cond_expr = lower_expr(cond, ctx, lcx, table)?;
            if cond_expr.ty() != KirType::Bool {
                return Err(LowerError::new(
                    format!("`while` condition is `{}`, expected `bool`", cond_expr.ty()),
                    cond.span.clone(),
                ));
            }
            let body = lower_block(body, ctx, lcx, table, ret_ty, TailSink::Discard)?;
            Ok(vec![ir::Stmt::While {
                cond: cond_expr,
                body,
            }])
        }
        ast::Stmt::Expr(expr) => match sink {
            TailSink::Discard => Ok(vec![ir::Stmt::Expr(lower_expr(expr, ctx, lcx, table)?)]),
            // A bare tail expression implicitly returns its value (mirrors
            // `exec_block`'s `StmtOutcome::Value` in the interpreter, issue
            // #159) — but only when the task actually returns a value; a
            // `Unit`-returning task's tail expression is still just
            // evaluated for its side effect.
            TailSink::Return if ret_ty != KirType::Unit => {
                let value = lower_expr_expecting(expr, ret_ty, ctx, lcx, table)?;
                Ok(vec![ir::Stmt::Return(Some(value))])
            }
            TailSink::Return => Ok(vec![ir::Stmt::Expr(lower_expr(expr, ctx, lcx, table)?)]),
            // A `when`-expression arm's tail value (issue #160) — written
            // into the result local instead of returned.
            TailSink::Assign(local) => {
                let expected = ctx.locals[local].ty;
                let value = lower_expr_expecting(expr, expected, ctx, lcx, table)?;
                Ok(vec![ir::Stmt::Assign { local, value }])
            }
        },
        ast::Stmt::SelfAssign { .. } => Err(LowerError::unsupported(
            "self.field assignment",
            stmt.span.clone(),
        )),
        ast::Stmt::For {
            binding,
            iter,
            filter,
            body,
        } => {
            if filter.is_some() {
                return Err(LowerError::unsupported(
                    "`for ... where` filter",
                    stmt.span.clone(),
                ));
            }
            let name = binding_ident(binding, &stmt.span)?;
            let ast::Expr::Range(start, end) = &iter.kind else {
                // Not a range literal — try a `list[T]` iterable instead of
                // rejecting outright.
                let iter_e = lower_expr(iter, ctx, lcx, table)?;
                let KirType::List(list_id) = iter_e.ty() else {
                    return Err(LowerError::unsupported(
                        "`for` over a non-range, non-list iterable (maps/sets land in a later \
                         M2 issue)",
                        iter.span.clone(),
                    ));
                };
                let elem_ty = lcx.lists.borrow()[list_id];
                ctx.push_scope();
                let var = ctx.declare(name, elem_ty);
                let body = lower_block(body, ctx, lcx, table, ret_ty, TailSink::Discard)?;
                ctx.pop_scope();
                return Ok(vec![ir::Stmt::ForEach {
                    var,
                    elem_ty,
                    list: iter_e,
                    body,
                }]);
            };
            // Bounds are evaluated once, before `var` exists, so they lower
            // in the enclosing scope and cannot reference the loop variable.
            let low = lower_expr(start, ctx, lcx, table)?;
            if low.ty() != KirType::I64 {
                return Err(LowerError::new(
                    format!("`for` range start is `{}`, expected `int`", low.ty()),
                    start.span.clone(),
                ));
            }
            let high = lower_expr(end, ctx, lcx, table)?;
            if high.ty() != KirType::I64 {
                return Err(LowerError::new(
                    format!("`for` range end is `{}`, expected `int`", high.ty()),
                    end.span.clone(),
                ));
            }
            ctx.push_scope();
            let var = ctx.declare(name, KirType::I64);
            let body = lower_block(body, ctx, lcx, table, ret_ty, TailSink::Discard)?;
            ctx.pop_scope();
            Ok(vec![ir::Stmt::ForIndex {
                var,
                low,
                high,
                body,
            }])
        }
        ast::Stmt::When { subject, arms } => Ok(vec![lower_when_stmt(
            subject, arms, ctx, lcx, table, ret_ty, sink, &stmt.span,
        )?]),
        ast::Stmt::TryCatch { body, catches } => Ok(vec![lower_try_catch(
            body, catches, ctx, lcx, table, ret_ty, &stmt.span,
        )?]),
        ast::Stmt::Raise(message) => Ok(vec![lower_raise(message, ctx, lcx, table)?]),
        ast::Stmt::Assert { .. } => Err(LowerError::unsupported("assert", stmt.span.clone())),
        ast::Stmt::Break => Err(LowerError::unsupported("break", stmt.span.clone())),
        ast::Stmt::Continue => Err(LowerError::unsupported("continue", stmt.span.clone())),
    }
}

/// Lowers `when subject { arms }` used as a statement, or (via
/// [`TailSink::Return`]/[`TailSink::Assign`]) as the chain backing `when`
/// used as an expression (issue #160) — each arm's tail value desugars
/// according to `sink` exactly like any other tail position (see
/// [`lower_block`]).
///
/// `subject` must be a plain identifier (a local/param already in scope),
/// not an arbitrary expression: KIR's `Expr` is a tree with no let-binding
/// of its own, so a non-trivial subject would otherwise get re-evaluated
/// once per arm comparison — same restriction, and same rationale, as
/// `expr::struct_spread_base`'s spread-update base.
#[allow(clippy::too_many_arguments)]
fn lower_when_stmt(
    subject: &ast::SpannedExpr,
    arms: &[ast::WhenArm],
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    ret_ty: KirType,
    sink: TailSink,
    stmt_span: &Span,
) -> Result<ir::Stmt, LowerError> {
    let ast::Expr::Ident(name) = &subject.kind else {
        return Err(LowerError::unsupported(
            "`when` over a non-identifier subject (only a bare local/param scrutinee is \
             lowered today)",
            subject.span.clone(),
        ));
    };
    let local = ctx.resolve(name).ok_or_else(|| {
        LowerError::new(format!("unknown identifier `{name}`"), subject.span.clone())
    })?;
    let scrutinee_ty = ctx.locals[local].ty;

    if arms.is_empty() {
        return Err(LowerError::new(
            "`when` has no arms".to_string(),
            subject.span.clone(),
        ));
    }
    build_when_chain(
        arms,
        0,
        local,
        scrutinee_ty,
        ctx,
        lcx,
        table,
        ret_ty,
        sink,
        stmt_span,
    )
}

/// Builds the nested `if`/`else` chain for `arms[idx..]`. Exhaustiveness is
/// already proven by the checker (`keel-compiler`'s `when` exhaustiveness
/// check) — this does not re-derive that proof, so the *last* arm is always
/// lowered unconditionally: if every earlier arm's condition was false,
/// exhaustiveness guarantees the last arm is the only remaining possibility,
/// whether or not it's spelled as a wildcard `_`.
#[allow(clippy::too_many_arguments)]
fn build_when_chain(
    arms: &[ast::WhenArm],
    idx: usize,
    local: LocalId,
    scrutinee_ty: KirType,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    ret_ty: KirType,
    sink: TailSink,
    stmt_span: &Span,
) -> Result<ir::Stmt, LowerError> {
    let arm = &arms[idx];
    if let Some(guard) = &arm.guard {
        return Err(LowerError::unsupported(
            "`when` arm guard (`where`)",
            guard.span.clone(),
        ));
    }
    let then_branch = lower_block(&arm.body, ctx, lcx, table, ret_ty, sink)?;
    let is_last = idx + 1 == arms.len();
    let cond = if is_last {
        ir::Expr::ConstBool(true)
    } else {
        lower_arm_condition(
            &arm.patterns,
            local,
            scrutinee_ty,
            ctx,
            lcx,
            table,
            stmt_span,
        )?
    };
    let else_branch = if is_last {
        Vec::new()
    } else {
        vec![build_when_chain(
            arms,
            idx + 1,
            local,
            scrutinee_ty,
            ctx,
            lcx,
            table,
            ret_ty,
            sink,
            stmt_span,
        )?]
    };
    Ok(ir::Stmt::If {
        cond,
        then_branch,
        else_branch,
    })
}

/// Lowers `name = when subject { arms }` / `name: ty = when subject { arms
/// }` — issue #160's `when`-as-expression in `let` position (the shape
/// `examples/when_expression.keel`'s `grade` task uses). Desugars to a
/// declare-only `Stmt::Let { local, init: None }` (`keel-codegen`'s
/// `Stmt::Let` arm allocates storage but skips the initial store when
/// `init` is `None` — see `ir.rs`'s doc) followed by the same nested
/// `if`-chain [`lower_when_stmt`] builds for the statement form, except
/// each arm's tail value is written into `local` via [`TailSink::Assign`]
/// instead of returned.
///
/// A call-argument or other nested-sub-expression position (`f(when ...
/// {...})`, `1 + when ... {...}`) is *not* supported by this issue: KIR's
/// `Expr` is a tree with no statement-sequencing of its own, so hoisting a
/// `when`-expression out of an arbitrary nested position would need every
/// `lower_expr`/`lower_expr_expecting` call site to thread out an
/// accumulator of hoisted statements — real plumbing, out of proportion to
/// what this issue's fixtures need (`let`/`return` position only); such a
/// position still hits `lower/expr.rs`'s plain `WhenExpr` rejection.
///
/// With no annotation, the result type is inferred from the *first* arm's
/// tail value, lowered once purely to read off its type and then discarded
/// — the same "local structural inference" already used for list-literal
/// element types (`lower/expr.rs`'s module doc) — rather than consulting
/// `CheckArtifacts::expr_types` (which would need a new checker-`Ty` ->
/// `KirType` converter this crate doesn't have yet, see `lower/mod.rs`'s
/// `CheckArtifacts` doc). Every arm (including the first, lowered again for
/// real) is then coerced against that inferred type via the same
/// `TailSink::Assign` path `lower_stmt`'s `ast::Stmt::Expr` arm uses, so a
/// type mismatch between arms surfaces as an ordinary `lower_expr_expecting`
/// error, not a silent pick of one arm's type.
#[allow(clippy::too_many_arguments)]
fn lower_when_expr_let(
    name: &str,
    annotation: &Option<Node<ast::TypeExpr>>,
    subject: &ast::SpannedExpr,
    arms: &[ast::WhenArm],
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    ret_ty: KirType,
    stmt_span: &Span,
) -> Result<Vec<ir::Stmt>, LowerError> {
    if arms.is_empty() {
        return Err(LowerError::new(
            "`when` has no arms".to_string(),
            subject.span.clone(),
        ));
    }
    let result_ty = match annotation {
        Some(ann) => ty_expr_to_kir(
            ann,
            lcx.structs_by_name,
            lcx.enums_by_name,
            lcx.lists,
            lcx.nullables,
        )?,
        None => {
            let probe = lower_block(&arms[0].body, ctx, lcx, table, ret_ty, TailSink::Discard)?;
            match probe.last() {
                Some(ir::Stmt::Expr(e)) => e.ty(),
                _ => {
                    return Err(LowerError::unsupported(
                        "`when`-expression arm whose body doesn't end in a value-producing \
                         expression",
                        stmt_span.clone(),
                    ));
                }
            }
        }
    };
    let local = ctx.declare(name, result_ty);
    let declare = ir::Stmt::Let { local, init: None };
    let chain = lower_when_stmt(
        subject,
        arms,
        ctx,
        lcx,
        table,
        ret_ty,
        TailSink::Assign(local),
        stmt_span,
    )?;
    Ok(vec![declare, chain])
}

/// ORs together the per-pattern equality tests for one `when` arm (an arm
/// can list multiple comma-separated patterns, matching if any one does —
/// the parser guarantees at least one).
fn lower_arm_condition(
    patterns: &[ast::Pattern],
    local: LocalId,
    scrutinee_ty: KirType,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    stmt_span: &Span,
) -> Result<ir::Expr, LowerError> {
    let mut cond: Option<ir::Expr> = None;
    for pattern in patterns {
        let test = lower_pattern_test(pattern, local, scrutinee_ty, ctx, lcx, table, stmt_span)?;
        cond = Some(match cond {
            None => test,
            Some(prev) => ir::Expr::BinOp {
                op: ir::BinOp::Or,
                left: Box::new(prev),
                right: Box::new(test),
                ty: KirType::Bool,
            },
        });
    }
    Ok(cond.expect("the parser requires at least one pattern per `when` arm"))
}

/// Lowers one `when`-arm pattern to a `bool` equality test against the
/// scrutinee local. `Pattern::Ident`/`Wildcard`/`Variant`/`Struct` carry no
/// span of their own (see `keel-syntax`'s `ast::stmt::Pattern`) — diagnostics
/// for them fall back to `stmt_span` (the whole `when` statement's span).
/// Rich enum-variant/struct destructuring patterns aren't lowered yet
/// (payload extraction is a follow-up issue, same deferral as rich enum
/// construction — see `lower/mod.rs`'s enum-resolution doc).
fn lower_pattern_test(
    pattern: &ast::Pattern,
    local: LocalId,
    scrutinee_ty: KirType,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    stmt_span: &Span,
) -> Result<ir::Expr, LowerError> {
    match pattern {
        ast::Pattern::Wildcard => Ok(ir::Expr::ConstBool(true)),
        ast::Pattern::Ident(name) => {
            let KirType::Enum(enum_id) = scrutinee_ty else {
                return Err(LowerError::unsupported(
                    "identifier pattern on a non-enum scrutinee (variable-binding patterns \
                     aren't lowered yet)",
                    stmt_span.clone(),
                ));
            };
            let layout = &lcx.enum_layouts[enum_id];
            let variant_index = layout.variant_index(name).ok_or_else(|| {
                LowerError::new(
                    format!("enum `{}` has no variant `{name}`", layout.name),
                    stmt_span.clone(),
                )
            })?;
            Ok(ir::Expr::BinOp {
                op: ir::BinOp::Eq,
                left: Box::new(ir::Expr::Local {
                    id: local,
                    ty: scrutinee_ty,
                }),
                right: Box::new(ir::Expr::MakeEnum {
                    enum_id,
                    variant_index,
                }),
                ty: KirType::Bool,
            })
        }
        ast::Pattern::Literal(lit_expr) => {
            let lit = lower_expr(lit_expr, ctx, lcx, table)?;
            if lit.ty() != scrutinee_ty {
                return Err(LowerError::new(
                    format!(
                        "`when` pattern is `{}`, scrutinee is `{}`",
                        super::describe_ty(lit.ty(), lcx),
                        super::describe_ty(scrutinee_ty, lcx)
                    ),
                    lit_expr.span.clone(),
                ));
            }
            Ok(ir::Expr::BinOp {
                op: ir::BinOp::Eq,
                left: Box::new(ir::Expr::Local {
                    id: local,
                    ty: scrutinee_ty,
                }),
                right: Box::new(lit),
                ty: KirType::Bool,
            })
        }
        ast::Pattern::Variant { .. } => Err(LowerError::unsupported(
            "rich enum-variant pattern (payload destructuring lands in a later M2/M3 issue)",
            stmt_span.clone(),
        )),
        ast::Pattern::Struct { .. } => Err(LowerError::unsupported(
            "struct pattern in `when`",
            stmt_span.clone(),
        )),
    }
}

/// Lowers `raise expr` to the synthetic `UserRaised { message: str }`
/// struct's construction (reusing `Expr::MakeStruct` wholesale — see
/// `ir.rs`'s `Stmt::Raise` doc). `expr` must already be `Str`-typed; the
/// interpreter's non-`str` `Display`-coercion path (`raise 42` becomes
/// `"42"`) is a later M2/M3 concern.
fn lower_raise(
    message: &ast::SpannedExpr,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
) -> Result<ir::Stmt, LowerError> {
    let message_e = lower_expr(message, ctx, lcx, table)?;
    if message_e.ty() != KirType::Str {
        return Err(LowerError::new(
            format!(
                "`raise` of a `{}` value (only a `str` message is supported; the interpreter's \
                 non-`str` `Display`-coercion path is a later M2/M3 concern)",
                super::describe_ty(message_e.ty(), lcx)
            ),
            message.span.clone(),
        ));
    }
    let struct_id = lcx
        .user_raised_struct_id
        .expect("program_uses_raise_or_try found this Stmt::Raise, so the synthetic struct exists");
    let span = table.intern(message.span.clone());
    Ok(ir::Stmt::Raise {
        error: ir::Expr::MakeStruct {
            struct_id,
            fields: vec![message_e],
        },
        span,
    })
}

/// Lowers `try { body } catch binder: Ty { handler }`. Only a single catch
/// clause of type `Error` or `UserRaised` is supported — both bind the same
/// synthetic `UserRaised` shape, since `raise` only ever produces one (see
/// `ir.rs`'s `Stmt::TryCatch` doc); multiple clauses, or a clause over any
/// other type name, are rejected rather than silently only-partially
/// modeled.
fn lower_try_catch(
    body: &ast::Block,
    catches: &[ast::CatchClause],
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    ret_ty: KirType,
    stmt_span: &Span,
) -> Result<ir::Stmt, LowerError> {
    let [catch] = catches else {
        return Err(LowerError::unsupported(
            "multiple catch clauses (only a single `catch e: Error` or `catch e: UserRaised` \
             clause is supported until per-namespace error kinds are modeled, a later M2/M3 \
             concern)",
            stmt_span.clone(),
        ));
    };
    let ast::TypeExpr::Named(caught_name) = &catch.ty.kind else {
        return Err(LowerError::unsupported(
            "catch clause over a non-named type",
            catch.ty.span.clone(),
        ));
    };
    if caught_name != "Error" && caught_name != "UserRaised" {
        return Err(LowerError::unsupported(
            "catch clause over an error type other than `Error`/`UserRaised` (per-namespace \
             error kinds — FileError, HttpError, … — aren't modeled by the compiled backend \
             yet, a later M2/M3 concern)",
            catch.ty.span.clone(),
        ));
    }
    let struct_id = lcx.user_raised_struct_id.expect(
        "program_uses_raise_or_try found this Stmt::TryCatch, so the synthetic struct exists",
    );

    // Neither `body` nor `handler` is a tail position for this issue's
    // desugaring: `body` may raise before reaching its own tail, and
    // whether `handler` runs at all depends on that, so treating either as
    // the function's implicit return would assume more than the checker
    // itself proves. A `try`/`catch` whose own arms both explicitly
    // `return` already terminates fine — see `block_terminates`'s
    // `TryCatch` arm.
    let body = lower_block(body, ctx, lcx, table, ret_ty, TailSink::Discard)?;

    let binder_ty = KirType::Struct(struct_id);
    ctx.push_scope();
    let binder = ctx.declare(&catch.name, binder_ty);
    let handler = lower_block(&catch.body, ctx, lcx, table, ret_ty, TailSink::Discard)?;
    ctx.pop_scope();

    Ok(ir::Stmt::TryCatch {
        body,
        binder,
        binder_ty,
        handler,
    })
}

/// Whether every reachable path through `block` ends in a `return` or
/// `raise` — the KIR-level mirror of `keel-codegen`'s `block_is_terminated`
/// (an LLVM basic-block terminator check); this runs *before* codegen, over
/// the tail-desugared body (see [`lower_block`]'s `sink` doc), so
/// `lower_task_body` can reject a non-`none`-returning task whose body still
/// doesn't return on some path with a clear `LowerError` instead of letting
/// `finish_block`'s `build_unreachable` fallback silently miscompile it
/// (issue #159).
///
/// An `if` terminates when both branches do — except when its condition is
/// the literal `ConstBool(true)` [`build_when_chain`] emits for a `when`
/// statement's last arm: exhaustiveness (already proven by the checker)
/// guarantees that branch is always taken, so its `else_branch` (always
/// empty in that shape) is provably dead code, not a real unterminated
/// path. An absent `else` on a genuine `if`/`else` lowers to an empty
/// branch with a non-constant condition, which correctly still fails to
/// terminate here. A `try`/`catch` terminates only when both `body` and
/// `handler` do — unlike tail-expression desugaring, this is not a
/// tail-position rewrite, just recognizing a shape (explicit `return` in
/// both arms) that already compiles correctly today, see
/// `raise_try_catch.rs`'s `try_catch_as_the_tail_statement`-style fixtures.
/// `while`/`for` are never terminators (loops aren't exhaustive).
pub(crate) fn block_terminates(block: &Block) -> bool {
    match block.last() {
        Some(ir::Stmt::Return(_)) | Some(ir::Stmt::Raise { .. }) => true,
        Some(ir::Stmt::If {
            cond: ir::Expr::ConstBool(true),
            then_branch,
            ..
        }) => block_terminates(then_branch),
        Some(ir::Stmt::If {
            then_branch,
            else_branch,
            ..
        }) => block_terminates(then_branch) && block_terminates(else_branch),
        Some(ir::Stmt::TryCatch { body, handler, .. }) => {
            block_terminates(body) && block_terminates(handler)
        }
        _ => false,
    }
}
