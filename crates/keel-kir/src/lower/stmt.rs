//! Statement lowering — the M0 scalar subset plus M1's `for`-over-ranges
//! and M2's named-struct literals in `let`/`return` position, `when` as
//! a statement (over a simple-enum or a str/int scrutinee with a wildcard
//! arm, named by a bare local or produced by an arbitrary subject
//! expression — see [`lower_when_stmt`]) and *as an expression* anywhere a
//! value is expected (see [`TailSink::Assign`], [`lower_when_expr_let`] and
//! [`lower_when_expr_value`]), `if` used as an expression in those same
//! positions (issue #192 — [`lower_if_expr_let`] and [`lower_if_expr_value`]
//! over the shared [`lower_if_expr_chain`]), and `for x in xs` over a
//! `list[T]` (lowered to
//! [`keel_kir::ir::Stmt::ForEach`]): `let`/assign, `if`/`else`, `while`,
//! `for x in a..b`, `for x in xs`, `return`, `when`, and bare expression
//! statements. Everything else (`assert`, `break`/`continue`,
//! `self.field = ...`, `for` with a `where` filter or a non-range, non-list
//! iterable) is rejected; see module docs on `lower/mod.rs`. An `if` used as
//! an expression with no `else` is rejected too, but as a *diagnostic* rather
//! than an unsupported-construct notice: `SPEC.md` §8.1 calls that a compile
//! error, and this is the first engine to enforce it (see [`require_else`]).

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
    /// A `when`- or `if`-expression's result local (issues #160, #192): a
    /// bare tail expression becomes `<local> = <expr>` instead of a `return`
    /// — used while lowering each arm of a `when`, or each branch of an `if`,
    /// used as an expression, where the arm's/branch's "tail value" is the
    /// value the whole expression produces, not the enclosing function's
    /// return value.
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
        out.extend(lower_stmt(stmt, ctx, lcx, table, stmt_sink)?);
    }
    ctx.pop_scope();
    Ok(out)
}

/// Lowers a single statement, possibly to more than one [`ir::Stmt`].
///
/// Two things produce extra statements. A `let`/`return` whose value is a
/// `when`- or `if`-expression desugars to a declare-only `Let` plus the
/// `if`-chain that assigns into it, or to the chain itself (see
/// [`lower_when_expr_let`] and [`lower_if_expr_let`]). And either of those
/// in an arbitrary *nested* position (`f(when n {...})`, `f(if c {...} else
/// {...})`) hoists the same declare+chain pair through [`FnCtx::hoist`] —
/// this function installs a fresh hoist buffer per statement and emits
/// whatever landed in it ahead of the statement's own lowering, which is
/// what confines a hoist to the nearest enclosing statement instead of
/// letting it escape past an `if`/loop body.
///
/// Does not open its own scope — callers that need block scoping (`if`/
/// `while` bodies) go through [`lower_block`]; the synthetic top-level
/// function lowers its statements directly into the function's single root
/// scope.
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
    sink: TailSink,
) -> Result<Vec<ir::Stmt>, LowerError> {
    let outer = ctx.begin_hoist();
    let lowered = lower_stmt_inner(stmt, ctx, lcx, table, sink);
    let mut out = ctx.end_hoist(outer);
    out.extend(lowered?);
    Ok(out)
}

fn lower_stmt_inner(
    stmt: &Node<ast::Stmt>,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    sink: TailSink,
) -> Result<Vec<ir::Stmt>, LowerError> {
    let ret_ty = ctx.ret_ty;
    match &stmt.kind {
        ast::Stmt::Let { binding, ty, value } => {
            // `(a, b) = pair` binds N names from one subject, which
            // `binding_ident` cannot express (it returns a single name).
            if let ast::Binding::Destruct(pat) = binding {
                return lower_destructure_let(pat, ty, value, ctx, lcx, table, &stmt.span);
            }
            let name = binding_ident(binding, &stmt.span)?;
            if let ast::Expr::WhenExpr { subject, arms } = &value.kind {
                return lower_when_expr_let(name, ty, subject, arms, ctx, lcx, table, &stmt.span);
            }
            if let ast::Expr::IfExpr {
                cond,
                then_body,
                else_body,
            } = &value.kind
            {
                return lower_if_expr_let(
                    name, ty, cond, then_body, else_body, ctx, lcx, table, &stmt.span,
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
                    lcx.tuples,
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
            // `x += <rhs>` reads `x` *before* evaluating `rhs`, so the read is
            // a sibling of `rhs` in evaluation order: if `rhs` hoists (its
            // `when` arms could themselves do `x += 1`), the read has to be
            // spilled ahead of the hoisted chain rather than left to happen
            // after it.
            let mut read = [ir::Expr::Local {
                id: local,
                ty: local_ty,
            }];
            let mark = ctx.hoist_mark();
            let rhs_expr = lower_expr(rhs, ctx, lcx, table)?;
            ctx.keep_order(mark, &mut read, &stmt.span)?;
            let [read] = read;
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
                left: Box::new(read),
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
                let chain =
                    lower_when_stmt(subject, arms, ctx, lcx, table, TailSink::Return, &stmt.span)?;
                return Ok(vec![chain]);
            }
            if let ast::Expr::IfExpr {
                cond,
                then_body,
                else_body,
            } = &value.kind
            {
                // `return if cond { ... } else { ... }` — same reasoning as
                // the `when` case above: each branch's tail value returns
                // directly, so no result temp is needed.
                let chain = lower_if_expr_chain(
                    cond,
                    then_body,
                    else_body,
                    ctx,
                    lcx,
                    table,
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
            // No hoist guard on the condition: it's evaluated exactly once,
            // unconditionally, before either branch — so hoisting a `when`
            // out of it and emitting it ahead of the whole `if` preserves
            // both the order and the number of evaluations.
            let then_branch = lower_block(then_body, ctx, lcx, table, sink)?;
            let else_branch = match else_body {
                Some(body) => lower_block(body, ctx, lcx, table, sink)?,
                None => Vec::new(),
            };
            Ok(vec![ir::Stmt::If {
                cond: cond_expr,
                then_branch,
                else_branch,
            }])
        }
        ast::Stmt::While { cond, body } => {
            // Unlike an `if` condition, a `while` condition is re-evaluated
            // once per iteration — hoisting out of it would emit the chain
            // ahead of the loop and evaluate it exactly once, so it's
            // rejected rather than silently miscompiled.
            let mark = ctx.hoist_mark();
            let cond_expr = lower_expr(cond, ctx, lcx, table)?;
            ctx.forbid_hoist(
                mark,
                "a `while` condition (the condition is re-evaluated once per iteration, but \
                 the hoisted arms would run only once, ahead of the loop)",
                &cond.span,
            )?;
            if cond_expr.ty() != KirType::Bool {
                return Err(LowerError::new(
                    format!("`while` condition is `{}`, expected `bool`", cond_expr.ty()),
                    cond.span.clone(),
                ));
            }
            let body = lower_block(body, ctx, lcx, table, TailSink::Discard)?;
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
                // rejecting outright. The iterable is evaluated exactly once,
                // before the loop, so a hoist out of it is fine unguarded.
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
                let body = lower_block(body, ctx, lcx, table, TailSink::Discard)?;
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
            // They are siblings of each other, though — `for i in f()..(when
            // ...)` must not reorder the two.
            let mut low = [lower_expr(start, ctx, lcx, table)?];
            if low[0].ty() != KirType::I64 {
                return Err(LowerError::new(
                    format!("`for` range start is `{}`, expected `int`", low[0].ty()),
                    start.span.clone(),
                ));
            }
            let mark = ctx.hoist_mark();
            let high = lower_expr(end, ctx, lcx, table)?;
            ctx.keep_order(mark, &mut low, &end.span)?;
            let [low] = low;
            if high.ty() != KirType::I64 {
                return Err(LowerError::new(
                    format!("`for` range end is `{}`, expected `int`", high.ty()),
                    end.span.clone(),
                ));
            }
            ctx.push_scope();
            let var = ctx.declare(name, KirType::I64);
            let body = lower_block(body, ctx, lcx, table, TailSink::Discard)?;
            ctx.pop_scope();
            Ok(vec![ir::Stmt::ForIndex {
                var,
                low,
                high,
                body,
            }])
        }
        ast::Stmt::When { subject, arms } => Ok(vec![lower_when_stmt(
            subject, arms, ctx, lcx, table, sink, &stmt.span,
        )?]),
        ast::Stmt::TryCatch { body, catches } => Ok(vec![lower_try_catch(
            body, catches, ctx, lcx, table, &stmt.span,
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
/// The arm chain compares against a *local*, never against `subject`'s own
/// `Expr`: KIR's `Expr` is a tree with no let-binding of its own, so an
/// inline subject would be re-evaluated once per arm comparison. A bare
/// identifier already names a local, so it is used directly — no temp, no
/// copy, and an unresolvable name still reports as `unknown identifier`.
/// Any other subject expression is bound to a synthetic `<when.subject>`
/// temp pushed through [`FnCtx::hoist`] (issue #191), which runs it exactly
/// once, unconditionally, ahead of the enclosing statement — precisely the
/// contract that buffer models, and the same reason `lower_stmt`'s `if`
/// condition needs no hoist guard.
///
/// Hoisting means a `when` over a non-identifier subject inherits the
/// positional restrictions every other hoisting construct has: it is
/// rejected by [`FnCtx::forbid_hoist`] where there is no enclosing statement
/// to run ahead of (a parameter default — see `decl.rs`'s
/// `lower_param_defaults`) or where the subject would not be evaluated
/// exactly once (a `while` condition, an `and`/`or` right operand, a `??`
/// fallback, a `when` arm's own pattern test).
fn lower_when_stmt(
    subject: &ast::SpannedExpr,
    arms: &[ast::WhenArm],
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    sink: TailSink,
    stmt_span: &Span,
) -> Result<ir::Stmt, LowerError> {
    // Checked before the subject is lowered so an arm-less `when` doesn't
    // leave a `<when.subject>` `Let` in the hoist buffer on the way out.
    if arms.is_empty() {
        return Err(LowerError::new(
            "`when` has no arms".to_string(),
            subject.span.clone(),
        ));
    }
    let (local, scrutinee_ty) = match &subject.kind {
        ast::Expr::Ident(name) => {
            let local = ctx.resolve(name).ok_or_else(|| {
                LowerError::new(format!("unknown identifier `{name}`"), subject.span.clone())
            })?;
            (local, ctx.locals[local].ty)
        }
        _ => {
            let value = lower_expr(subject, ctx, lcx, table)?;
            let ty = value.ty();
            let local = ctx.declare_temp("<when.subject>", ty);
            ctx.hoist(ir::Stmt::Let {
                local,
                init: Some(value),
            });
            (local, ty)
        }
    };
    build_when_chain(
        arms,
        0,
        local,
        scrutinee_ty,
        ctx,
        lcx,
        table,
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
    let then_branch = lower_block(&arm.body, ctx, lcx, table, sink)?;
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
/// A `when`-expression in an arbitrary *nested* position (`f(when ...
/// {...})`, `1 + when ... {...}`) builds the same declare+chain pair, but
/// binds it to a synthetic temp and hoists it — see
/// [`lower_when_expr_value`]. This function stays separate because it can
/// bind the user's own name directly, with no temp and no copy.
///
/// With no annotation, the result type is inferred from the *first* arm's
/// tail value, lowered once purely to read off its type and then discarded
/// (see [`probe_block_ty`]) — the same "local structural inference" already
/// used for list-literal element types (`lower/expr.rs`'s module doc) —
/// rather than consulting `CheckArtifacts::expr_types` (which would need a
/// new checker-`Ty` -> `KirType` converter this crate doesn't have yet, see
/// `lower/mod.rs`'s `CheckArtifacts` doc). Every arm (including the first,
/// lowered again for real) is then coerced against that inferred type via the
/// same `TailSink::Assign` path `lower_stmt`'s `ast::Stmt::Expr` arm uses, so
/// a type mismatch between arms surfaces as an ordinary
/// `lower_expr_expecting` error, not a silent pick of one arm's type.
#[allow(clippy::too_many_arguments)]
fn lower_when_expr_let(
    name: &str,
    annotation: &Option<Node<ast::TypeExpr>>,
    subject: &ast::SpannedExpr,
    arms: &[ast::WhenArm],
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
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
            lcx.maps,
            lcx.sets,
            lcx.nullables,
            lcx.tuples,
        )?,
        None => probe_block_ty(&arms[0].body, ctx, lcx, table, stmt_span)?,
    };
    let local = ctx.declare(name, result_ty);
    let declare = ir::Stmt::Let { local, init: None };
    let chain = lower_when_stmt(
        subject,
        arms,
        ctx,
        lcx,
        table,
        TailSink::Assign(local),
        stmt_span,
    )?;
    Ok(vec![declare, chain])
}

/// Lowers a `when`-expression used in an arbitrary nested position — a call
/// argument, a binary-op operand, a list element, an interpolation slot — to
/// the `ir::Expr` that *reads* its result, hoisting the statements that
/// produce it (issue #170).
///
/// The desugaring is exactly [`lower_when_expr_let`]'s, over a synthetic
/// `<when.result>` temp instead of a user-named local: a declare-only
/// `Stmt::Let` plus the nested `if`-chain [`lower_when_stmt`] builds, both
/// pushed through [`FnCtx::hoist`] so [`lower_stmt`] emits them ahead of the
/// enclosing statement. Nothing new reaches `keel-codegen` — these are the
/// same two statement shapes issue #160 already emits.
///
/// `expected` is the type the surrounding syntax pins, when it pins one (a
/// call argument's declared param type, a list element's type, …). With
/// `None` — a plain [`super::expr::lower_expr`] position such as a stdlib
/// namespace argument, whose catalog params are `dynamic` — the result type
/// falls back to the same discarded first-arm probe `lower_when_expr_let`
/// uses with no annotation.
pub(crate) fn lower_when_expr_value(
    subject: &ast::SpannedExpr,
    arms: &[ast::WhenArm],
    expected: Option<KirType>,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    span: &Span,
) -> Result<ir::Expr, LowerError> {
    if arms.is_empty() {
        return Err(LowerError::new(
            "`when` has no arms".to_string(),
            subject.span.clone(),
        ));
    }
    let result_ty = match expected {
        Some(ty) => ty,
        None => probe_block_ty(&arms[0].body, ctx, lcx, table, span)?,
    };
    let local = ctx.declare_temp("<when.result>", result_ty);
    ctx.hoist(ir::Stmt::Let { local, init: None });
    let chain = lower_when_stmt(
        subject,
        arms,
        ctx,
        lcx,
        table,
        TailSink::Assign(local),
        span,
    )?;
    ctx.hoist(chain);
    Ok(ir::Expr::Local {
        id: local,
        ty: result_ty,
    })
}

/// Reads off the `KirType` a `when`-expression's first arm — or an
/// `if`-expression's `then` branch — produces, by lowering that block and
/// throwing the result away.
///
/// The probe runs in its own hoist buffer and its own slice of `ctx.locals`,
/// both discarded afterwards: the block is about to be lowered *again* for
/// real, and leaving either behind would duplicate its hoisted statements or
/// leave dead locals in the function (visible in golden dumps). Truncating
/// `locals` is safe precisely because everything the probe produced is
/// dropped here — no surviving `Expr` holds one of those ids.
///
/// Probing only the *first* branch means `x = if c { return 0 } else { 1 }`
/// is rejected even though the checker accepts it (it propagates the other
/// branch's type past a `return` — see `keel-compiler`'s `IfExpr` inference).
/// That is the same pre-existing limitation this probe has always had for
/// `when`, and it only bites where no type is pinned: an annotation
/// (`x: int = ...`) or any position that calls `lower_expr_expecting` skips
/// the probe entirely.
fn probe_block_ty(
    body: &ast::Block,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    span: &Span,
) -> Result<KirType, LowerError> {
    let outer = ctx.begin_hoist();
    let locals_mark = ctx.locals.len();
    let probe = lower_block(body, ctx, lcx, table, TailSink::Discard);
    ctx.end_hoist(outer);
    ctx.locals.truncate(locals_mark);
    match probe?.last() {
        Some(ir::Stmt::Expr(e)) => Ok(e.ty()),
        _ => Err(LowerError::unsupported(
            "an unannotated `when`/`if` expression whose first branch doesn't end in a \
             value-producing expression",
            span.clone(),
        )),
    }
}

/// Rejects a one-armed `if` used where a value is expected — which `SPEC.md`
/// §8.1 already calls a compile error ("An `if` without `else` used as an
/// expression is a compile error").
///
/// Nothing enforced that rule before this. The parser admits the form
/// (`else_body` defaults to an empty block — see `keel-syntax`'s `if_expr`),
/// the checker types the whole expression as the `then` branch's type, and
/// the interpreter yields `none` on the false path. This is the first engine
/// to hold the line, so a program rejected here still runs under `keel run`
/// until the checker catches up.
fn require_else(else_body: &ast::Block, span: &Span) -> Result<(), LowerError> {
    if else_body.is_empty() {
        return Err(LowerError::new(
            "`if` used as an expression needs an `else` branch (there is no value to produce \
             when the condition is false)"
                .to_string(),
            span.clone(),
        ));
    }
    Ok(())
}

/// Builds the `ir::Stmt::If` backing an `if` used as an *expression* — the
/// same shape `lower_stmt`'s `ast::Stmt::If` arm emits for the statement
/// form, except each branch's tail value desugars according to `sink` (see
/// [`TailSink`]), exactly like a `when` arm's body in [`build_when_chain`].
/// Nothing new reaches `keel-codegen`.
///
/// An `else if` chain arrives here as an `else_body` holding the single
/// statement `Stmt::Expr(Expr::IfExpr { .. })` — that is how the parser
/// spells its own recursion — so it needs no special case: the block's tail
/// position carries `sink` into the nested `if` through the ordinary
/// [`lower_block`] path. In a value position that costs one `<if.result>`
/// temp per `else if` level, which keeps this function ignorant of chaining.
#[allow(clippy::too_many_arguments)]
fn lower_if_expr_chain(
    cond: &ast::SpannedExpr,
    then_body: &ast::Block,
    else_body: &ast::Block,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    sink: TailSink,
    span: &Span,
) -> Result<ir::Stmt, LowerError> {
    require_else(else_body, span)?;
    // No hoist guard on the condition, for the same reason the statement form
    // needs none: it is evaluated exactly once, unconditionally, before either
    // branch, so anything hoisted out of it still runs exactly once and in
    // order ahead of the enclosing statement.
    let cond_expr = lower_expr(cond, ctx, lcx, table)?;
    if cond_expr.ty() != KirType::Bool {
        return Err(LowerError::new(
            format!("`if` condition is `{}`, expected `bool`", cond_expr.ty()),
            cond.span.clone(),
        ));
    }
    let then_branch = lower_block(then_body, ctx, lcx, table, sink)?;
    let else_branch = lower_block(else_body, ctx, lcx, table, sink)?;
    Ok(ir::Stmt::If {
        cond: cond_expr,
        then_branch,
        else_branch,
    })
}

/// Lowers `name = if cond { ... } else { ... }` / `name: ty = ...` — issue
/// #192's `if`-as-expression in `let` position, the sibling of
/// [`lower_when_expr_let`] and desugared identically: a declare-only
/// `Stmt::Let { local, init: None }` followed by the `if` itself, with each
/// branch's tail value written into `local` via [`TailSink::Assign`].
///
/// As with `when`, the result type comes from an explicit annotation when
/// there is one and otherwise from a discarded probe of the `then` branch
/// (see [`probe_block_ty`]); every branch is then coerced against it through
/// the same `lower_expr_expecting` path, so a mismatch between branches
/// surfaces as an ordinary type error rather than a silent pick of one side.
#[allow(clippy::too_many_arguments)]
fn lower_if_expr_let(
    name: &str,
    annotation: &Option<Node<ast::TypeExpr>>,
    cond: &ast::SpannedExpr,
    then_body: &ast::Block,
    else_body: &ast::Block,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    stmt_span: &Span,
) -> Result<Vec<ir::Stmt>, LowerError> {
    // Ahead of the probe: a missing `else` should report as a missing `else`,
    // not as an empty block that produced no value to read a type from.
    require_else(else_body, stmt_span)?;
    let result_ty = match annotation {
        Some(ann) => ty_expr_to_kir(
            ann,
            lcx.structs_by_name,
            lcx.enums_by_name,
            lcx.lists,
            lcx.maps,
            lcx.sets,
            lcx.nullables,
            lcx.tuples,
        )?,
        None => probe_block_ty(then_body, ctx, lcx, table, stmt_span)?,
    };
    let local = ctx.declare(name, result_ty);
    let declare = ir::Stmt::Let { local, init: None };
    let chain = lower_if_expr_chain(
        cond,
        then_body,
        else_body,
        ctx,
        lcx,
        table,
        TailSink::Assign(local),
        stmt_span,
    )?;
    Ok(vec![declare, chain])
}

/// Lowers an `if`-expression used in an arbitrary nested position — a call
/// argument, a binary-op operand, an interpolation slot — to the `ir::Expr`
/// that *reads* its result, hoisting the statements that produce it. The
/// exact shape of [`lower_when_expr_value`], over an `<if.result>` temp.
///
/// Because it hoists, an `if`-expression inherits every positional
/// restriction the hoist buffer already enforces — [`FnCtx::forbid_hoist`]
/// rejects it in a `while` condition, an `and`/`or` right operand, a `??`
/// fallback, a `when` arm's pattern test, and a parameter default. Those
/// guards key off the buffer growing, not off any particular syntax, so they
/// needed no change to cover this construct.
///
/// `expected` is the type the surrounding syntax pins, when it pins one; with
/// `None` the result type falls back to the same discarded `then`-branch
/// probe [`lower_if_expr_let`] uses without an annotation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_if_expr_value(
    cond: &ast::SpannedExpr,
    then_body: &ast::Block,
    else_body: &ast::Block,
    expected: Option<KirType>,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    span: &Span,
) -> Result<ir::Expr, LowerError> {
    require_else(else_body, span)?;
    let result_ty = match expected {
        Some(ty) => ty,
        None => probe_block_ty(then_body, ctx, lcx, table, span)?,
    };
    let local = ctx.declare_temp("<if.result>", result_ty);
    ctx.hoist(ir::Stmt::Let { local, init: None });
    let chain = lower_if_expr_chain(
        cond,
        then_body,
        else_body,
        ctx,
        lcx,
        table,
        TailSink::Assign(local),
        span,
    )?;
    ctx.hoist(chain);
    Ok(ir::Expr::Local {
        id: local,
        ty: result_ty,
    })
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
        // An arm's own pattern test sits inside the `else` chain, so it runs
        // only when every earlier arm failed to match — hoisting out of one
        // would move it ahead of the whole `when`, running it unconditionally.
        // The parser only admits literal patterns here, so this is a guard
        // against a future pattern form arriving silently, not a diagnostic
        // any program reaches today.
        let mark = ctx.hoist_mark();
        let test = lower_pattern_test(pattern, local, scrutinee_ty, ctx, lcx, table, stmt_span)?;
        ctx.forbid_hoist(mark, "a `when` arm pattern", stmt_span)?;
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
    let body = lower_block(body, ctx, lcx, table, TailSink::Discard)?;

    let binder_ty = KirType::Struct(struct_id);
    ctx.push_scope();
    let binder = ctx.declare(&catch.name, binder_ty);
    let handler = lower_block(&catch.body, ctx, lcx, table, TailSink::Discard)?;
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

/// Lowers `(a, b) = pair` — a positional tuple destructure — into a subject
/// local plus one `TupleGet` binding per name.
///
/// The subject is bound to its own synthetic local first so the right-hand
/// side is evaluated exactly once, however many names it feeds; reading
/// `pair.0` and `pair.1` off a re-lowered RHS would duplicate any work (or
/// any `raise`) inside it.
///
/// Struct destructuring (`{a, b} = value`) stays unsupported: it needs field-
/// name resolution against a `StructLayout` and has no tuple fixture driving
/// it, so it keeps the pre-existing "destructuring binding" error rather than
/// a half-built path.
fn lower_destructure_let(
    pat: &ast::DestructPat,
    annotation: &Option<Node<ast::TypeExpr>>,
    value: &ast::SpannedExpr,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    span: &Span,
) -> Result<Vec<ir::Stmt>, LowerError> {
    let ast::DestructPat::Tuple(names) = pat else {
        return Err(LowerError::unsupported(
            "struct destructuring binding",
            span.clone(),
        ));
    };

    let init = if let Some(annotation) = annotation {
        let expected = ty_expr_to_kir(
            annotation,
            lcx.structs_by_name,
            lcx.enums_by_name,
            lcx.lists,
            lcx.maps,
            lcx.sets,
            lcx.nullables,
            lcx.tuples,
        )?;
        lower_expr_expecting(value, expected, ctx, lcx, table)?
    } else {
        lower_expr(value, ctx, lcx, table)?
    };

    let subject_ty = init.ty();
    let KirType::Tuple(tuple_id) = subject_ty else {
        return Err(LowerError::unsupported(
            "positional destructuring of a non-tuple value (list/struct destructuring is a \
             later-M2/M3 concern)",
            span.clone(),
        ));
    };
    let elems = lcx.tuples.borrow()[tuple_id].elems.clone();
    if elems.len() != names.len() {
        return Err(LowerError::new(
            format!(
                "destructuring binds {} name(s) but the tuple has {} element(s) — the checker \
                 should have rejected this",
                names.len(),
                elems.len()
            ),
            span.clone(),
        ));
    }

    // Declared before the element locals so each `TupleGet` can reference it.
    let subject_local = ctx.declare("<destructure.subject>", subject_ty);
    let mut stmts = vec![ir::Stmt::Let {
        local: subject_local,
        init: Some(init),
    }];
    for (index, (name, elem_ty)) in names.iter().zip(&elems).enumerate() {
        let local = ctx.declare(name, *elem_ty);
        stmts.push(ir::Stmt::Let {
            local,
            init: Some(ir::Expr::TupleGet {
                base: Box::new(ir::Expr::Local {
                    id: subject_local,
                    ty: subject_ty,
                }),
                index,
                ty: *elem_ty,
            }),
        });
    }
    Ok(stmts)
}
