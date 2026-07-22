//! Expression lowering — the M0 scalar subset (literals, identifiers,
//! arithmetic/comparison/logical binary ops, unary `-`/`not`, and direct
//! calls to other lowered tasks) plus M1's stdlib namespace calls
//! (`io.show(...)`, `log.info(...)` — see [`lower_call`]). Everything else
//! (field/index access, value method calls, casts, `if`/`when` as
//! expressions, lambdas, compound literals, string interpolation, `?.`/`??`,
//! pipelines, duration literals, enum variants) is rejected; see module docs
//! on `lower/mod.rs`.

use std::collections::HashMap;

use keel_compiler::types::artifacts::CheckArtifacts;
use keel_syntax::ast::{self, SpannedExpr};
use keel_syntax::lexer::Span;

use keel_catalog::builtins::BuiltinResult;

use super::{FnCtx, FuncSig, LowerError};
use crate::ir::{self, CallTarget, Expr};
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
/// enough for M0/M1; see the `CheckArtifacts` note in `lower/mod.rs` for why
/// this stays local structural inference rather than a `CheckArtifacts`
/// lookup.
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
    ns_bindings: &HashMap<String, String>,
    table: &mut SpanTable,
    artifacts: &CheckArtifacts,
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
            let left_e = lower_expr(left, ctx, funcs, ns_bindings, table, artifacts)?;
            let right_e = lower_expr(right, ctx, funcs, ns_bindings, table, artifacts)?;
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
            let operand_e = lower_expr(operand, ctx, funcs, ns_bindings, table, artifacts)?;
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
        ast::Expr::Call { callee, args } => lower_call(
            callee,
            args,
            ctx,
            funcs,
            ns_bindings,
            table,
            artifacts,
            &expr.span,
        ),
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
        ast::Expr::MethodCall {
            object,
            method,
            args,
        } => {
            // Namespace call (`io.show(...)`): recognized only when `object`
            // is a bare identifier bound by `use std/<name>` *and* not
            // shadowed by a local — mirrors the checker's "lexical locals
            // shadow globals" precedence (`db = db.connect(...)` rebinds
            // `db` to a connection value; see `checker/expr.rs`). Anything
            // else (a real value method call, `xs.map(f)`) isn't lowered
            // yet — value methods land alongside the container ABI (M2+).
            if let ast::Expr::Ident(obj_name) = &object.kind
                && ctx.resolve(obj_name).is_none()
                && let Some(namespace) = ns_bindings.get(obj_name)
            {
                lower_ns_call(
                    namespace,
                    method,
                    args,
                    ctx,
                    funcs,
                    ns_bindings,
                    table,
                    artifacts,
                    &expr.span,
                )
            } else {
                Err(LowerError::unsupported("method call", expr.span.clone()))
            }
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

/// Lowers a direct call to another lowered task. `namespace.method(...)`
/// calls never reach here — the parser produces `ast::Expr::MethodCall` for
/// all dot-call syntax, not `Call` with a field-access callee; see the
/// `ast::Expr::MethodCall` arm in `lower_expr` for namespace-call lowering.
#[allow(clippy::too_many_arguments)]
fn lower_call(
    callee: &SpannedExpr,
    args: &[ast::CallArg],
    ctx: &mut FnCtx,
    funcs: &HashMap<String, FuncSig>,
    ns_bindings: &HashMap<String, String>,
    table: &mut SpanTable,
    artifacts: &CheckArtifacts,
    call_span: &Span,
) -> Result<Expr, LowerError> {
    let ast::Expr::Ident(name) = &callee.kind else {
        return Err(LowerError::unsupported(
            "indirect call (only direct calls to named tasks are supported)",
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
        let lowered = lower_expr(&arg.value, ctx, funcs, ns_bindings, table, artifacts)?;
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
        target: CallTarget::Fn(sig.func_id),
        args: lowered_args,
        ty: sig.ret,
        span: table.intern(call_span.clone()),
    })
}

/// Lowers a stdlib namespace call (`namespace.method(args)`) to
/// `CallTarget::Ns`. Argument *types* are not checked against the catalog's
/// declared params here (most of M1's catalog surface — including every
/// method reachable from this issue's `io`/`log` scope — takes `dynamic`,
/// which has no `KirType` until the boxing pass lands in M2/M3; `keel
/// check` already validated the call before lowering ever sees it). Arity
/// *is* checked, and named/spread arguments are rejected — no M1 namespace
/// method in the scalar-subset surface needs them yet.
///
/// `namespace`/`method`/`call_span` push this one function over clippy's
/// default argument-count threshold; the other five are the same lowering
/// state every function in this module already threads (`ctx`, `funcs`,
/// `ns_bindings`, `table`) plus the call's own arg list. Bundling all of it
/// into a shared "lowering context" struct is a reasonable future
/// refactor, but not one this issue's scope should force — see `lower_call`
/// just above, at 7 params, one under the limit, using the same pattern.
#[allow(clippy::too_many_arguments)]
fn lower_ns_call(
    namespace: &str,
    method: &str,
    args: &[ast::CallArg],
    ctx: &mut FnCtx,
    funcs: &HashMap<String, FuncSig>,
    ns_bindings: &HashMap<String, String>,
    table: &mut SpanTable,
    artifacts: &CheckArtifacts,
    call_span: &Span,
) -> Result<Expr, LowerError> {
    let builtin = keel_catalog::catalog_method(namespace, method).ok_or_else(|| {
        LowerError::new(
            format!("`{namespace}` has no method `{method}`"),
            call_span.clone(),
        )
    })?;
    let ns_id = keel_catalog::namespace_id(namespace)
        .expect("ns_bindings only ever contains namespaces validated in lower_use");

    let required = builtin.params.iter().filter(|p| !p.optional).count();
    if args.len() < required || args.len() > builtin.params.len() {
        return Err(LowerError::new(
            format!(
                "`{namespace}.{method}` takes {} argument(s), got {}",
                if required == builtin.params.len() {
                    required.to_string()
                } else {
                    format!("{required}-{}", builtin.params.len())
                },
                args.len()
            ),
            call_span.clone(),
        ));
    }

    let mut lowered_args = Vec::with_capacity(args.len());
    for arg in args {
        if arg.name.is_some() || arg.spread {
            return Err(LowerError::unsupported(
                "named or spread arguments to a stdlib namespace call",
                arg.value.span.clone(),
            ));
        }
        lowered_args.push(lower_expr(
            &arg.value,
            ctx,
            funcs,
            ns_bindings,
            table,
            artifacts,
        )?);
    }

    let ty = result_ty_to_kir(builtin.result, call_span)?;

    Ok(Expr::Call {
        target: CallTarget::Ns {
            ns_id,
            method_id: builtin.method_id,
        },
        args: lowered_args,
        ty,
        span: table.intern(call_span.clone()),
    })
}

/// Resolves a catalog method's declared result to a `KirType`, rejecting
/// anything that needs boxing/context-dependent resolution (M2/M3 scope).
fn result_ty_to_kir(result: BuiltinResult, span: &Span) -> Result<KirType, LowerError> {
    let BuiltinResult::Fixed(spec) = result else {
        return Err(LowerError::unsupported(
            "a namespace method whose result type depends on runtime context (`as:`-typed \
             extraction/classification, or otherwise dynamic — needs the boxing pass, M2/M3)",
            span.clone(),
        ));
    };
    KirType::from_tyspec(spec).ok_or_else(|| {
        LowerError::unsupported(
            &format!("a namespace method returning `{spec:?}` (needs M2+ types)"),
            span.clone(),
        )
    })
}
