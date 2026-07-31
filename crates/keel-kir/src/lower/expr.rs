//! Expression lowering — the M0 scalar subset (literals, identifiers,
//! arithmetic/comparison/logical binary ops, unary `-`/`not`, and direct
//! calls to other lowered tasks) plus M1's stdlib namespace calls
//! (`io.show(...)`, `log.info(...)` — see [`lower_call`]), M2's named-
//! struct literals, spread-update, field access, simple-enum variant
//! construction (`Priority.low`), `list[T]` literals/`push`/`len`/indexing
//! (`T` restricted to int/float/bool/str — see [`lower_list_lit`]), and
//! nullable `?.`/`??`/a `none` literal against a known expected nullable
//! type (inner restricted to int/float/bool/str/list/struct — see
//! `lower/mod.rs`'s `is_nullable_inner_ty`), and string interpolation
//! (desugars to `str` concatenation plus a per-slot to-string runtime call —
//! slots restricted to int/float/bool/str, format specs deferred — see
//! [`lower_string_lit`]), and `when` used as an expression in an arbitrary
//! nested position (`f(when n {...})`, `1 + when n {...}`), which hoists a
//! declare+`if`-chain pair ahead of the enclosing statement — see
//! [`super::stmt::lower_when_expr_value`] and [`FnCtx::keep_order`].
//! Everything else (casts, `if` as an expression, lambdas, `!`/`!.`
//! null-assert, pipelines, duration literals, rich (payload-carrying) enum
//! variants, non-container method calls/indexing) is rejected; see module
//! docs on `lower/mod.rs`.
//!
//! # Evaluation order and hoisting
//!
//! Because a nested `when`-expression's statements are emitted *ahead of the
//! whole enclosing statement*, any sibling sub-expression to its left would
//! otherwise end up running after it. Every site below that lowers two or
//! more sub-expressions in sequence therefore takes a [`FnCtx::hoist_mark`]
//! before each one and calls [`FnCtx::keep_order`] with the siblings lowered
//! so far, which spills them into temps bound ahead of the hoisted chain if
//! (and only if) that chain materialized. Sites where a sub-expression is
//! *not* evaluated exactly once — an `and`/`or` right operand, a `??`
//! fallback — call [`FnCtx::forbid_hoist`] instead, since no spill can
//! recover a conditional evaluation.

use std::collections::HashMap;

use keel_syntax::ast::{self, SpannedExpr};
use keel_syntax::lexer::Span;

use keel_catalog::builtins::BuiltinResult;

use super::{FnCtx, LowerCtx, LowerError, describe_ty};
use crate::ir::{self, CallTarget, Expr, MapId, StructId};
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
            if matches!(left, KirType::Nullable(_)) || matches!(right, KirType::Nullable(_)) {
                Err(LowerError::new(
                    format!(
                        "cannot compare nullable `{left}` and `{right}` for equality \
                         (unwrap via `??` first)"
                    ),
                    span.clone(),
                ))
            } else if left == right {
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
        ast::Expr::StringLit(parts) => lower_string_lit(parts, &expr.span, ctx, lcx, table),
        ast::Expr::Ident(name) => {
            let local = ctx.resolve(name).ok_or_else(|| {
                LowerError::new(format!("unknown identifier `{name}`"), expr.span.clone())
            })?;
            let ty = ctx.locals[local].ty;
            Ok(Expr::Local { id: local, ty })
        }
        ast::Expr::BinaryOp { left, op, right } => {
            let mut left_e = [lower_expr(left, ctx, lcx, table)?];
            let mark = ctx.hoist_mark();
            let right_e = lower_expr(right, ctx, lcx, table)?;
            let kir_op = convert_binop(*op);
            // `and`/`or` short-circuit: the right operand runs only when the
            // left didn't already decide the result, so a hoisted chain — which
            // would run unconditionally, ahead of the whole statement — can't
            // model it. Every other operator evaluates both operands exactly
            // once, left to right, which a spill of the left one preserves.
            if matches!(kir_op, ir::BinOp::And | ir::BinOp::Or) {
                ctx.forbid_hoist(
                    mark,
                    "the right-hand operand of `and`/`or` (short-circuiting means it may not \
                     be evaluated at all)",
                    &right.span,
                )?;
            } else {
                ctx.keep_order(mark, &mut left_e, &expr.span)?;
            }
            let [left_e] = left_e;
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

            // Positional tuple read (`pair.0`, `SPEC.md` §2.8). An all-digit
            // field name can only come from `postfix_field_name()`, so it is
            // unambiguously an index rather than a struct field. The checker
            // has already bounds-checked it and rejected `.N` on non-tuples,
            // so the arity check here is a lowering invariant, not a
            // user-facing diagnostic.
            if let Some(index) = keel_syntax::ast::tuple_index(field) {
                let KirType::Tuple(tuple_id) = base_e.ty() else {
                    return Err(LowerError::new(
                        format!(
                            "positional read `.{index}` on a {} value — the checker should \
                             have rejected this",
                            describe_ty(base_e.ty(), lcx)
                        ),
                        expr.span.clone(),
                    ));
                };
                let elems = &lcx.tuples.borrow()[tuple_id].elems;
                let ty = *elems.get(index).ok_or_else(|| {
                    LowerError::new(
                        format!(
                            "tuple index {index} is out of bounds for a {}-element shape — \
                             the checker should have rejected this",
                            elems.len()
                        ),
                        expr.span.clone(),
                    )
                })?;
                return Ok(Expr::TupleGet {
                    base: Box::new(base_e),
                    index,
                    ty,
                });
            }

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
        ast::Expr::NullFieldAccess(base, field) => {
            let base_e = lower_expr(base, ctx, lcx, table)?;
            let KirType::Nullable(nullable_id) = base_e.ty() else {
                return Err(LowerError::unsupported(
                    "`?.` on a non-nullable value (only a nullable-struct receiver is lowered \
                     so far)",
                    expr.span.clone(),
                ));
            };
            let KirType::Struct(struct_id) = lcx.nullables.borrow()[nullable_id] else {
                return Err(LowerError::unsupported(
                    "`?.` on a nullable non-struct value (str/list/scalar field access isn't \
                     meaningful — only a nullable struct has fields)",
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
            let field_ty = layout.fields[field_index].1;
            let ty = KirType::Nullable(super::intern_nullable(lcx.nullables, field_ty));
            Ok(Expr::NullFieldGet {
                base: Box::new(base_e),
                field_index,
                ty,
            })
        }
        ast::Expr::NullAssert(_) => Err(LowerError::unsupported("null-assert", expr.span.clone())),
        ast::Expr::SelfAccess { .. } => {
            Err(LowerError::unsupported("self access", expr.span.clone()))
        }
        ast::Expr::SelfRef => Err(LowerError::unsupported("self reference", expr.span.clone())),
        // No expected-type context here (that's `lower_expr_expecting`'s
        // job) — an anonymous struct literal with nothing pinning it to a
        // named struct isn't modeled yet (deferred until an M2 fixture
        // needs one; see `ir.rs`'s `StructLayout` doc). An int/bool-keyed
        // literal is unambiguously a map even without that context (the
        // checker's own `classify_literal` treats it that way, `str`/bare
        // keys need an expected `map[str, V]` type to disambiguate from a
        // struct) — but non-`str` map keys aren't modeled yet either (see
        // `KirType::Map`'s doc), so this names that specifically rather than
        // reporting the generic struct-literal message for what's actually
        // a map literal.
        ast::Expr::StructLit(fields) => {
            if let Some((ast::MapLitKey::Int(_) | ast::MapLitKey::Bool(_), value)) = fields.first()
            {
                return Err(LowerError::unsupported(
                    "map literal with a non-str key (int/bool keys are a later M2/M3 concern)",
                    value.span.clone(),
                ));
            }
            Err(LowerError::unsupported(
                "struct literal outside a known-struct-typed position (a `let` annotation, \
                 `return`, or call argument) — the same restriction applies to a bareword/`str`-\
                 keyed map literal, which needs an expected `map[str, V]` type to disambiguate \
                 from a struct",
                expr.span.clone(),
            ))
        }
        ast::Expr::StructSpreadUpdate { .. } => Err(LowerError::unsupported(
            "struct spread-update outside a known-struct-typed position (a `let` annotation, \
             `return`, or call argument)",
            expr.span.clone(),
        )),
        ast::Expr::ListLit(items) => lower_list_lit(items, &expr.span, ctx, lcx, table),
        ast::Expr::SetLit(items) => lower_set_lit(items, &expr.span, ctx, lcx, table),
        ast::Expr::TupleLit(items) => {
            // Tuples are structural, so the shape is inferred bottom-up from
            // the element expressions — no expected-type context needed, in
            // contrast to struct literals (see `lower_expr_expecting`).
            let mut elems: Vec<Expr> = Vec::with_capacity(items.len());
            for item in items {
                let mark = ctx.hoist_mark();
                let elem_e = lower_expr(item, ctx, lcx, table)?;
                ctx.keep_order(mark, &mut elems, &item.span)?;
                if !super::is_tuple_element_ty(elem_e.ty()) {
                    return Err(LowerError::unsupported(
                        "tuple element type other than int/float/bool/str or a nested tuple \
                         (container/struct/enum/nullable elements need the `Value` marshaling a \
                         by-value tuple deliberately avoids)",
                        item.span.clone(),
                    ));
                }
                elems.push(elem_e);
            }
            let shape = elems.iter().map(Expr::ty).collect::<Vec<_>>();
            let tuple_id = super::intern_tuple(lcx.tuples, shape);
            Ok(Expr::MakeTuple { tuple_id, elems })
        }
        ast::Expr::NullCoalesce(nullable, fallback) => {
            let nullable_e = lower_expr(nullable, ctx, lcx, table)?;
            let KirType::Nullable(nullable_id) = nullable_e.ty() else {
                return Err(LowerError::unsupported(
                    "`??` on a non-nullable left-hand side",
                    expr.span.clone(),
                ));
            };
            let inner_ty = lcx.nullables.borrow()[nullable_id];
            // The fallback runs only when the left-hand side is null — same
            // conditional-evaluation problem as an `and`/`or` right operand.
            let mark = ctx.hoist_mark();
            let fallback_e = lower_expr_expecting(fallback, inner_ty, ctx, lcx, table)?;
            ctx.forbid_hoist(
                mark,
                "a `??` fallback (it is evaluated only when the left-hand side is null)",
                &fallback.span,
            )?;
            Ok(Expr::NullCoalesce {
                nullable: Box::new(nullable_e),
                fallback: Box::new(fallback_e),
                ty: inner_ty,
            })
        }
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
                KirType::Map(_) => {
                    lower_map_method_call(object_e, method, args, ctx, lcx, table, &expr.span)
                }
                KirType::Set(_) => {
                    lower_set_method_call(object_e, method, args, ctx, lcx, table, &expr.span)
                }
                _ => Err(LowerError::unsupported("method call", expr.span.clone())),
            }
        }
        ast::Expr::Cast { .. } => Err(LowerError::unsupported("`as` cast", expr.span.clone())),
        ast::Expr::IfExpr { .. } => Err(LowerError::unsupported(
            "`if` expression",
            expr.span.clone(),
        )),
        // A `when` in a nested position with no expected type pinned by the
        // surrounding syntax (a stdlib namespace argument, an interpolation
        // slot, …) — the result type comes from the first arm's own tail
        // value. `lower_expr_expecting` handles the pinned case.
        ast::Expr::WhenExpr { subject, arms } => {
            super::stmt::lower_when_expr_value(subject, arms, None, ctx, lcx, table, &expr.span)
        }
        ast::Expr::Lambda { .. } => Err(LowerError::unsupported("lambda", expr.span.clone())),
        ast::Expr::Index { object, index } => {
            let mut object_e = [lower_expr(object, ctx, lcx, table)?];
            let KirType::List(list_id) = object_e[0].ty() else {
                return Err(LowerError::unsupported(
                    "index access on a non-list value (strings/maps land in a later M2 issue)",
                    expr.span.clone(),
                ));
            };
            let mark = ctx.hoist_mark();
            let index_e = lower_expr(index, ctx, lcx, table)?;
            ctx.keep_order(mark, &mut object_e, &expr.span)?;
            let [object_e] = object_e;
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
    // A nested `when`-expression takes its result type straight from the
    // pinned one, so each arm's tail value is coerced against it (the same
    // `TailSink::Assign` path a `let`-position `when` uses) instead of being
    // inferred from the first arm alone — which is also what lets an arm
    // whose tail is a struct literal resolve to the *named* struct.
    if let ast::Expr::WhenExpr { subject, arms } = &expr.kind {
        return super::stmt::lower_when_expr_value(
            subject,
            arms,
            Some(expected),
            ctx,
            lcx,
            table,
            &expr.span,
        );
    }
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
    // `{key: value, ...}` is the same ambiguous `ast::Expr::StructLit` node
    // a struct literal parses to (see `ast::MapLitKey`'s doc) — the checker
    // itself only resolves a bareword/`str`-keyed one to a map when an
    // expected `map[str, V]` type is already in scope; since that's exactly
    // what `expected` already carries here, this needs no `CheckArtifacts`
    // lookup, same as `lower_struct_lit` above never needing one.
    if let KirType::Map(map_id) = expected
        && let ast::Expr::StructLit(fields) = &expr.kind
    {
        return lower_map_lit(fields, map_id, &expr.span, ctx, lcx, table);
    }
    // `none` has no type of its own to infer bottom-up (`lower_expr` rejects
    // it outright) — the checker only accepts a bare `none` literal where an
    // expected nullable type already pins one (a param default, a `let`
    // annotation, a call argument, a nullable struct field), so this is the
    // only place it resolves.
    if let KirType::Nullable(id) = expected {
        if matches!(expr.kind, ast::Expr::None_) {
            return Ok(Expr::NullLit { ty: expected });
        }
        // The checker allows passing a non-nullable `T` wherever `T?` is
        // expected (widening, one-directional — the other way needs `?.`/
        // `??`/an assert). Bottom-up lowering already handles anything
        // that's *already* `Nullable(inner)`-typed (an identifier bound
        // `T?`, `?.`/`??` results, …) as well as a plain `inner`-typed value
        // that needs wrapping; a raw struct literal assigned directly to a
        // nullable-struct position isn't supported yet (every M2 fixture
        // binds the struct to a name first).
        let inner_ty = lcx.nullables.borrow()[id];
        let lowered = lower_expr(expr, ctx, lcx, table)?;
        if lowered.ty() == expected {
            return Ok(lowered);
        }
        if lowered.ty() == inner_ty {
            return Ok(Expr::NullSome {
                value: Box::new(lowered),
                ty: expected,
            });
        }
        return Err(LowerError::new(
            format!(
                "expected `{}` or `{}`, got `{}`",
                describe_ty(expected, lcx),
                describe_ty(inner_ty, lcx),
                describe_ty(lowered.ty(), lcx)
            ),
            expr.span.clone(),
        ));
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

    // Fields are rebuilt in declared order, which is also the order they're
    // evaluated in — so an earlier field must be spilled if a later one
    // hoists. (Declared order is not necessarily the *source* order the
    // interpreter evaluates in; that pre-existing difference is only
    // observable for side-effecting field expressions and is out of scope
    // here.)
    let mut fields: Vec<Expr> = Vec::with_capacity(layout.fields.len());
    for (field_name, field_ty) in &layout.fields {
        let value = by_name.remove(field_name.as_str()).ok_or_else(|| {
            LowerError::new(
                format!("missing field `{field_name}` for struct `{}`", layout.name),
                span.clone(),
            )
        })?;
        let mark = ctx.hoist_mark();
        let value_e = lower_expr_expecting(value, *field_ty, ctx, lcx, table)?;
        ctx.keep_order(mark, &mut fields, &value.span)?;
        fields.push(value_e);
    }
    if let Some((extra_name, extra_value)) = by_name.into_iter().next() {
        return Err(LowerError::new(
            format!("struct `{}` has no field `{extra_name}`", layout.name),
            extra_value.span.clone(),
        ));
    }

    Ok(Expr::MakeStruct { struct_id, fields })
}

/// Lowers a bareword/`str`-keyed `{key: value, ...}` literal against its
/// expected `map[str, V]` type (the `map_methods.keel`-style shape,
/// `stock: map[str, int] = {apples: 12, ...}`) — folds into a
/// `MapNew`/`MapInsert` chain, same shape [`lower_list_lit`] builds for a
/// list literal. Field order in the source doesn't matter (unlike a struct
/// literal, a map has no declared field order to rebuild into) — each
/// key/value pair just inserts in source order.
fn lower_map_lit(
    lit_fields: &[(ast::MapLitKey, SpannedExpr)],
    map_id: MapId,
    span: &Span,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
) -> Result<Expr, LowerError> {
    let value_ty = lcx.maps.borrow()[map_id];
    let map_ty = KirType::Map(map_id);
    let span_id = table.intern(span.clone());

    let mut seen: HashMap<&str, ()> = HashMap::with_capacity(lit_fields.len());
    let mut acc = Expr::Call {
        target: CallTarget::Rt(ir::RtFn::MapNew),
        args: Vec::new(),
        ty: map_ty,
        span: span_id,
    };
    for (key, value) in lit_fields {
        let key_str = key.as_str().ok_or_else(|| {
            LowerError::unsupported(
                "map literal with a non-str key (int/bool keys are a later M2/M3 concern)",
                value.span.clone(),
            )
        })?;
        if seen.insert(key_str, ()).is_some() {
            // The interpreter silently takes last-wins on a duplicate map
            // key (no error); rejecting it here is a deliberate, stricter-
            // than-interpreter divergence — this repo's namespace-
            // implementation checklist treats a duplicate key as a
            // structural-precondition violation that must error rather than
            // silently drop data, and no conformance fixture should ever
            // contain a duplicate-key map literal in the first place.
            return Err(LowerError::new(
                format!("duplicate key `{key_str}` in map literal"),
                value.span.clone(),
            ));
        }
        // The accumulator holds every pair inserted so far, so spilling it is
        // exactly what keeps those earlier values ahead of a hoist from this
        // one.
        let mark = ctx.hoist_mark();
        let value_e = lower_expr_expecting(value, value_ty, ctx, lcx, table)?;
        let mut prior = [acc];
        ctx.keep_order(mark, &mut prior, &value.span)?;
        let [prior] = prior;
        acc = Expr::Call {
            target: CallTarget::Rt(ir::RtFn::MapInsert),
            args: vec![prior, Expr::ConstStr(key_str.to_string()), value_e],
            ty: map_ty,
            span: span_id,
        };
    }
    Ok(acc)
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
    let mut fields: Vec<Expr> = Vec::with_capacity(layout.fields.len());
    for (index, (field_name, field_ty)) in layout.fields.iter().enumerate() {
        let field_expr = if let Some(value) = overrides_by_name.remove(field_name.as_str()) {
            let mark = ctx.hoist_mark();
            let value_e = lower_expr_expecting(value, *field_ty, ctx, lcx, table)?;
            // Spills both earlier overrides and the implicit `FieldGet` reads
            // of `base` interleaved between them.
            ctx.keep_order(mark, &mut fields, &value.span)?;
            value_e
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

/// Desugars a (possibly interpolated) string literal to a chain of `+`
/// (`str` concatenation, `BinOp::Add`) over its literal segments and
/// interpolation slots (§2.3: "string interpolation → concat calls") — an
/// `int`/`float`/`bool`-typed slot is first converted via the matching
/// `RtFn::*ToStr` runtime call; a `str`-typed slot is spliced in as-is.
/// Slots with a format spec (`{expr:spec}`) and slots of any other type
/// (struct/enum-to-string is a later M2/M3 concern — coordinate with
/// #145/#146 rather than block on them) are rejected, not silently dropped.
fn lower_string_lit(
    parts: &[ast::StringPart],
    span: &Span,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
) -> Result<Expr, LowerError> {
    if parts.is_empty() {
        return Ok(Expr::ConstStr(String::new()));
    }

    let span_id = table.intern(span.clone());
    let mut acc: Option<Expr> = None;
    for part in parts {
        let piece = match part {
            ast::StringPart::Literal(s) => Expr::ConstStr(s.clone()),
            ast::StringPart::ParseError(raw) => {
                return Err(LowerError::new(
                    format!("invalid expression in string interpolation: `{raw}`"),
                    span.clone(),
                ));
            }
            ast::StringPart::Interpolation(e, spec) => {
                if spec.is_some() {
                    return Err(LowerError::unsupported(
                        "string-interpolation format spec (`{expr:spec}`)",
                        e.span.clone(),
                    ));
                }
                // `acc` holds every segment emitted so far, so spilling it
                // keeps them ahead of a hoist from this slot.
                let mark = ctx.hoist_mark();
                let lowered = lower_expr(e, ctx, lcx, table)?;
                if let Some(prev) = acc.as_mut() {
                    ctx.keep_order(mark, std::slice::from_mut(prev), &e.span)?;
                }
                let rt_fn = match lowered.ty() {
                    KirType::Str => None,
                    KirType::I64 => Some(ir::RtFn::IntToStr),
                    KirType::F64 => Some(ir::RtFn::FloatToStr),
                    KirType::Bool => Some(ir::RtFn::BoolToStr),
                    other => {
                        return Err(LowerError::new(
                            format!(
                                "interpolating a `{}` value is not supported by the scalar-subset \
                                 KIR lowering (M0) (only int/float/bool/str values may be \
                                 interpolated — struct/enum to-string is a later M2/M3 concern)",
                                describe_ty(other, lcx)
                            ),
                            e.span.clone(),
                        ));
                    }
                };
                match rt_fn {
                    None => lowered,
                    Some(rt_fn) => Expr::Call {
                        target: CallTarget::Rt(rt_fn),
                        args: vec![lowered],
                        ty: KirType::Str,
                        span: span_id,
                    },
                }
            }
        };
        acc = Some(match acc {
            None => piece,
            Some(prev) => Expr::BinOp {
                op: ir::BinOp::Add,
                left: Box::new(prev),
                right: Box::new(piece),
                ty: KirType::Str,
            },
        });
    }
    Ok(acc.expect("parser never produces an empty StringPart list"))
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

    let mut lowered_args: Vec<Expr> = Vec::with_capacity(sig.params.len());
    for (arg, expected_ty) in args.iter().zip(&sig.params) {
        if arg.name.is_some() || arg.spread {
            return Err(LowerError::unsupported(
                "named or spread call arguments",
                arg.value.span.clone(),
            ));
        }
        let mark = ctx.hoist_mark();
        let arg_e = lower_expr_expecting(&arg.value, *expected_ty, ctx, lcx, table)?;
        ctx.keep_order(mark, &mut lowered_args, &arg.value.span)?;
        lowered_args.push(arg_e);
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

    let mut lowered_args: Vec<Expr> = Vec::with_capacity(args.len());
    for arg in args {
        if arg.name.is_some() || arg.spread {
            return Err(LowerError::unsupported(
                "named or spread arguments to a stdlib namespace call",
                arg.value.span.clone(),
            ));
        }
        let mark = ctx.hoist_mark();
        let arg_e = lower_expr(&arg.value, ctx, lcx, table)?;
        ctx.keep_order(mark, &mut lowered_args, &arg.value.span)?;
        lowered_args.push(arg_e);
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

    let mut elements: Vec<Expr> = Vec::with_capacity(items.len());
    elements.push(first_e);
    for item in rest {
        let mark = ctx.hoist_mark();
        let item_e = lower_expr(item, ctx, lcx, table)?;
        ctx.keep_order(mark, &mut elements, &item.span)?;
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

/// Lowers `set[expr, ...]` — folds into a `SetNew`/`SetInsert` chain, the
/// same shape [`lower_list_lit`] builds.
///
/// Duplicate elements are *not* rejected here, unlike [`lower_map_lit`]'s
/// duplicate keys: `set[1, 1]` is a well-defined set of one, and each
/// `SetInsert` drops the redundant element at runtime via the interpreter's
/// own dedup (`keel-runtime`'s `value::set_insert`). A duplicate map key, by
/// contrast, silently discards a *value* the program supplied, which is why
/// that one is an error.
fn lower_set_lit(
    items: &[SpannedExpr],
    span: &Span,
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
) -> Result<Expr, LowerError> {
    let [first, rest @ ..] = items else {
        return Err(LowerError::unsupported(
            "empty set literal (element type can't be inferred without an annotation)",
            span.clone(),
        ));
    };
    let first_e = lower_expr(first, ctx, lcx, table)?;
    let elem_ty = first_e.ty();
    if !super::is_list_element_ty(elem_ty) {
        return Err(LowerError::unsupported(
            "set element type other than int/float/bool/str (struct/enum elements need Value \
             marshaling, a later M2/M3 concern)",
            first.span.clone(),
        ));
    }

    let mut elements: Vec<Expr> = Vec::with_capacity(items.len());
    elements.push(first_e);
    for item in rest {
        let mark = ctx.hoist_mark();
        let item_e = lower_expr(item, ctx, lcx, table)?;
        ctx.keep_order(mark, &mut elements, &item.span)?;
        if item_e.ty() != elem_ty {
            return Err(LowerError::new(
                format!(
                    "set literal has mixed element types: `{}` and `{}`",
                    describe_ty(elem_ty, lcx),
                    describe_ty(item_e.ty(), lcx)
                ),
                item.span.clone(),
            ));
        }
        elements.push(item_e);
    }

    let set_id = super::intern_set(lcx.sets, elem_ty);
    let set_ty = KirType::Set(set_id);
    let span_id = table.intern(span.clone());
    let mut acc = Expr::Call {
        target: CallTarget::Rt(ir::RtFn::SetNew),
        args: Vec::new(),
        ty: set_ty,
        span: span_id,
    };
    for element in elements {
        acc = Expr::Call {
            target: CallTarget::Rt(ir::RtFn::SetInsert),
            args: vec![acc, element],
            ty: set_ty,
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
    // The receiver was lowered before any argument, so it's their left
    // sibling in evaluation order.
    let mut object_e = [object_e];
    match method {
        "push" => {
            let [arg] = args else {
                return Err(LowerError::new(
                    format!("`push` takes 1 argument, got {}", args.len()),
                    call_span.clone(),
                ));
            };
            let mark = ctx.hoist_mark();
            let elem_e = lower_expr_expecting(&arg.value, elem_ty, ctx, lcx, table)?;
            ctx.keep_order(mark, &mut object_e, call_span)?;
            let [object_e] = object_e;
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
            let [object_e] = object_e;
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

/// Lowers a value method call on a `map[str, V]`-typed receiver (`m.get(k)`,
/// `m.keys()`, `m.values()`, `m.len()`/`.count()`/`.size()`,
/// `m.is_empty()`, `m.contains(k)`/`.has(k)`) — the construct/read subset
/// issue #162 scopes to (no mutation method exists in the interpreter to
/// match yet — see `KirType::Map`'s doc). `is_empty` has no dedicated
/// runtime op; it's synthesized as `len == 0` rather than adding a
/// `keel_map_is_empty` FFI symbol whose only job would be exactly that.
fn lower_map_method_call(
    object_e: Expr,
    method: &str,
    args: &[ast::CallArg],
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    call_span: &Span,
) -> Result<Expr, LowerError> {
    let KirType::Map(map_id) = object_e.ty() else {
        unreachable!("caller only invokes lower_map_method_call on a KirType::Map receiver");
    };
    for arg in args {
        if arg.name.is_some() || arg.spread {
            return Err(LowerError::unsupported(
                "named or spread arguments to a map method call",
                arg.value.span.clone(),
            ));
        }
    }

    let value_ty = lcx.maps.borrow()[map_id];
    let span_id = table.intern(call_span.clone());
    // The receiver was lowered before any argument, so it's their left
    // sibling in evaluation order — `prior` carries it into `keep_order`.
    let mut object_e = [object_e];

    let one_str_arg = |args: &[ast::CallArg],
                       ctx: &mut FnCtx,
                       table: &mut SpanTable,
                       prior: &mut [Expr]|
     -> Result<Expr, LowerError> {
        let [arg] = args else {
            return Err(LowerError::new(
                format!("`{method}` takes 1 argument, got {}", args.len()),
                call_span.clone(),
            ));
        };
        let mark = ctx.hoist_mark();
        let key_e = lower_expr_expecting(&arg.value, KirType::Str, ctx, lcx, table)?;
        ctx.keep_order(mark, prior, &arg.value.span)?;
        Ok(key_e)
    };

    match method {
        // Mirrors `lower_list_method_call`'s `push`: a value method whose
        // result type is the receiver's, not a mutation of the receiver.
        "insert" => {
            let [key_arg, val_arg] = args else {
                return Err(LowerError::new(
                    format!("`insert` takes 2 arguments, got {}", args.len()),
                    call_span.clone(),
                ));
            };
            let key_mark = ctx.hoist_mark();
            let key_e = lower_expr_expecting(&key_arg.value, KirType::Str, ctx, lcx, table)?;
            ctx.keep_order(key_mark, &mut object_e, &key_arg.value.span)?;
            let [receiver] = object_e;
            let mut prior = [receiver, key_e];
            let val_mark = ctx.hoist_mark();
            let val_e = lower_expr_expecting(&val_arg.value, value_ty, ctx, lcx, table)?;
            ctx.keep_order(val_mark, &mut prior, &val_arg.value.span)?;
            let [object_e, key_e] = prior;
            Ok(Expr::Call {
                target: CallTarget::Rt(ir::RtFn::MapInsert),
                args: vec![object_e, key_e, val_e],
                ty: KirType::Map(map_id),
                span: span_id,
            })
        }
        "get" => {
            let key_e = one_str_arg(args, ctx, table, &mut object_e)?;
            let [object_e] = object_e;
            let nullable_id = super::intern_nullable(lcx.nullables, value_ty);
            Ok(Expr::Call {
                target: CallTarget::Rt(ir::RtFn::MapGet),
                args: vec![object_e, key_e],
                ty: KirType::Nullable(nullable_id),
                span: span_id,
            })
        }
        "contains" | "has" => {
            let key_e = one_str_arg(args, ctx, table, &mut object_e)?;
            let [object_e] = object_e;
            Ok(Expr::Call {
                target: CallTarget::Rt(ir::RtFn::MapContains),
                args: vec![object_e, key_e],
                ty: KirType::Bool,
                span: span_id,
            })
        }
        "keys" => {
            if !args.is_empty() {
                return Err(LowerError::new(
                    format!("`keys` takes 0 arguments, got {}", args.len()),
                    call_span.clone(),
                ));
            }
            let [object_e] = object_e;
            let list_id = super::intern_list(lcx.lists, KirType::Str);
            Ok(Expr::Call {
                target: CallTarget::Rt(ir::RtFn::MapKeys),
                args: vec![object_e],
                ty: KirType::List(list_id),
                span: span_id,
            })
        }
        "values" => {
            if !args.is_empty() {
                return Err(LowerError::new(
                    format!("`values` takes 0 arguments, got {}", args.len()),
                    call_span.clone(),
                ));
            }
            let [object_e] = object_e;
            let list_id = super::intern_list(lcx.lists, value_ty);
            Ok(Expr::Call {
                target: CallTarget::Rt(ir::RtFn::MapValues),
                args: vec![object_e],
                ty: KirType::List(list_id),
                span: span_id,
            })
        }
        "len" | "count" | "size" => {
            if !args.is_empty() {
                return Err(LowerError::new(
                    format!("`{method}` takes 0 arguments, got {}", args.len()),
                    call_span.clone(),
                ));
            }
            let [object_e] = object_e;
            Ok(Expr::Call {
                target: CallTarget::Rt(ir::RtFn::MapLen),
                args: vec![object_e],
                ty: KirType::I64,
                span: span_id,
            })
        }
        "is_empty" => {
            if !args.is_empty() {
                return Err(LowerError::new(
                    format!("`is_empty` takes 0 arguments, got {}", args.len()),
                    call_span.clone(),
                ));
            }
            let [object_e] = object_e;
            let len_e = Expr::Call {
                target: CallTarget::Rt(ir::RtFn::MapLen),
                args: vec![object_e],
                ty: KirType::I64,
                span: span_id,
            };
            Ok(Expr::BinOp {
                op: ir::BinOp::Eq,
                left: Box::new(len_e),
                right: Box::new(Expr::ConstInt(0)),
                ty: KirType::Bool,
            })
        }
        other => Err(LowerError::unsupported(
            &format!(
                "map method `{other}` (only `insert`/`get`/`keys`/`values`/`len`/`count`/`size`/\
                 `is_empty`/`contains`/`has` are lowered so far)"
            ),
            call_span.clone(),
        )),
    }
}

/// Lowers a value method call on a `set[T]`-typed receiver (`s.add(v)`,
/// `s.contains(v)`, `s.len()`/`.count()`/`.size()`, `s.is_empty()`).
///
/// The read-only list pipeline a set also accepts at runtime
/// (`.map`/`.filter`/… — see `keel-runtime`'s `SET_LIST_METHODS`) is not
/// lowered here, for the same reason the equivalent list methods aren't
/// lowered in [`lower_list_method_call`]: they take lambdas, which arrive
/// with closure support in M3.
///
/// Like `lower_map_method_call`, `is_empty` is synthesized as `len == 0`
/// rather than given its own FFI symbol.
fn lower_set_method_call(
    object_e: Expr,
    method: &str,
    args: &[ast::CallArg],
    ctx: &mut FnCtx,
    lcx: &LowerCtx<'_>,
    table: &mut SpanTable,
    call_span: &Span,
) -> Result<Expr, LowerError> {
    let KirType::Set(set_id) = object_e.ty() else {
        unreachable!("caller only invokes lower_set_method_call on a KirType::Set receiver");
    };
    for arg in args {
        if arg.name.is_some() || arg.spread {
            return Err(LowerError::unsupported(
                "named or spread arguments to a set method call",
                arg.value.span.clone(),
            ));
        }
    }

    let elem_ty = lcx.sets.borrow()[set_id];
    let span_id = table.intern(call_span.clone());
    // Same receiver-is-a-left-sibling handling as `lower_map_method_call`.
    let mut object_e = [object_e];

    let one_elem_arg = |args: &[ast::CallArg],
                        ctx: &mut FnCtx,
                        table: &mut SpanTable,
                        prior: &mut [Expr]|
     -> Result<Expr, LowerError> {
        let [arg] = args else {
            return Err(LowerError::new(
                format!("`{method}` takes 1 argument, got {}", args.len()),
                call_span.clone(),
            ));
        };
        let mark = ctx.hoist_mark();
        let elem_e = lower_expr_expecting(&arg.value, elem_ty, ctx, lcx, table)?;
        ctx.keep_order(mark, prior, &arg.value.span)?;
        Ok(elem_e)
    };

    match method {
        "add" => {
            let elem_e = one_elem_arg(args, ctx, table, &mut object_e)?;
            let [object_e] = object_e;
            Ok(Expr::Call {
                target: CallTarget::Rt(ir::RtFn::SetInsert),
                args: vec![object_e, elem_e],
                ty: KirType::Set(set_id),
                span: span_id,
            })
        }
        "contains" => {
            let elem_e = one_elem_arg(args, ctx, table, &mut object_e)?;
            let [object_e] = object_e;
            Ok(Expr::Call {
                target: CallTarget::Rt(ir::RtFn::SetContains),
                args: vec![object_e, elem_e],
                ty: KirType::Bool,
                span: span_id,
            })
        }
        "len" | "count" | "size" => {
            if !args.is_empty() {
                return Err(LowerError::new(
                    format!("`{method}` takes 0 arguments, got {}", args.len()),
                    call_span.clone(),
                ));
            }
            let [object_e] = object_e;
            Ok(Expr::Call {
                target: CallTarget::Rt(ir::RtFn::SetLen),
                args: vec![object_e],
                ty: KirType::I64,
                span: span_id,
            })
        }
        "is_empty" => {
            if !args.is_empty() {
                return Err(LowerError::new(
                    format!("`is_empty` takes 0 arguments, got {}", args.len()),
                    call_span.clone(),
                ));
            }
            let [object_e] = object_e;
            let len_e = Expr::Call {
                target: CallTarget::Rt(ir::RtFn::SetLen),
                args: vec![object_e],
                ty: KirType::I64,
                span: span_id,
            };
            Ok(Expr::BinOp {
                op: ir::BinOp::Eq,
                left: Box::new(len_e),
                right: Box::new(Expr::ConstInt(0)),
                ty: KirType::Bool,
            })
        }
        other => Err(LowerError::unsupported(
            &format!(
                "set method `{other}` (only `add`/`contains`/`len`/`count`/`size`/\
                 `is_empty` are lowered so far)"
            ),
            call_span.clone(),
        )),
    }
}
