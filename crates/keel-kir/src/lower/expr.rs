//! Expression lowering — the M0 scalar subset (literals, identifiers,
//! arithmetic/comparison/logical binary ops, unary `-`/`not`, and direct
//! calls to other lowered tasks) plus M1's stdlib namespace calls
//! (`io.show(...)`, `log.info(...)` — see [`lower_call`]), M2's named-
//! struct literals, spread-update, field access, simple-enum variant
//! construction (`Priority.low`), and `list[T]` literals/`push`/`len`/
//! indexing (`T` restricted to int/float/bool/str — see [`lower_list_lit`]).
//! Everything else (casts, `if`/`when` as expressions, lambdas, set/tuple
//! literals, string interpolation, `?.`/`??`, pipelines, duration literals,
//! rich (payload-carrying) enum variants, non-list method calls/indexing)
//! is rejected; see module docs on `lower/mod.rs`.

use std::collections::HashMap;

use keel_syntax::ast::{self, SpannedExpr};
use keel_syntax::lexer::Span;

use keel_catalog::builtins::BuiltinResult;

use super::{FnCtx, LowerCtx, LowerError, describe_ty};
use crate::ir::{self, CallTarget, Expr, StructId};
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
    lcx: &LowerCtx<'_>,
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
            let left_e = lower_expr(left, ctx, lcx, table)?;
            let right_e = lower_expr(right, ctx, lcx, table)?;
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
            let operand_e = lower_expr(operand, ctx, lcx, table)?;
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
        ast::Expr::Call { callee, args } => lower_call(callee, args, ctx, lcx, table, &expr.span),
        ast::Expr::FieldAccess(base, field) => {
            // Enum-variant construction (`Priority.low`): unlike struct field
            // access, `base` here is a *type name*, not a value — mirrors
            // the checker's "lexical locals shadow globals" precedence
            // (`NameKind::Enum` short-circuit in `checker/expr.rs`) so a
            // local that happens to shadow an enum's name still resolves as
            // a value, not a variant.
            if let ast::Expr::Ident(name) = &base.kind
                && ctx.resolve(name).is_none()
                && let Some(enum_id) = lcx.enums_by_name.get(name).copied()
            {
                let layout = &lcx.enum_layouts[enum_id];
                let variant_index = layout.variant_index(field).ok_or_else(|| {
                    LowerError::new(
                        format!("enum `{}` has no variant `{field}`", layout.name),
                        expr.span.clone(),
                    )
                })?;
                return Ok(Expr::MakeEnum {
                    enum_id,
                    variant_index,
                });
            }

            let base_e = lower_expr(base, ctx, lcx, table)?;
            let KirType::Struct(struct_id) = base_e.ty() else {
                return Err(LowerError::unsupported(
                    "field access on a non-struct value (containers/dynamic land in a later \
                     M2 issue)",
                    expr.span.clone(),
                ));
            };
            let layout = &lcx.struct_layouts[struct_id];
            let field_index = layout.field_index(field).ok_or_else(|| {
                LowerError::new(
                    format!("struct `{}` has no field `{field}`", layout.name),
                    expr.span.clone(),
                )
            })?;
            let ty = layout.fields[field_index].1;
            Ok(Expr::FieldGet {
                base: Box::new(base_e),
                field_index,
                ty,
            })
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
        // No expected-type context here (that's `lower_expr_expecting`'s
        // job) — an anonymous struct literal with nothing pinning it to a
        // named struct isn't modeled yet (deferred until an M2 fixture
        // needs one; see `ir.rs`'s `StructLayout` doc).
        ast::Expr::StructLit(_) => Err(LowerError::unsupported(
            "struct literal outside a known-struct-typed position (a `let` annotation, \
             `return`, or call argument)",
            expr.span.clone(),
        )),
        ast::Expr::StructSpreadUpdate { .. } => Err(LowerError::unsupported(
            "struct spread-update outside a known-struct-typed position (a `let` annotation, \
             `return`, or call argument)",
            expr.span.clone(),
        )),
        ast::Expr::ListLit(items) => lower_list_lit(items, &expr.span, ctx, lcx, table),
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
                && let Some(namespace) = lcx.ns_bindings.get(obj_name)
            {
                return lower_ns_call(namespace, method, args, ctx, lcx, table, &expr.span);
            }
            let object_e = lower_expr(object, ctx, lcx, table)?;
            match object_e.ty() {
                KirType::List(_) => {
                    lower_list_method_call(object_e, method, args, ctx, lcx, table, &expr.span)
                }
                _ => Err(LowerError::unsupported("method call", expr.span.clone())),
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
        ast::Expr::Index { object, index } => {
            let object_e = lower_expr(object, ctx, lcx, table)?;
            let KirType::List(list_id) = object_e.ty() else {
                return Err(LowerError::unsupported(
                    "index access on a non-list value (strings/maps land in a later M2 issue)",
                    expr.span.clone(),
                ));
            };
            let index_e = lower_expr(index, ctx, lcx, table)?;
            if index_e.ty() != KirType::I64 {
                return Err(LowerError::new(
                    format!(
                        "list index is `{}`, expected `int`",
                        describe_ty(index_e.ty(), lcx)
                    ),
                    index.span.clone(),
                ));
            }
            let elem_ty = lcx.lists.borrow()[list_id];
            Ok(Expr::Index {
                list: Box::new(object_e),
                index: Box::new(index_e),
                ty: elem_ty,
            })
        }
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

/// Lowers `expr` against a known expected type — used only where the
/// surrounding syntax pins a concrete type (a `let` annotation, `return`, a
/// call argument), so a struct literal or spread-update (which the checker
/// always types as an unnamed structural shape — see `lower/mod.rs`'s
/// `CheckArtifacts` doc) can resolve to the *named* struct that context
/// calls for. Falls through to ordinary bottom-up [`lower_expr`] for every
/// other expression kind, then checks its type matches.
pub(crate) fn lower_expr_expecting(
    expr: &SpannedExpr,
    expected: KirType,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
) -> Result<Expr, LowerError> {
    if let KirType::Struct(struct_id) = expected {
        match &expr.kind {
            ast::Expr::StructLit(fields) => {
                return lower_struct_lit(fields, struct_id, &expr.span, ctx, lcx, table);
            }
            ast::Expr::StructSpreadUpdate { base, overrides } => {
                return lower_struct_spread(base, overrides, struct_id, ctx, lcx, table);
            }
            _ => {}
        }
    }
    let lowered = lower_expr(expr, ctx, lcx, table)?;
    if lowered.ty() != expected {
        return Err(LowerError::new(
            format!(
                "expected `{}`, got `{}`",
                describe_ty(expected, lcx),
                describe_ty(lowered.ty(), lcx)
            ),
            expr.span.clone(),
        ));
    }
    Ok(lowered)
}

/// Lowers a struct literal (`{field: value, ...}`) against its expected
/// struct type — matches fields by *name*, not literal source order
/// (mirrors the checker's structural assignability rule,
/// `crates/keel-compiler/src/types/checker.rs`), and rebuilds them in the
/// struct's declared field order.
fn lower_struct_lit(
    lit_fields: &[(ast::MapLitKey, SpannedExpr)],
    struct_id: StructId,
    span: &Span,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
) -> Result<Expr, LowerError> {
    let layout = &lcx.struct_layouts[struct_id];

    let mut by_name: HashMap<&str, &SpannedExpr> = HashMap::with_capacity(lit_fields.len());
    for (key, value) in lit_fields {
        let name = key.as_str().ok_or_else(|| {
            LowerError::unsupported("non-identifier struct-literal key", value.span.clone())
        })?;
        if by_name.insert(name, value).is_some() {
            return Err(LowerError::new(
                format!("duplicate field `{name}` in struct literal"),
                value.span.clone(),
            ));
        }
    }

    let mut fields = Vec::with_capacity(layout.fields.len());
    for (field_name, field_ty) in &layout.fields {
        let value = by_name.remove(field_name.as_str()).ok_or_else(|| {
            LowerError::new(
                format!("missing field `{field_name}` for struct `{}`", layout.name),
                span.clone(),
            )
        })?;
        fields.push(lower_expr_expecting(value, *field_ty, ctx, lcx, table)?);
    }
    if let Some((extra_name, extra_value)) = by_name.into_iter().next() {
        return Err(LowerError::new(
            format!("struct `{}` has no field `{extra_name}`", layout.name),
            extra_value.span.clone(),
        ));
    }

    Ok(Expr::MakeStruct { struct_id, fields })
}

/// Resolves the `(LocalId, KirType)` a spread-update's `base` already
/// carries. `base` must be a plain identifier: reading a non-overridden
/// field means reading `base` again, which is only safe when that's a
/// side-effect-free variable read, not an arbitrary expression (a call
/// could have side effects, and KIR's tree-shaped `Expr` has no let-binding
/// to evaluate an arbitrary `base` once and reuse the result). Every M2
/// fixture spreads an already-bound struct value, so this covers the exit
/// criterion; a general expression base is deferred, not silently
/// mis-evaluated.
///
/// Shared by [`lower_struct_spread`] (which also needs the struct id to
/// build the result) and `lower_stmt`'s `Let` arm (which uses this to infer
/// the expected type for `x = { ...base, .. }` with no explicit
/// annotation — the common case, since the type is already pinned by
/// `base` and an annotation would be redundant).
pub(crate) fn struct_spread_base(
    base: &SpannedExpr,
    ctx: &FnCtx,
) -> Result<(crate::ir::LocalId, KirType), LowerError> {
    let ast::Expr::Ident(base_name) = &base.kind else {
        return Err(LowerError::unsupported(
            "struct spread-update over a non-identifier base expression",
            base.span.clone(),
        ));
    };
    let base_local = ctx.resolve(base_name).ok_or_else(|| {
        LowerError::new(
            format!("unknown identifier `{base_name}`"),
            base.span.clone(),
        )
    })?;
    Ok((base_local, ctx.locals[base_local].ty))
}

/// Lowers `{ ...base, field: value, ... }` against its expected struct
/// type: copies every non-overridden field from `base`, takes overridden
/// ones from `overrides`, and rebuilds in declared field order (same as
/// [`lower_struct_lit`]).
fn lower_struct_spread(
    base: &SpannedExpr,
    overrides: &[(String, SpannedExpr)],
    struct_id: StructId,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
) -> Result<Expr, LowerError> {
    let (base_local, base_ty) = struct_spread_base(base, ctx)?;
    if base_ty != KirType::Struct(struct_id) {
        return Err(LowerError::new(
            format!(
                "spread-update base is `{}`, expected struct `{}`",
                describe_ty(base_ty, lcx),
                lcx.struct_layouts[struct_id].name
            ),
            base.span.clone(),
        ));
    }

    let mut overrides_by_name: HashMap<&str, &SpannedExpr> =
        HashMap::with_capacity(overrides.len());
    for (name, value) in overrides {
        if overrides_by_name.insert(name.as_str(), value).is_some() {
            return Err(LowerError::new(
                format!("duplicate field `{name}` in spread-update"),
                value.span.clone(),
            ));
        }
    }

    let layout = &lcx.struct_layouts[struct_id];
    let mut fields = Vec::with_capacity(layout.fields.len());
    for (index, (field_name, field_ty)) in layout.fields.iter().enumerate() {
        let field_expr = if let Some(value) = overrides_by_name.remove(field_name.as_str()) {
            lower_expr_expecting(value, *field_ty, ctx, lcx, table)?
        } else {
            Expr::FieldGet {
                base: Box::new(Expr::Local {
                    id: base_local,
                    ty: base_ty,
                }),
                field_index: index,
                ty: *field_ty,
            }
        };
        fields.push(field_expr);
    }
    if let Some((extra_name, extra_value)) = overrides_by_name.into_iter().next() {
        return Err(LowerError::new(
            format!("struct `{}` has no field `{extra_name}`", layout.name),
            extra_value.span.clone(),
        ));
    }

    Ok(Expr::MakeStruct { struct_id, fields })
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
fn lower_call(
    callee: &SpannedExpr,
    args: &[ast::CallArg],
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    call_span: &Span,
) -> Result<Expr, LowerError> {
    let ast::Expr::Ident(name) = &callee.kind else {
        return Err(LowerError::unsupported(
            "indirect call (only direct calls to named tasks are supported)",
            callee.span.clone(),
        ));
    };
    let sig = lcx.funcs.get(name).ok_or_else(|| {
        LowerError::new(format!("unknown function `{name}`"), callee.span.clone())
    })?;
    let defaults = &lcx.param_defaults[&sig.func_id];
    let required = defaults.iter().filter(|d| d.is_none()).count();

    if args.len() > sig.params.len() || args.len() < required {
        return Err(LowerError::new(
            format!(
                "`{name}` takes {} argument(s), got {}",
                if required == sig.params.len() {
                    required.to_string()
                } else {
                    format!("{required}-{}", sig.params.len())
                },
                args.len()
            ),
            call_span.clone(),
        ));
    }

    let mut lowered_args = Vec::with_capacity(sig.params.len());
    for (arg, expected_ty) in args.iter().zip(&sig.params) {
        if arg.name.is_some() || arg.spread {
            return Err(LowerError::unsupported(
                "named or spread call arguments",
                arg.value.span.clone(),
            ));
        }
        lowered_args.push(lower_expr_expecting(
            &arg.value,
            *expected_ty,
            ctx,
            lcx,
            table,
        )?);
    }
    // Every param beyond the supplied args is missing one — the arity check
    // above already proved each has a default (`lower_param_defaults` clones
    // the same pre-lowered `Expr` into every omitting call site, since KIR
    // `Expr` trees aren't shared/interned).
    for default in &defaults[args.len()..] {
        lowered_args.push(
            default
                .clone()
                .expect("arity check above proved every trailing param here has a default"),
        );
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
fn lower_ns_call(
    namespace: &str,
    method: &str,
    args: &[ast::CallArg],
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
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
        lowered_args.push(lower_expr(&arg.value, ctx, lcx, table)?);
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

/// Lowers `[e0, e1, ...]` to a chain of `CallTarget::Rt` calls — `keel_list_
/// new()` then a `keel_list_push` per element, folded left-to-right (no
/// dedicated `MakeStruct`-style construction node needed: the container ABI
/// is "opaque to codegen, every operation a runtime call" by design,
/// `designs/llvm-compilation.md` §2.7, and push already is one).
///
/// Every element must share one `KirType`, restricted to int/float/bool/str
/// (`is_list_element_ty`) — a struct/enum element needs `Value` marshaling
/// that doesn't exist yet. An empty literal is rejected too: with no
/// elements there's nothing to infer the element type from, and this crate
/// doesn't consult the checker's own inference for it (see `lower/mod.rs`'s
/// `CheckArtifacts` doc).
fn lower_list_lit(
    items: &[SpannedExpr],
    span: &Span,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
) -> Result<Expr, LowerError> {
    let [first, rest @ ..] = items else {
        return Err(LowerError::unsupported(
            "empty list literal (element type can't be inferred without an annotation)",
            span.clone(),
        ));
    };
    let first_e = lower_expr(first, ctx, lcx, table)?;
    let elem_ty = first_e.ty();
    if !super::is_list_element_ty(elem_ty) {
        return Err(LowerError::unsupported(
            "list element type other than int/float/bool/str (struct/enum elements need \
             Value marshaling, a later M2/M3 concern)",
            first.span.clone(),
        ));
    }

    let mut elements = Vec::with_capacity(items.len());
    elements.push(first_e);
    for item in rest {
        let item_e = lower_expr(item, ctx, lcx, table)?;
        if item_e.ty() != elem_ty {
            return Err(LowerError::new(
                format!(
                    "list literal has mixed element types: `{}` and `{}`",
                    describe_ty(elem_ty, lcx),
                    describe_ty(item_e.ty(), lcx)
                ),
                item.span.clone(),
            ));
        }
        elements.push(item_e);
    }

    let list_id = super::intern_list(lcx.lists, elem_ty);
    let list_ty = KirType::List(list_id);
    let span_id = table.intern(span.clone());
    let mut acc = Expr::Call {
        target: CallTarget::Rt(ir::RtFn::ListNew),
        args: Vec::new(),
        ty: list_ty,
        span: span_id,
    };
    for element in elements {
        acc = Expr::Call {
            target: CallTarget::Rt(ir::RtFn::ListPush),
            args: vec![acc, element],
            ty: list_ty,
            span: span_id,
        };
    }
    Ok(acc)
}

/// Lowers a value method call on a `list[T]`-typed receiver (`xs.push(v)`,
/// `xs.len()`) — the only container value methods lowered so far.
fn lower_list_method_call(
    object_e: Expr,
    method: &str,
    args: &[ast::CallArg],
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    call_span: &Span,
) -> Result<Expr, LowerError> {
    let KirType::List(list_id) = object_e.ty() else {
        unreachable!("caller only invokes lower_list_method_call on a KirType::List receiver");
    };
    for arg in args {
        if arg.name.is_some() || arg.spread {
            return Err(LowerError::unsupported(
                "named or spread arguments to a list method call",
                arg.value.span.clone(),
            ));
        }
    }

    let elem_ty = lcx.lists.borrow()[list_id];
    let span_id = table.intern(call_span.clone());
    match method {
        "push" => {
            let [arg] = args else {
                return Err(LowerError::new(
                    format!("`push` takes 1 argument, got {}", args.len()),
                    call_span.clone(),
                ));
            };
            let elem_e = lower_expr_expecting(&arg.value, elem_ty, ctx, lcx, table)?;
            Ok(Expr::Call {
                target: CallTarget::Rt(ir::RtFn::ListPush),
                args: vec![object_e, elem_e],
                ty: KirType::List(list_id),
                span: span_id,
            })
        }
        "len" | "count" => {
            if !args.is_empty() {
                return Err(LowerError::new(
                    format!("`{method}` takes 0 arguments, got {}", args.len()),
                    call_span.clone(),
                ));
            }
            Ok(Expr::Call {
                target: CallTarget::Rt(ir::RtFn::ListLen),
                args: vec![object_e],
                ty: KirType::I64,
                span: span_id,
            })
        }
        other => Err(LowerError::unsupported(
            &format!("list method `{other}` (only `push`/`len`/`count` are lowered so far)"),
            call_span.clone(),
        )),
    }
}
