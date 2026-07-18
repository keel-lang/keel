//! Expression lowering — the M0 scalar subset: literals, identifiers,
//! arithmetic/comparison/logical binary ops, unary `-`/`not`, and direct
//! calls to other lowered tasks. Everything else (field/index access,
//! method calls, casts, `if`/`when` as expressions, lambdas, compound
//! literals, string interpolation, `?.`/`??`, ranges, pipelines, duration
//! literals, enum variants) is rejected; see module docs on `lower/mod.rs`.

use std::collections::HashMap;

use keel_syntax::ast::{self, SpannedExpr};
use keel_syntax::lexer::Span;

use super::{FnCtx, FuncSig, LowerError};
use crate::ir::{self, Expr};
use crate::span_table::SpanTable;
use crate::types::KirType;

pub(crate) fn convert_binop(op: ast::BinOp) -> ir::BinOp {
    match op {
        ast::BinOp::Add => ir::BinOp::Add,
        ast::BinOp::Sub => ir::BinOp::Sub,
        ast::BinOp::Mul => ir::BinOp::Mul,
        ast::BinOp::Div => ir::BinOp::Div,
        ast::BinOp::Mod => ir::BinOp::Mod,
        ast::BinOp::Eq => ir::BinOp::Eq,
        ast::BinOp::Neq => ir::BinOp::Neq,
        ast::BinOp::Lt => ir::BinOp::Lt,
        ast::BinOp::Gt => ir::BinOp::Gt,
        ast::BinOp::Lte => ir::BinOp::Lte,
        ast::BinOp::Gte => ir::BinOp::Gte,
        ast::BinOp::And => ir::BinOp::And,
        ast::BinOp::Or => ir::BinOp::Or,
    }
}

/// Structural type inference for a binary op, given both operands' already-
/// known KIR types. Mirrors the scalar slice of the real type checker's
/// binary-op rules (`crates/keel-compiler/src/types/checker.rs`) closely
/// enough for M0; see the `#109` seam note in `lower/mod.rs` for why this is
/// local inference rather than a `CheckArtifacts` lookup.
pub(crate) fn infer_binop_ty(
    op: ir::BinOp,
    left: KirType,
    right: KirType,
    span: &Span,
) -> Result<KirType, LowerError> {
    use ir::BinOp::{Add, And, Div, Eq, Gt, Gte, Lt, Lte, Mod, Mul, Neq, Or, Sub};
    match op {
        Add if left == KirType::Str && right == KirType::Str => Ok(KirType::Str),
        Add | Sub | Mul | Div | Mod => {
            if left == right && left.is_numeric() {
                Ok(left)
            } else {
                Err(LowerError::new(
                    format!("cannot apply arithmetic to `{left}` and `{right}`"),
                    span.clone(),
                ))
            }
        }
        Eq | Neq => {
            if left == right {
                Ok(KirType::Bool)
            } else {
                Err(LowerError::new(
                    format!("cannot compare `{left}` and `{right}` for equality"),
                    span.clone(),
                ))
            }
        }
        Lt | Gt | Lte | Gte => {
            if left == right && left.is_numeric() {
                Ok(KirType::Bool)
            } else {
                Err(LowerError::new(
                    format!("cannot order-compare `{left}` and `{right}`"),
                    span.clone(),
                ))
            }
        }
        And | Or => {
            if left == KirType::Bool && right == KirType::Bool {
                Ok(KirType::Bool)
            } else {
                Err(LowerError::new(
                    format!("`and`/`or` require `bool` operands, got `{left}` and `{right}`"),
                    span.clone(),
                ))
            }
        }
    }
}

pub(crate) fn lower_expr(
    expr: &SpannedExpr,
    ctx: &mut FnCtx,
    funcs: &HashMap<String, FuncSig>,
    table: &mut SpanTable,
) -> Result<Expr, LowerError> {
    match &expr.kind {
        ast::Expr::Integer(v) => Ok(Expr::ConstInt(*v)),
        ast::Expr::Float(v) => Ok(Expr::ConstFloat(*v)),
        ast::Expr::Bool(v) => Ok(Expr::ConstBool(*v)),
        ast::Expr::StringLit(parts) => lower_string_lit(parts, &expr.span),
        ast::Expr::Ident(name) => {
            let local = ctx.resolve(name).ok_or_else(|| {
                LowerError::new(format!("unknown identifier `{name}`"), expr.span.clone())
            })?;
            let ty = ctx.locals[local].ty;
            Ok(Expr::Local { id: local, ty })
        }
        ast::Expr::BinaryOp { left, op, right } => {
            let left_e = lower_expr(left, ctx, funcs, table)?;
            let right_e = lower_expr(right, ctx, funcs, table)?;
            let kir_op = convert_binop(*op);
            let ty = infer_binop_ty(kir_op, left_e.ty(), right_e.ty(), &expr.span)?;
            Ok(Expr::BinOp {
                op: kir_op,
                left: Box::new(left_e),
                right: Box::new(right_e),
                ty,
            })
        }
        ast::Expr::UnaryOp { op, expr: operand } => {
            let operand_e = lower_expr(operand, ctx, funcs, table)?;
            let ty = match op {
                ast::UnOp::Neg if operand_e.ty().is_numeric() => operand_e.ty(),
                ast::UnOp::Not if operand_e.ty() == KirType::Bool => KirType::Bool,
                ast::UnOp::Neg => {
                    return Err(LowerError::new(
                        format!("cannot negate `{}`", operand_e.ty()),
                        expr.span.clone(),
                    ));
                }
                ast::UnOp::Not => {
                    return Err(LowerError::new(
                        format!("`not` requires `bool`, got `{}`", operand_e.ty()),
                        expr.span.clone(),
                    ));
                }
            };
            let kir_op = match op {
                ast::UnOp::Neg => ir::UnOp::Neg,
                ast::UnOp::Not => ir::UnOp::Not,
            };
            Ok(Expr::UnOp {
                op: kir_op,
                operand: Box::new(operand_e),
                ty,
            })
        }
        ast::Expr::Call { callee, args } => lower_call(callee, args, ctx, funcs, table, &expr.span),
        ast::Expr::FieldAccess(..) => {
            Err(LowerError::unsupported("field access", expr.span.clone()))
        }
        ast::Expr::NullFieldAccess(..) => Err(LowerError::unsupported(
            "null-safe field access",
            expr.span.clone(),
        )),
        ast::Expr::NullAssert(_) => Err(LowerError::unsupported("null-assert", expr.span.clone())),
        ast::Expr::SelfAccess { .. } => {
            Err(LowerError::unsupported("self access", expr.span.clone()))
        }
        ast::Expr::SelfRef => Err(LowerError::unsupported("self reference", expr.span.clone())),
        ast::Expr::StructLit(_) => {
            Err(LowerError::unsupported("struct literal", expr.span.clone()))
        }
        ast::Expr::StructSpreadUpdate { .. } => Err(LowerError::unsupported(
            "struct spread update",
            expr.span.clone(),
        )),
        ast::Expr::ListLit(_) => Err(LowerError::unsupported("list literal", expr.span.clone())),
        ast::Expr::SetLit(_) => Err(LowerError::unsupported("set literal", expr.span.clone())),
        ast::Expr::TupleLit(_) => Err(LowerError::unsupported("tuple literal", expr.span.clone())),
        ast::Expr::NullCoalesce(..) => Err(LowerError::unsupported("`??`", expr.span.clone())),
        ast::Expr::Pipeline(..) => Err(LowerError::unsupported("`|>` pipeline", expr.span.clone())),
        ast::Expr::Range(..) => Err(LowerError::unsupported("range", expr.span.clone())),
        ast::Expr::MethodCall { .. } => {
            Err(LowerError::unsupported("method call", expr.span.clone()))
        }
        ast::Expr::Cast { .. } => Err(LowerError::unsupported("`as` cast", expr.span.clone())),
        ast::Expr::IfExpr { .. } => Err(LowerError::unsupported(
            "`if` expression",
            expr.span.clone(),
        )),
        ast::Expr::WhenExpr { .. } => Err(LowerError::unsupported(
            "`when` expression",
            expr.span.clone(),
        )),
        ast::Expr::Lambda { .. } => Err(LowerError::unsupported("lambda", expr.span.clone())),
        ast::Expr::Index { .. } => Err(LowerError::unsupported("index access", expr.span.clone())),
        ast::Expr::Duration { .. } => Err(LowerError::unsupported(
            "duration literal",
            expr.span.clone(),
        )),
        ast::Expr::EnumVariant { .. } => {
            Err(LowerError::unsupported("enum variant", expr.span.clone()))
        }
        ast::Expr::None_ => Err(LowerError::unsupported("`none`", expr.span.clone())),
    }
}

/// String interpolation is sugar (desugars to concat calls per §2.3) and is
/// deferred past M0; only a single non-interpolated literal segment lowers.
fn lower_string_lit(parts: &[ast::StringPart], span: &Span) -> Result<Expr, LowerError> {
    match parts {
        [ast::StringPart::Literal(s)] => Ok(Expr::ConstStr(s.clone())),
        [] => Ok(Expr::ConstStr(String::new())),
        _ => Err(LowerError::unsupported(
            "string interpolation",
            span.clone(),
        )),
    }
}

fn lower_call(
    callee: &SpannedExpr,
    args: &[ast::CallArg],
    ctx: &mut FnCtx,
    funcs: &HashMap<String, FuncSig>,
    table: &mut SpanTable,
    call_span: &Span,
) -> Result<Expr, LowerError> {
    let ast::Expr::Ident(name) = &callee.kind else {
        return Err(LowerError::unsupported(
            "indirect/method call (only direct calls to named tasks are supported)",
            callee.span.clone(),
        ));
    };
    let sig = funcs.get(name).ok_or_else(|| {
        LowerError::new(format!("unknown function `{name}`"), callee.span.clone())
    })?;

    if args.len() != sig.params.len() {
        return Err(LowerError::new(
            format!(
                "`{name}` takes {} argument(s), got {}",
                sig.params.len(),
                args.len()
            ),
            call_span.clone(),
        ));
    }

    let mut lowered_args = Vec::with_capacity(args.len());
    for (arg, expected_ty) in args.iter().zip(&sig.params) {
        if arg.name.is_some() || arg.spread {
            return Err(LowerError::unsupported(
                "named or spread call arguments",
                arg.value.span.clone(),
            ));
        }
        let lowered = lower_expr(&arg.value, ctx, funcs, table)?;
        if lowered.ty() != *expected_ty {
            return Err(LowerError::new(
                format!(
                    "argument to `{name}` is `{}`, expected `{expected_ty}`",
                    lowered.ty()
                ),
                arg.value.span.clone(),
            ));
        }
        lowered_args.push(lowered);
    }

    Ok(Expr::Call {
        target: sig.func_id,
        args: lowered_args,
        ty: sig.ret,
        span: table.intern(call_span.clone()),
    })
}
