//! Statement lowering — the M0 scalar subset plus M1's `for`-over-ranges
//! and M2's named-struct literals in `let`/`return` position, `when` as
//! a statement (over a simple-enum or a str/int scrutinee with a wildcard
//! arm — see [`lower_when_stmt`]), and `for x in xs` over a `list[T]`
//! (lowered to [`keel_kir::ir::Stmt::ForEach`]): `let`/assign, `if`/`else`,
//! `while`, `for x in a..b`, `for x in xs`, `return`, `when`, and bare
//! expression statements. Everything else (`try`/`catch`, `raise`, `assert`,
//! `break`/`continue`, `self.field = ...`, `for` with a `where` filter or a
//! non-range, non-list iterable) is rejected; see module docs on
//! `lower/mod.rs`.

use keel_syntax::ast::{self, Node};
use keel_syntax::lexer::Span;

use super::{FnCtx, LowerCtx, LowerError, binding_ident, ty_expr_to_kir};
use crate::ir::{self, Block, LocalId};
use crate::span_table::SpanTable;
use crate::types::KirType;

use super::expr::{lower_expr, lower_expr_expecting, struct_spread_base};

/// Lowers a `{ ... }` block in its own child scope.
pub(crate) fn lower_block(
    block: &ast::Block,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    ret_ty: KirType,
) -> Result<Block, LowerError> {
    ctx.push_scope();
    let mut out = Vec::with_capacity(block.len());
    for stmt in block {
        out.push(lower_stmt(stmt, ctx, lcx, table, ret_ty)?);
    }
    ctx.pop_scope();
    Ok(out)
}

/// Lowers a single statement. Does not open its own scope — callers that
/// need block scoping (`if`/`while` bodies) go through [`lower_block`]; the
/// synthetic top-level function lowers its statements directly into the
/// function's single root scope.
pub(crate) fn lower_stmt(
    stmt: &Node<ast::Stmt>,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    ret_ty: KirType,
) -> Result<ir::Stmt, LowerError> {
    match &stmt.kind {
        ast::Stmt::Let { binding, ty, value } => {
            let name = binding_ident(binding, &stmt.span)?;
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
            Ok(ir::Stmt::Let { local, init })
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
            Ok(ir::Stmt::Assign { local, value })
        }
        ast::Stmt::Return(None) => {
            if ret_ty != KirType::Unit {
                return Err(LowerError::new(
                    format!("bare `return` in a function declared to return `{ret_ty}`"),
                    stmt.span.clone(),
                ));
            }
            Ok(ir::Stmt::Return(None))
        }
        ast::Stmt::Return(Some(value)) => {
            let expr = lower_expr_expecting(value, ret_ty, ctx, lcx, table)?;
            Ok(ir::Stmt::Return(Some(expr)))
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
            let then_branch = lower_block(then_body, ctx, lcx, table, ret_ty)?;
            let else_branch = match else_body {
                Some(body) => lower_block(body, ctx, lcx, table, ret_ty)?,
                None => Vec::new(),
            };
            Ok(ir::Stmt::If {
                cond: cond_expr,
                then_branch,
                else_branch,
            })
        }
        ast::Stmt::While { cond, body } => {
            let cond_expr = lower_expr(cond, ctx, lcx, table)?;
            if cond_expr.ty() != KirType::Bool {
                return Err(LowerError::new(
                    format!("`while` condition is `{}`, expected `bool`", cond_expr.ty()),
                    cond.span.clone(),
                ));
            }
            let body = lower_block(body, ctx, lcx, table, ret_ty)?;
            Ok(ir::Stmt::While {
                cond: cond_expr,
                body,
            })
        }
        ast::Stmt::Expr(expr) => Ok(ir::Stmt::Expr(lower_expr(expr, ctx, lcx, table)?)),
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
                let body = lower_block(body, ctx, lcx, table, ret_ty)?;
                ctx.pop_scope();
                return Ok(ir::Stmt::ForEach {
                    var,
                    elem_ty,
                    list: iter_e,
                    body,
                });
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
            let body = lower_block(body, ctx, lcx, table, ret_ty)?;
            ctx.pop_scope();
            Ok(ir::Stmt::ForIndex {
                var,
                low,
                high,
                body,
            })
        }
        ast::Stmt::When { subject, arms } => {
            lower_when_stmt(subject, arms, ctx, lcx, table, ret_ty, &stmt.span)
        }
        ast::Stmt::TryCatch { .. } => Err(LowerError::unsupported("try/catch", stmt.span.clone())),
        ast::Stmt::Raise(_) => Err(LowerError::unsupported("raise", stmt.span.clone())),
        ast::Stmt::Assert { .. } => Err(LowerError::unsupported("assert", stmt.span.clone())),
        ast::Stmt::Break => Err(LowerError::unsupported("break", stmt.span.clone())),
        ast::Stmt::Continue => Err(LowerError::unsupported("continue", stmt.span.clone())),
    }
}

/// Lowers `when subject { arms }` used as a statement (each arm runs for its
/// side effect — `return`, a namespace call, etc. — not for a produced
/// value; `when` *as an expression*, e.g. `x = when ... { ... }`, isn't
/// lowered yet, see `lower/expr.rs`'s `WhenExpr` rejection).
///
/// `subject` must be a plain identifier (a local/param already in scope),
/// not an arbitrary expression: KIR's `Expr` is a tree with no let-binding
/// of its own, so a non-trivial subject would otherwise get re-evaluated
/// once per arm comparison — same restriction, and same rationale, as
/// `expr::struct_spread_base`'s spread-update base.
fn lower_when_stmt(
    subject: &ast::SpannedExpr,
    arms: &[ast::WhenArm],
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    ret_ty: KirType,
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
    stmt_span: &Span,
) -> Result<ir::Stmt, LowerError> {
    let arm = &arms[idx];
    if let Some(guard) = &arm.guard {
        return Err(LowerError::unsupported(
            "`when` arm guard (`where`)",
            guard.span.clone(),
        ));
    }
    let then_branch = lower_block(&arm.body, ctx, lcx, table, ret_ty)?;
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
            stmt_span,
        )?]
    };
    Ok(ir::Stmt::If {
        cond,
        then_branch,
        else_branch,
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
