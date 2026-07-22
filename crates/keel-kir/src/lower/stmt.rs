//! Statement lowering — the M0 scalar subset plus M1's `for`-over-ranges:
//! `let`/assign, `if`/`else`, `while`, `for x in a..b`, `return`, and bare
//! expression statements. Everything else (`when`, `try`/`catch`, `raise`,
//! `assert`, `break`/`continue`, `self.field = ...`, `for` with a `where`
//! filter or a non-range iterable) is rejected; see module docs on
//! `lower/mod.rs`.

use std::collections::HashMap;

use keel_compiler::types::artifacts::CheckArtifacts;
use keel_syntax::ast::{self, Node};

use super::{FnCtx, FuncSig, LowerError, binding_ident, ty_expr_to_kir};
use crate::ir::{self, Block};
use crate::span_table::SpanTable;
use crate::types::KirType;

use super::expr::lower_expr;

/// Lowers a `{ ... }` block in its own child scope.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_block(
    block: &ast::Block,
    ctx: &mut FnCtx,
    funcs: &HashMap<String, FuncSig>,
    ns_bindings: &HashMap<String, String>,
    table: &mut SpanTable,
    ret_ty: KirType,
    artifacts: &CheckArtifacts,
) -> Result<Block, LowerError> {
    ctx.push_scope();
    let mut out = Vec::with_capacity(block.len());
    for stmt in block {
        out.push(lower_stmt(
            stmt,
            ctx,
            funcs,
            ns_bindings,
            table,
            ret_ty,
            artifacts,
        )?);
    }
    ctx.pop_scope();
    Ok(out)
}

/// Lowers a single statement. Does not open its own scope — callers that
/// need block scoping (`if`/`while` bodies) go through [`lower_block`]; the
/// synthetic top-level function lowers its statements directly into the
/// function's single root scope.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_stmt(
    stmt: &Node<ast::Stmt>,
    ctx: &mut FnCtx,
    funcs: &HashMap<String, FuncSig>,
    ns_bindings: &HashMap<String, String>,
    table: &mut SpanTable,
    ret_ty: KirType,
    artifacts: &CheckArtifacts,
) -> Result<ir::Stmt, LowerError> {
    match &stmt.kind {
        ast::Stmt::Let { binding, ty, value } => {
            let name = binding_ident(binding, &stmt.span)?;
            let init = lower_expr(value, ctx, funcs, ns_bindings, table, artifacts)?;
            let declared_ty = init.ty();
            if let Some(annotation) = ty {
                let annotated = ty_expr_to_kir(annotation)?;
                if annotated != declared_ty {
                    return Err(LowerError::new(
                        format!(
                            "`{name}` is annotated `{annotated}` but the initializer is `{declared_ty}`"
                        ),
                        annotation.span.clone(),
                    ));
                }
            }
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
            let rhs_expr = lower_expr(rhs, ctx, funcs, ns_bindings, table, artifacts)?;
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
            let expr = lower_expr(value, ctx, funcs, ns_bindings, table, artifacts)?;
            if expr.ty() != ret_ty {
                return Err(LowerError::new(
                    format!(
                        "`return` value is `{}` but the function returns `{ret_ty}`",
                        expr.ty()
                    ),
                    value.span.clone(),
                ));
            }
            Ok(ir::Stmt::Return(Some(expr)))
        }
        ast::Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            let cond_expr = lower_expr(cond, ctx, funcs, ns_bindings, table, artifacts)?;
            if cond_expr.ty() != KirType::Bool {
                return Err(LowerError::new(
                    format!("`if` condition is `{}`, expected `bool`", cond_expr.ty()),
                    cond.span.clone(),
                ));
            }
            let then_branch =
                lower_block(then_body, ctx, funcs, ns_bindings, table, ret_ty, artifacts)?;
            let else_branch = match else_body {
                Some(body) => lower_block(body, ctx, funcs, ns_bindings, table, ret_ty, artifacts)?,
                None => Vec::new(),
            };
            Ok(ir::Stmt::If {
                cond: cond_expr,
                then_branch,
                else_branch,
            })
        }
        ast::Stmt::While { cond, body } => {
            let cond_expr = lower_expr(cond, ctx, funcs, ns_bindings, table, artifacts)?;
            if cond_expr.ty() != KirType::Bool {
                return Err(LowerError::new(
                    format!("`while` condition is `{}`, expected `bool`", cond_expr.ty()),
                    cond.span.clone(),
                ));
            }
            let body = lower_block(body, ctx, funcs, ns_bindings, table, ret_ty, artifacts)?;
            Ok(ir::Stmt::While {
                cond: cond_expr,
                body,
            })
        }
        ast::Stmt::Expr(expr) => Ok(ir::Stmt::Expr(lower_expr(
            expr,
            ctx,
            funcs,
            ns_bindings,
            table,
            artifacts,
        )?)),
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
                return Err(LowerError::unsupported(
                    "`for` over a non-range iterable (container ABI lands in M2)",
                    iter.span.clone(),
                ));
            };
            // Bounds are evaluated once, before `var` exists, so they lower
            // in the enclosing scope and cannot reference the loop variable.
            let low = lower_expr(start, ctx, funcs, ns_bindings, table, artifacts)?;
            if low.ty() != KirType::I64 {
                return Err(LowerError::new(
                    format!("`for` range start is `{}`, expected `int`", low.ty()),
                    start.span.clone(),
                ));
            }
            let high = lower_expr(end, ctx, funcs, ns_bindings, table, artifacts)?;
            if high.ty() != KirType::I64 {
                return Err(LowerError::new(
                    format!("`for` range end is `{}`, expected `int`", high.ty()),
                    end.span.clone(),
                ));
            }
            ctx.push_scope();
            let var = ctx.declare(name, KirType::I64);
            let body = lower_block(body, ctx, funcs, ns_bindings, table, ret_ty, artifacts)?;
            ctx.pop_scope();
            Ok(ir::Stmt::ForIndex {
                var,
                low,
                high,
                body,
            })
        }
        ast::Stmt::When { .. } => Err(LowerError::unsupported("when statement", stmt.span.clone())),
        ast::Stmt::TryCatch { .. } => Err(LowerError::unsupported("try/catch", stmt.span.clone())),
        ast::Stmt::Raise(_) => Err(LowerError::unsupported("raise", stmt.span.clone())),
        ast::Stmt::Assert { .. } => Err(LowerError::unsupported("assert", stmt.span.clone())),
        ast::Stmt::Break => Err(LowerError::unsupported("break", stmt.span.clone())),
        ast::Stmt::Continue => Err(LowerError::unsupported("continue", stmt.span.clone())),
    }
}
