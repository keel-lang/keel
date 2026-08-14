//! Expression type inference for the type checker.
//!
//! `infer_expr` recursively infers the [`Ty`] of every expression in the AST.
//! The inference is deliberately shallow — when a type cannot be computed cheaply
//! it falls back to [`Ty::Unknown`] and no error is emitted.  High-signal
//! diagnostics (undefined identifiers, wrong operator types, arity mismatches,
//! non-exhaustive `when`) are reported as errors via [`Checker::err`].

use std::collections::HashMap;

use crate::ast::*;
use crate::hir::{LiteralKind, MapKeyKind, NameKind};
use crate::types::prelude::{self, BuiltinResult};
use crate::types::scope::Scope;
use crate::types::ty::{Ty, UnknownReason, describe_ty};

use super::{
    BlockValue, Checker,
    binop::{check_binop, infer_binary},
};

fn mock_target_from_expr(expr: &SpannedExpr) -> Option<(&str, &str)> {
    let Expr::FieldAccess(object, method) = &expr.kind else {
        return None;
    };
    let Expr::Ident(namespace) = &object.as_ref().kind else {
        return None;
    };
    Some((namespace.as_str(), method.as_str()))
}

/// Message for `self.m(...)` inside an `impl` method. The interpreter routes
/// every `self.m(...)` to the *current agent's* task, so in an impl method it
/// either fails outright or — when the method was called from inside an agent —
/// silently invokes that agent's task of the same name. Neither is what the
/// author meant, so reject it at check time.
fn impl_self_call_error(method: &str) -> String {
    format!(
        "`self.{method}(...)` is not available in an `impl` method: `self` is the \
         receiver value, and `self.{method}(...)` always calls the enclosing \
         agent's task. Move the logic into a top-level task and call it as \
         `{method}(self)`"
    )
}

impl Checker<'_, '_> {
    pub(crate) fn infer_expr(&mut self, spanned: &SpannedExpr, scope: &mut Scope) -> Ty {
        let ty = self.infer_expr_uncached(spanned, scope);
        self.record_expr_type(&spanned.span, &ty);
        ty
    }

    /// Core inference match, factored out of [`Checker::infer_expr`] so the
    /// wrapper can record every expression's resolved type in one place —
    /// including every recursive sub-expression, since inner arms call back
    /// through `self.infer_expr`, not this function directly. Consumed by
    /// [`crate::types::checker::check_program_with_artifacts`] and
    /// [`crate::types::checker::check_graph_with_artifacts`].
    fn infer_expr_uncached(&mut self, spanned: &SpannedExpr, scope: &mut Scope) -> Ty {
        let expr = &spanned.kind;
        match expr {
            Expr::Integer(_) => Ty::Int,
            Expr::Float(_) => Ty::Float,
            Expr::Bool(_) => Ty::Bool,
            Expr::None_ => Ty::None_,

            Expr::StringLit(parts) => {
                for p in parts {
                    match p {
                        StringPart::Interpolation(e, _spec) => {
                            self.infer_expr(e, scope);
                        }
                        StringPart::ParseError(raw) => {
                            self.err(format!(
                                "invalid expression in string interpolation: `{raw}`"
                            ));
                        }
                        StringPart::Literal(_) => {}
                    }
                }
                Ty::Str
            }

            Expr::Ident(name) => {
                // Lexical scope always shadows global declarations.
                if let Some(t) = scope.get(name) {
                    return t.clone();
                }
                // Global namespace: consult the pre-built name index.
                match self.hir.resolve_global(name).kind {
                    NameKind::TopTask => {
                        // Identifier used as a value — expose the task's type as
                        // a function so callers can check arity at call sites.
                        if let Some(t) = self.top_tasks.get(name) {
                            return Ty::Func(
                                t.params.iter().map(|(_, ty)| ty.clone()).collect(),
                                Box::new(t.return_type.clone()),
                            );
                        }
                        // The index and top_tasks are built together; this branch
                        // is unreachable in practice.
                        Ty::Error
                    }
                    // Agent, type-level, module, or prelude identifier used as
                    // an expression — no value-level type is tracked statically.
                    NameKind::Agent
                    | NameKind::Enum
                    | NameKind::TypeName
                    | NameKind::Module
                    | NameKind::PreludeNamespace => Ty::Unknown(UnknownReason::InferenceLimitation),
                    // Undefined-name errors are emitted by HIR lowering before
                    // inference runs. Return Ty::Error
                    // here so downstream type checks are suppressed via
                    // is_opaque(), avoiding cascading noise.
                    NameKind::Local => unreachable!("HIR global resolution excludes locals"),
                    NameKind::Unresolved => Ty::Error,
                }
            }

            Expr::SelfAccess { field, .. } => {
                // Inside an `impl` method the receiver is a value of the
                // implementing type — resolve the field against that type.
                // Non-struct implementing types (enums, aliases) resolve
                // opaquely, so nothing is falsely reported.
                if let Some(self_ty) = self.impl_self_ty() {
                    return self.resolve_struct_field_checked(&self_ty, field);
                }
                let Some(agent_name) = self.current_agent.clone() else {
                    self.err(format!("`self.{field}` used outside an agent"));
                    return Ty::Error;
                };
                if let Some(t) = self
                    .agents
                    .get(&agent_name)
                    .and_then(|a| a.state_fields.get(field))
                {
                    return t.clone();
                }
                self.err(format!("agent `{agent_name}` has no state field `{field}`"));
                Ty::Error
            }

            // In an `impl` method `self` is the receiver value, so it has the
            // implementing type. Elsewhere it is the agent reference itself,
            // whose type is not tracked statically — only `self.field` and
            // `self.task(...)` are resolved there.
            Expr::SelfRef => self
                .impl_self_ty()
                .unwrap_or(Ty::Unknown(UnknownReason::InferenceLimitation)),

            Expr::FieldAccess(obj, field) => {
                if matches!(field.as_str(), "called" | "call_count")
                    && let Some((namespace, method)) = mock_target_from_expr(obj)
                    && prelude::catalog_method(namespace, method).is_some()
                {
                    if self.current_test_mocks.as_ref().is_some_and(|mocks| {
                        mocks.contains(&(namespace.to_string(), method.to_string()))
                    }) {
                        return if field == "called" { Ty::Bool } else { Ty::Int };
                    }
                    self.err(format!(
                        "`{namespace}.{method}.{field}` requires \
                         `testing.mock({namespace}.{method}).returns(...)` in the current test"
                    ));
                    return Ty::Error;
                }
                // Resolve ident-qualified field access through the name index
                // before falling through to value-level struct field access.
                // Lexical locals shadow globals (e.g. `db = db.connect(...)`
                // rebinds `db` to a connection value).
                if let Expr::Ident(name) = &obj.as_ref().kind
                    && scope.get(name).is_none()
                {
                    match self.hir.resolve_global(name).kind {
                        // Enum variant shortcut: `Urgency.medium`.
                        NameKind::Enum => {
                            if let Some(variants) = self.enum_variants.get(name)
                                && !variants.contains(field)
                            {
                                self.err(format!("enum `{name}` has no variant `{field}`"));
                            }
                            return Ty::Enum(name.clone(), vec![]);
                        }
                        // Module namespace member access: `file.read`, `uuid.DNS`,
                        // `validation.email`, `validation.Watcher`.
                        NameKind::Module => {
                            return self.module_field_access(name, field);
                        }
                        // Prelude identifier field access.
                        NameKind::PreludeNamespace => {
                            // Tombstone: `Uuid` is still the built-in type, but its
                            // constructors and constants moved to `std/uuid`.
                            if name == "Uuid" {
                                self.err(format!(
                                    "`Uuid.{field}` moved to the `std/uuid` module — \
                                     add `use std/uuid` and write `uuid.{field}`"
                                ));
                                return Ty::Error;
                            }
                            // Other prelude identifier field accesses are not
                            // value-level types tracked statically.
                            return Ty::Unknown(UnknownReason::InferenceLimitation);
                        }
                        // Agent handler reference: `Foo.process` — validate
                        // the handler exists and return an opaque delegate type.
                        NameKind::Agent => {
                            if let Some(agent) = self.agents.get(name)
                                && !agent.handlers.contains_key(field)
                            {
                                self.err(format!(
                                    "agent `{name}` has no handler `{field}`; \
                                     declared handlers: {}",
                                    if agent.handlers.is_empty() {
                                        "none".to_string()
                                    } else {
                                        agent
                                            .handlers
                                            .keys()
                                            .map(|s| s.as_str())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    }
                                ));
                            }
                            // Handler references are delegate targets, not values.
                            return Ty::Unknown(UnknownReason::InferenceLimitation);
                        }
                        // All other global categories (TopTask, TypeName, Unresolved)
                        // fall through to the value-level field-access path below.
                        _ => {}
                    }
                }
                let obj_ty = self.infer_expr(obj, scope);
                self.resolve_struct_field_checked(&obj_ty, field)
            }

            Expr::NullFieldAccess(obj, field) => {
                let obj_ty = self.infer_expr(obj, scope);
                Ty::Nullable(Box::new(self.resolve_struct_field_checked(&obj_ty, field)))
            }

            Expr::NullAssert(e) => {
                let ty = self.infer_expr(e, scope);
                match ty {
                    Ty::Nullable(inner) => *inner,
                    other => other,
                }
            }

            Expr::StructLit(fields) => {
                match self
                    .hir
                    .literal_kind(spanned)
                    .unwrap_or(LiteralKind::Struct)
                {
                    LiteralKind::InvalidMixedKeys => {
                        self.err(
                            "map literal has mixed key types — all keys must be \
                             the same type (str, int, or bool)"
                                .to_string(),
                        );
                        for (_, value) in fields {
                            self.infer_expr(value, scope);
                        }
                        Ty::Error
                    }
                    LiteralKind::Map(key_kind) => {
                        let key_ty = match key_kind {
                            MapKeyKind::Str => Ty::Str,
                            MapKeyKind::Int => Ty::Int,
                            MapKeyKind::Bool => Ty::Bool,
                        };
                        // Use Option<Ty> as the "not yet set" sentinel. A plain
                        // is_opaque() check would re-treat a legitimately opaque
                        // first value (e.g. Json.parse -> Unknown) as "unset".
                        let mut val_ty: Option<Ty> = None;
                        for (_, value) in fields {
                            let ty = self.infer_expr(value, scope);
                            match val_ty {
                                None => val_ty = Some(ty),
                                Some(ref expected) => {
                                    self.expect(&ty, expected, "map literal value");
                                }
                            }
                        }
                        let val_ty =
                            val_ty.unwrap_or(Ty::Unknown(UnknownReason::InferenceLimitation));
                        Ty::Map(Box::new(key_ty), Box::new(val_ty))
                    }
                    LiteralKind::Struct => {
                        let mut inferred: Vec<(String, Ty)> = Vec::with_capacity(fields.len());
                        for (key, value) in fields {
                            let ty = self.infer_expr(value, scope);
                            inferred.push((key.as_str().unwrap_or("").to_string(), ty));
                        }
                        Ty::Struct {
                            name: None,
                            fields: inferred,
                        }
                    }
                }
            }

            Expr::StructSpreadUpdate { base, overrides } => {
                let base_ty = self.infer_expr(base, scope);
                let (base_name, base_fields) = match base_ty.strip_nullable() {
                    Ty::Struct { name, fields } => (name.clone(), fields.clone()),
                    Ty::Map(key_ty, val_ty) => {
                        let key_ty = key_ty.clone();
                        let val_ty = val_ty.clone();
                        let mut seen: std::collections::HashSet<&str> =
                            std::collections::HashSet::new();
                        for (k, v) in overrides {
                            let vt = self.infer_expr(v, scope);
                            self.expect(&vt, &val_ty, "spread-update map value");
                            if !seen.insert(k.as_str()) {
                                self.err(format!(
                                    "duplicate key `{}` in spread-update — \
                                     each key may only be overridden once",
                                    k
                                ));
                            }
                        }
                        return Ty::Map(key_ty, val_ty);
                    }
                    other if other.is_opaque() => {
                        // Base type is opaque — infer overrides for side-effects
                        // but propagate the opaque kind as the result.
                        let result = other.clone();
                        for (_, v) in overrides {
                            self.infer_expr(v, scope);
                        }
                        return result;
                    }
                    other => {
                        self.err(format!(
                            "spread-update base must be a struct or map, got {}",
                            describe_ty(other)
                        ));
                        for (_, v) in overrides {
                            self.infer_expr(v, scope);
                        }
                        return Ty::Error;
                    }
                };
                let result_fields = base_fields.clone();
                let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
                for (k, v) in overrides {
                    let val_ty = self.infer_expr(v, scope);
                    if !seen.insert(k.as_str()) {
                        self.err(format!(
                            "duplicate field `{}` in spread-update — \
                             each field may only be overridden once",
                            k
                        ));
                        continue;
                    }
                    if let Some(pos) = result_fields.iter().position(|(f, _)| f == k) {
                        // Validate the override against the field's declared
                        // type (previously unchecked — any type silently
                        // replaced the field's static type here, mirroring
                        // the map branch above's existing `self.expect` call
                        // for the same purpose, #174). The result keeps the
                        // struct's *declared* field type, not the override
                        // expression's raw inferred type — e.g. overriding a
                        // `str?` field with a bare `none` must not narrow the
                        // result field's static type to `None_`.
                        let field_ty = result_fields[pos].1.clone();
                        self.expect(&val_ty, &field_ty, &format!("spread-update field `{k}`"));
                    } else {
                        self.err(format!(
                            "unknown field `{}` in spread-update — not present in base struct",
                            k
                        ));
                    }
                }
                Ty::Struct {
                    name: base_name,
                    fields: result_fields,
                }
            }

            Expr::ListLit(items) => {
                // Empty list — element type is indeterminate until used.
                let mut element_ty = Ty::Unknown(UnknownReason::InferenceLimitation);
                for (i, e) in items.iter().enumerate() {
                    let ty = self.infer_expr(e, scope);
                    if i == 0 {
                        element_ty = ty;
                    } else {
                        // Narrow nominal check: reject mixing differently-named
                        // struct types in the same list literal.  Structural and
                        // anonymous struct elements are left to the assignment
                        // expect_at to validate against the declared element type.
                        if let (Ty::Struct { name: Some(a), .. }, Ty::Struct { name: Some(b), .. }) =
                            (&ty, &element_ty)
                            && a != b
                        {
                            self.err_at(
                                format!("list element type mismatch: expected `{b}`, got `{a}`"),
                                e.span.clone(),
                            );
                        }
                    }
                }
                Ty::List(Box::new(element_ty))
            }

            Expr::SetLit(items) => {
                // Empty set — element type is indeterminate until used.
                let mut element_ty = Ty::Unknown(UnknownReason::InferenceLimitation);
                for (i, e) in items.iter().enumerate() {
                    let ty = self.infer_expr(e, scope);
                    if i == 0 {
                        element_ty = ty;
                    }
                }
                Ty::Set(Box::new(element_ty))
            }

            Expr::TupleLit(items) => {
                Ty::Tuple(items.iter().map(|e| self.infer_expr(e, scope)).collect())
            }

            Expr::BinaryOp { left, op, right } => {
                let l = self.infer_expr(left, scope);
                let r = self.infer_expr(right, scope);
                if let Some(msg) = check_binop(*op, &l, &r) {
                    self.err(msg);
                }
                infer_binary(*op, &l, &r)
            }

            Expr::UnaryOp { op, expr: inner } => {
                let t = self.infer_expr(inner, scope);
                match op {
                    UnOp::Neg => match t.strip_nullable() {
                        Ty::Int => Ty::Int,
                        Ty::Float => Ty::Float,
                        other if other.is_opaque() => other.clone(),
                        other => {
                            self.err(format!("cannot negate {}", describe_ty(other)));
                            Ty::Error
                        }
                    },
                    UnOp::Not => Ty::Bool,
                }
            }

            Expr::NullCoalesce(l, r) => {
                let l_ty = self.infer_expr(l, scope);
                let r_ty = self.infer_expr(r, scope);
                // `x ?? fallback` unwraps x's nullable wrapper; result is the
                // inner type of x (or fallback's type when x is Unknown).
                match l_ty {
                    Ty::Nullable(inner) => *inner,
                    other if other.is_opaque() => r_ty,
                    other => other,
                }
            }

            Expr::Index { object, index } => {
                let obj_ty = self.infer_expr(object, scope);
                let idx_ty = self.infer_expr(index, scope);
                match obj_ty.strip_nullable() {
                    Ty::Map(key_ty, val_ty) => {
                        let val_ty = val_ty.clone();
                        self.expect(&idx_ty, key_ty, "map subscript key");
                        // Missing key returns none, so result is nullable.
                        Ty::Nullable(val_ty)
                    }
                    Ty::List(elem) => {
                        let idx_base = idx_ty.strip_nullable();
                        if !matches!(idx_base, Ty::Int) && !idx_base.is_opaque() {
                            self.err(format!(
                                "subscript index must be int, got {}",
                                describe_ty(&idx_ty)
                            ));
                        }
                        *elem.clone()
                    }
                    Ty::Str => {
                        let idx_base = idx_ty.strip_nullable();
                        if !matches!(idx_base, Ty::Int) && !idx_base.is_opaque() {
                            self.err(format!(
                                "subscript index must be int, got {}",
                                describe_ty(&idx_ty)
                            ));
                        }
                        Ty::Str
                    }
                    other if other.is_opaque() => Ty::Unknown(UnknownReason::InferenceLimitation),
                    other => {
                        self.err(format!(
                            "subscript `[i]` is not supported on {}; \
                             lists and strings support subscript access",
                            describe_ty(other)
                        ));
                        Ty::Error
                    }
                }
            }

            Expr::Range(start, end) => {
                let s = self.infer_expr(start, scope);
                let e = self.infer_expr(end, scope);
                let sb = s.strip_nullable();
                if !matches!(sb, Ty::Int) && !sb.is_opaque() {
                    self.err(format!("range start must be int, got {}", describe_ty(&s)));
                }
                let eb = e.strip_nullable();
                if !matches!(eb, Ty::Int) && !eb.is_opaque() {
                    self.err(format!("range end must be int, got {}", describe_ty(&e)));
                }
                Ty::List(Box::new(Ty::Int))
            }

            Expr::Pipeline(l, r) => {
                let _ = self.infer_expr(l, scope);
                self.infer_expr(r, scope)
            }

            Expr::Call { callee, args } => {
                // Infer all arg types once; reuse for both arity and type checks.
                let arg_tys: Vec<Ty> = args
                    .iter()
                    .map(|a| self.infer_expr(&a.value, scope))
                    .collect();
                if let Expr::SelfAccess {
                    field: task_name, ..
                } = &callee.as_ref().kind
                {
                    if self.current_impl_type.is_some() {
                        self.err(impl_self_call_error(task_name));
                        return Ty::Error;
                    }
                    let Some(agent_name) = self.current_agent.clone() else {
                        self.err(format!("`self.{task_name}(...)` used outside an agent"));
                        return Ty::Error;
                    };
                    let Some(sig) = self
                        .agents
                        .get(&agent_name)
                        .and_then(|agent| agent.tasks.get(task_name))
                        .cloned()
                    else {
                        self.err(format!("agent `{agent_name}` has no task `{task_name}`"));
                        return Ty::Error;
                    };
                    let expected = sig.params.len();
                    let positional = args
                        .iter()
                        .filter(|arg| arg.name.is_none() && !arg.spread)
                        .count();
                    if !sig.variadic && positional > expected {
                        let param_names: Vec<String> =
                            sig.params.iter().map(|(name, _)| name.clone()).collect();
                        self.wrong_arity(
                            format!("{agent_name}.{task_name}"),
                            expected,
                            positional,
                            param_names,
                            spanned.span.clone(),
                        );
                    }
                    self.check_call_args(
                        &sig.params,
                        &sig.required,
                        sig.variadic,
                        args,
                        &arg_tys,
                        &format!("task `{agent_name}.{task_name}`"),
                        spanned.span.clone(),
                    );
                    return sig.return_type;
                }
                // Dispatch on callee identity via the name index when the callee
                // is a bare identifier.
                if let Expr::Ident(name) = &callee.as_ref().kind {
                    match self.hir.resolve_global(name).kind {
                        NameKind::TopTask => {
                            let sig = self.top_tasks.get(name).cloned();
                            if let Some(sig) = sig {
                                let expected = sig.params.len();
                                // Count only non-spread positional args; named args
                                // may map to params by name.
                                let positional: usize = args
                                    .iter()
                                    .filter(|a| a.name.is_none() && !a.spread)
                                    .count();
                                if !sig.variadic && positional > expected {
                                    let param_names: Vec<String> =
                                        sig.params.iter().map(|(n, _)| n.clone()).collect();
                                    self.wrong_arity(
                                        name.clone(),
                                        expected,
                                        positional,
                                        param_names,
                                        spanned.span.clone(),
                                    );
                                }
                                // For generic tasks, infer type params from argument types,
                                // substitute into param types, then check each arg.
                                if let Some(td) = self.generic_task_decls.get(name).cloned() {
                                    let mut type_env: HashMap<String, Ty> = HashMap::new();
                                    for (param, arg_ty) in td.params.iter().zip(arg_tys.iter()) {
                                        self.unify_type_params(
                                            &param.ty.kind,
                                            arg_ty,
                                            &td.type_params,
                                            &mut type_env,
                                        );
                                    }
                                    // Record the call-site instantiation, ordered by the
                                    // task's declared type params. Params the unifier
                                    // couldn't pin down (e.g. an unused type param) fall
                                    // back to Unknown rather than silently omitting a slot.
                                    let call_args: Vec<Ty> = td
                                        .type_params
                                        .iter()
                                        .map(|p| {
                                            type_env.get(p).cloned().unwrap_or(Ty::Unknown(
                                                UnknownReason::InferenceLimitation,
                                            ))
                                        })
                                        .collect();
                                    self.record_instantiation(name, call_args);
                                    let td_variadic = td.params.last().is_some_and(|p| p.variadic);
                                    let resolved_params: Vec<(String, Ty)> = td
                                        .params
                                        .iter()
                                        .map(|p| {
                                            (
                                                match &p.name {
                                                    crate::ast::Binding::Ident(s) => s.clone(),
                                                    _ => String::new(),
                                                },
                                                self.resolve_type_with_env(&p.ty.kind, &type_env),
                                            )
                                        })
                                        .collect();
                                    let resolved_required: Vec<bool> = td
                                        .params
                                        .iter()
                                        .map(|p| !p.variadic && p.default.is_none())
                                        .collect();
                                    self.check_call_args(
                                        &resolved_params,
                                        &resolved_required,
                                        td_variadic,
                                        args,
                                        &arg_tys,
                                        &format!("task `{name}`"),
                                        spanned.span.clone(),
                                    );
                                    if let Some(ret_node) = &td.return_type {
                                        return self
                                            .resolve_type_with_env(&ret_node.kind, &type_env);
                                    }
                                    return Ty::None_;
                                }
                                self.check_call_args(
                                    &sig.params,
                                    &sig.required,
                                    sig.variadic,
                                    args,
                                    &arg_tys,
                                    &format!("task `{name}`"),
                                    spanned.span.clone(),
                                );
                                return sig.return_type.clone();
                            }
                        }
                        NameKind::Module => {
                            self.err(format!(
                                "`{name}` is a module, not a function — call a member \
                                 like `{name}.<method>(...)`"
                            ));
                            return Ty::Error;
                        }
                        NameKind::PreludeNamespace => {
                            // std member imported unqualified:
                            // `use parse from std/json` then `parse(...)`.
                            if let Some(entry) = self.hir.std_fn_import(name) {
                                if !self.agent_capability_allows(
                                    entry.namespace,
                                    entry.name,
                                    name,
                                    spanned.span.clone(),
                                ) {
                                    return Ty::Error;
                                }
                                self.check_builtin_args(
                                    entry,
                                    args,
                                    &arg_tys,
                                    spanned.span.clone(),
                                );
                                return self.catalog_result_ty(entry, args);
                            }
                            // Validate delegate(...) targets at compile time.
                            if name == "delegate" {
                                return self.check_delegate_call(args, &arg_tys);
                            }
                            // Typed inference for prelude free functions.
                            match name.as_str() {
                                "typeof" => return Ty::Str,
                                "min" | "max" => {
                                    // Validate by: is a function if present.
                                    if let Some(by_ty) = args
                                        .iter()
                                        .zip(arg_tys.iter())
                                        .find(|(a, _)| a.name.as_deref() == Some("by"))
                                        .map(|(_, ty)| ty)
                                        && !matches!(by_ty, Ty::Func(..))
                                        && !by_ty.is_opaque()
                                    {
                                        self.err(format!(
                                            "`{name}`: `by:` must be a function, got `{}`",
                                            describe_ty(by_ty)
                                        ));
                                    }
                                    let positional_tys: Vec<Ty> = args
                                        .iter()
                                        .zip(arg_tys.iter())
                                        .filter(|(a, _)| a.name.is_none())
                                        .map(|(a, ty)| {
                                            if a.spread {
                                                match ty {
                                                    Ty::List(inner) | Ty::Set(inner) => {
                                                        *inner.clone()
                                                    }
                                                    _ => ty.clone(),
                                                }
                                            } else {
                                                ty.clone()
                                            }
                                        })
                                        .collect();
                                    let elem_ty = match positional_tys.as_slice() {
                                        // No positional args — element type is indeterminate.
                                        [] => Ty::Unknown(UnknownReason::InferenceLimitation),
                                        [Ty::List(inner)] => *inner.clone(),
                                        [single] => single.clone(),
                                        slice
                                            if slice
                                                .iter()
                                                .all(|t| self.types_match(t, &slice[0])) =>
                                        {
                                            slice[0].clone()
                                        }
                                        slice => {
                                            let types: Vec<String> =
                                                slice.iter().map(describe_ty).collect();
                                            self.err(format!(
                                                "`{name}`: arguments must all have the same \
                                                 type, got {}",
                                                types.join(", ")
                                            ));
                                            Ty::Error
                                        }
                                    };
                                    return Ty::Nullable(Box::new(elem_ty));
                                }
                                // Other prelude free identifiers called as functions —
                                // return type is indeterminate.
                                _ => {}
                            }
                        }
                        // Agent, TypeName, Enum, Unresolved — not callable by name;
                        // fall through to the generic callee inference path.
                        _ => {}
                    }
                }
                let _ = self.infer_expr(callee, scope);
                // Callee type not resolved — return type is also indeterminate.
                Ty::Unknown(UnknownReason::InferenceLimitation)
            }

            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                // Infer all arg types once; reuse for both arity and type checks.
                let arg_tys: Vec<Ty> = args
                    .iter()
                    .map(|a| self.infer_expr(&a.value, scope))
                    .collect();
                if let Expr::Ident(namespace) = &object.as_ref().kind
                    && namespace == "testing"
                    && method == "mock"
                {
                    if self.current_test_mocks.is_none() {
                        self.err("`testing.mock(...)` can only be used inside a test block");
                        return Ty::Error;
                    }
                    let Some(target_arg) = args.first() else {
                        self.err("`testing.mock(...)` requires a namespace method target");
                        return Ty::Error;
                    };
                    let Some((target_namespace, target_method)) =
                        mock_target_from_expr(&target_arg.value)
                    else {
                        self.err("`testing.mock(...)` requires a namespace method target");
                        return Ty::Error;
                    };
                    if prelude::catalog_method(target_namespace, target_method).is_none() {
                        self.err(format!(
                            "unknown mock target `{target_namespace}.{target_method}`"
                        ));
                        return Ty::Error;
                    }
                    return Ty::Unknown(UnknownReason::InferenceLimitation);
                }
                if method == "called_with"
                    && let Some((namespace, target_method)) = mock_target_from_expr(object)
                    && prelude::catalog_method(namespace, target_method).is_some()
                {
                    if self.current_test_mocks.as_ref().is_some_and(|mocks| {
                        mocks.contains(&(namespace.to_string(), target_method.to_string()))
                    }) {
                        return Ty::Bool;
                    }
                    self.err(format!(
                        "`{namespace}.{target_method}.called_with(...)` requires \
                         `testing.mock({namespace}.{target_method}).returns(...)` in the \
                         current test"
                    ));
                    return Ty::Error;
                }
                if matches!(&object.as_ref().kind, Expr::SelfRef) {
                    if self.current_impl_type.is_some() {
                        self.err(impl_self_call_error(method));
                        return Ty::Error;
                    }
                    let Some(agent_name) = self.current_agent.clone() else {
                        self.err(format!("`self.{method}(...)` used outside an agent"));
                        return Ty::Error;
                    };
                    let Some(sig) = self
                        .agents
                        .get(&agent_name)
                        .and_then(|agent| agent.tasks.get(method))
                        .cloned()
                    else {
                        self.err(format!("agent `{agent_name}` has no task `{method}`"));
                        return Ty::Error;
                    };
                    let expected = sig.params.len();
                    let positional = args
                        .iter()
                        .filter(|arg| arg.name.is_none() && !arg.spread)
                        .count();
                    if !sig.variadic && positional > expected {
                        let param_names: Vec<String> =
                            sig.params.iter().map(|(name, _)| name.clone()).collect();
                        self.wrong_arity(
                            format!("{agent_name}.{method}"),
                            expected,
                            positional,
                            param_names,
                            spanned.span.clone(),
                        );
                    }
                    self.check_call_args(
                        &sig.params,
                        &sig.required,
                        sig.variadic,
                        args,
                        &arg_tys,
                        &format!("task `{agent_name}.{method}`"),
                        spanned.span.clone(),
                    );
                    return sig.return_type;
                }
                // Resolve ident-qualified method calls through the name index.
                // Lexical locals shadow globals (e.g. `db = db.connect(...)`).
                if let Expr::Ident(name) = &object.as_ref().kind
                    && scope.get(name).is_none()
                {
                    match self.hir.resolve_global(name).kind {
                        NameKind::Agent => {
                            // Direct agent-task calls are not supported; callers
                            // must use `self.task(...)` or the mailbox API.
                            self.err(format!(
                                "direct agent task calls like `{name}.{method}(...)` \
                                 are unsupported; use `self.{method}(...)` inside that \
                                 agent or mailbox APIs such as `send(...)` / \
                                 `delegate(...)`"
                            ));
                            return Ty::Error;
                        }
                        // Module member call: `file.read(...)`, `validation.email(...)`.
                        NameKind::Module => {
                            return self.module_method_call(
                                name,
                                method,
                                args,
                                &arg_tys,
                                spanned.span.clone(),
                            );
                        }
                        NameKind::PreludeNamespace => {
                            // Tombstone: `Uuid` is still the built-in type, but its
                            // constructors moved to `std/uuid`.
                            if name == "Uuid" {
                                self.err(format!(
                                    "`Uuid.{method}(...)` moved to the `std/uuid` module — \
                                     add `use std/uuid` and write `uuid.{method}(...)`"
                                ));
                                return Ty::Error;
                            }
                            // A hint identifier (`json`, `text`, …) shadowing a std
                            // module name: the call only makes sense on the module.
                            if prelude::catalog_method(name, method.as_str()).is_some() {
                                self.err(format!(
                                    "`{name}` is not imported — add `use std/{name}` to \
                                     call `{name}.{method}(...)`"
                                ));
                                return Ty::Error;
                            }
                        }
                        // TopTask, TypeName, Enum, Unresolved, or a local that was
                        // bound to a non-namespace value — fall through to value-level
                        // method dispatch below.
                        _ => {}
                    }
                }
                let obj_ty = self.infer_expr(object, scope);
                match (obj_ty.strip_nullable(), method.as_str()) {
                    (Ty::List(elem), "push" | "filter" | "sort" | "reverse" | "take" | "skip") => {
                        Ty::List(elem.clone())
                    }
                    (Ty::List(_), "flatten") => {
                        Ty::List(Box::new(Ty::Unknown(UnknownReason::InferenceLimitation)))
                    }
                    (Ty::List(_), "len" | "count") => Ty::Int,
                    (Ty::List(_), "is_empty") => Ty::Bool,
                    (Ty::List(_), "contains" | "any" | "all") => Ty::Bool,
                    (Ty::List(elem), "first" | "last" | "find") => Ty::Nullable(elem.clone()),
                    (Ty::List(elem_a), "zip") => {
                        let elem_b = match arg_tys.first().map(|ty| ty.strip_nullable()) {
                            Some(Ty::List(e)) => *e.clone(),
                            Some(other) => {
                                self.err(format!(
                                    "`.zip()` expects a list argument, got {}",
                                    describe_ty(other)
                                ));
                                Ty::Error
                            }
                            None => Ty::Unknown(UnknownReason::InferenceLimitation),
                        };
                        Ty::List(Box::new(Ty::Tuple(vec![*elem_a.clone(), elem_b])))
                    }
                    (Ty::List(_), "map") => {
                        Ty::List(Box::new(Ty::Unknown(UnknownReason::InferenceLimitation)))
                    }
                    (Ty::List(_), "reduce" | "sum" | "min" | "max") => {
                        Ty::Unknown(UnknownReason::InferenceLimitation)
                    }
                    (Ty::List(_), "join") => Ty::Str,
                    (Ty::Str, "len" | "count" | "length") => Ty::Int,
                    (
                        Ty::Str,
                        "upper" | "lower" | "trim" | "strip" | "trim_start" | "trim_end" | "repeat"
                        | "slice" | "replace" | "to_str",
                    ) => Ty::Str,
                    (Ty::Str, "split") => Ty::List(Box::new(Ty::Str)),
                    (Ty::Str, "contains" | "starts_with" | "ends_with" | "is_empty") => Ty::Bool,
                    (Ty::Str, "to_int") => Ty::Nullable(Box::new(Ty::Int)),
                    (Ty::Str, "to_float") => Ty::Nullable(Box::new(Ty::Float)),
                    (Ty::Str, "index_of") => Ty::Nullable(Box::new(Ty::Int)),
                    (Ty::Str, "truncate" | "pad" | "sub") => Ty::Str,
                    (Ty::Str, "matches") => Ty::Bool,
                    (Ty::Str, "extract") => Ty::Nullable(Box::new(Ty::Str)),
                    (Ty::Str, "find_all") => Ty::List(Box::new(Ty::Str)),
                    (Ty::Map(_, v), "get") => Ty::Nullable(v.clone()),
                    (Ty::Map(k, _), "keys") => Ty::List(k.clone()),
                    (Ty::Map(_, v), "values") => Ty::List(v.clone()),
                    (Ty::Map(_, _), "len" | "count" | "size") => Ty::Int,
                    (Ty::Map(_, _), "is_empty") => Ty::Bool,
                    (Ty::Map(_, _), "contains" | "has") => Ty::Bool,
                    // Value methods: `.insert`/`.add` return a fresh container
                    // rather than mutating the receiver, exactly like `.push`.
                    (Ty::Map(k, v), "insert") => Ty::Map(k.clone(), v.clone()),
                    (Ty::Set(elem), "add") => Ty::Set(elem.clone()),
                    // `push` is deliberately not a set method (it would hand
                    // back a list). Catching it here turns what would be a
                    // runtime "method not available" into a fix-it.
                    (Ty::Set(_), "push") => {
                        self.err(
                            "`push` is a list method — use `.add(v)` to add an element to a set"
                                .to_string(),
                        );
                        Ty::Error
                    }
                    (Ty::Set(_), "len" | "count" | "size") => Ty::Int,
                    (Ty::Set(_), "is_empty") => Ty::Bool,
                    (Ty::Set(_), "contains") => Ty::Bool,
                    // The read-only list pipeline a set borrows at runtime
                    // (`interpreter/methods.rs`'s `SET_LIST_METHODS`) yields
                    // lists, never sets — inference here is as shallow as the
                    // list arms it mirrors, deliberately.
                    (Ty::Set(elem), "sort" | "reverse" | "filter" | "take" | "skip") => {
                        Ty::List(elem.clone())
                    }
                    (Ty::Set(elem), "first" | "last" | "find") => Ty::Nullable(elem.clone()),
                    (Ty::Set(_), "any" | "all") => Ty::Bool,
                    (Ty::Set(_), "join") => Ty::Str,
                    (Ty::Set(_), "map" | "flatten") => {
                        Ty::List(Box::new(Ty::Unknown(UnknownReason::InferenceLimitation)))
                    }
                    (Ty::Int, "abs" | "floor" | "ceil" | "round") => Ty::Int,
                    (Ty::Float, "abs" | "floor" | "ceil" | "round") => Ty::Float,
                    // Datetime.parts returns a struct whose fields depend on the
                    // locale/format — not yet modelled statically.
                    (Ty::Datetime, "parts") => Ty::Unknown(UnknownReason::InferenceLimitation),
                    (Ty::Datetime, "format") => Ty::Nullable(Box::new(Ty::Str)),
                    (Ty::Uuid, "to_str" | "format") => Ty::Str,
                    (Ty::Uuid, "version") => Ty::Int,
                    (Ty::DbConnection, "query") => {
                        Ty::List(Box::new(Ty::Map(Box::new(Ty::Str), Box::new(Ty::Dynamic))))
                    }
                    (Ty::DbConnection, "exec") => Ty::Int,
                    // Method not in the static dispatch table — shallow inference.
                    _ => Ty::Unknown(UnknownReason::InferenceLimitation),
                }
            }

            Expr::Cast { expr, ty } => {
                self.infer_expr(expr, scope);
                self.resolve_type(&ty.kind)
            }

            Expr::IfExpr {
                cond,
                then_body,
                else_body,
            } => {
                let c = self.infer_expr(cond, scope);
                self.expect(&c, &Ty::Bool, "`if` condition");
                let then_v = self.block_value(then_body, scope);
                let else_v = self.block_value(else_body, scope);
                // Every `Expr::IfExpr` is in value position — the statement
                // form parses to `Stmt::If` (whose `else_body` is an
                // `Option<Block>`), so a branch that produces no value here is
                // always a defect, never a plain `if` statement being checked.
                // A missing `else` reaches this as an empty block: the parser
                // folds its absence into `else_body.unwrap_or_default()`.
                match (then_v, else_v) {
                    (BlockValue::NoValue, _) => {
                        self.err_valueless_branch(
                            then_body,
                            "the `then` branch of this `if` expression",
                            &spanned.span,
                        );
                        Ty::Error
                    }
                    (_, BlockValue::NoValue) => {
                        self.err_valueless_branch(
                            else_body,
                            "the `else` branch of this `if` expression",
                            &spanned.span,
                        );
                        Ty::Error
                    }
                    // Both branches leave via `return`/`raise`, so the `if`
                    // never yields a value to anyone and any binding it feeds
                    // is unreachable. Opaque rather than `Ty::None_`: `None_`
                    // is a real type that would go on to fail against the
                    // binding's use ("expected int, got none") for dead code
                    // that never runs.
                    (BlockValue::Diverges, BlockValue::Diverges) => {
                        Ty::Unknown(UnknownReason::InferenceLimitation)
                    }
                    (BlockValue::Diverges, BlockValue::Value(ty))
                    | (BlockValue::Value(ty), BlockValue::Diverges) => ty,
                    (BlockValue::Value(then_ty), BlockValue::Value(else_ty)) => {
                        if !then_ty.is_opaque() && !else_ty.is_opaque() {
                            self.expect(
                                &else_ty,
                                &then_ty,
                                "`if` branches must have the same type",
                            );
                        }
                        then_ty
                    }
                }
            }

            Expr::WhenExpr { subject, arms } => {
                let subject_ty = self.infer_expr(subject, scope);
                let when_span = self.current_span.clone().unwrap_or_default();
                // Reuse exhaustiveness checking from the statement path.
                self.check_when_arms(&subject_ty, arms, scope, when_span);
                // Unify arm result types. `None` until an arm actually
                // contributes one, so that "no arm has been seen yet" is not
                // spelled with a real type that could be mistaken for an arm's
                // own — which is what let a `when` whose every arm diverges type
                // as `none` and then fail against the binding's use.
                let mut result_ty: Option<Ty> = None;
                for arm in arms {
                    scope.push();
                    for p in &arm.patterns {
                        match p {
                            Pattern::Variant {
                                name: variant_name,
                                bindings,
                            } => {
                                for (idx, b) in bindings.iter().enumerate() {
                                    if b == "_" {
                                        continue;
                                    }
                                    let field_ty = self.resolve_variant_field(
                                        &subject_ty,
                                        variant_name,
                                        b,
                                        idx,
                                    );
                                    scope.define(b.clone(), field_ty);
                                }
                            }
                            Pattern::Struct { fields } => {
                                for f in fields {
                                    if f == "_" {
                                        continue;
                                    }
                                    let field_ty = self.resolve_struct_field(&subject_ty, f);
                                    scope.define(f.clone(), field_ty);
                                }
                            }
                            _ => {}
                        }
                    }
                    let arm_v = self.block_value(&arm.body, scope);
                    scope.pop();
                    let arm_ty = match arm_v {
                        // Same defect as an `if` branch that produces no value:
                        // the arm evaluates to `none` at runtime whatever type
                        // the other arms pinned (issue #197).
                        BlockValue::NoValue => {
                            self.err_valueless_branch(
                                &arm.body,
                                "this `when` expression arm",
                                &spanned.span,
                            );
                            Ty::Error
                        }
                        // An arm that exits via `return`/`raise` contributes no
                        // type, and legitimately so — the remaining arms decide.
                        BlockValue::Diverges => continue,
                        BlockValue::Value(ty) => ty,
                    };
                    match &result_ty {
                        None => result_ty = Some(arm_ty),
                        Some(pinned) => {
                            if !arm_ty.is_opaque() && !pinned.is_opaque() {
                                self.expect(
                                    &arm_ty,
                                    &pinned.clone(),
                                    "`when` expression arms must all have the same type",
                                );
                            }
                        }
                    }
                }
                // No arm contributed a type — every one of them diverged, so the
                // expression is unreachable. Opaque rather than `Ty::None_`, for
                // the reason given on the `if` both-diverge arm above.
                result_ty.unwrap_or(Ty::Unknown(UnknownReason::InferenceLimitation))
            }

            Expr::Lambda { params, body } => {
                scope.push_lambda();
                for p in params {
                    // Unannotated lambda parameters — type inference is not yet
                    // implemented; bind as InferenceLimitation to avoid cascade errors.
                    let ty =
                        p.ty.as_ref()
                            .map(|t| self.resolve_type(&t.kind))
                            .unwrap_or(Ty::Unknown(UnknownReason::InferenceLimitation));
                    scope.define(p.name.clone(), ty);
                }
                let ret = match body {
                    LambdaBody::Expr(e) => self.infer_expr(e, scope),
                    LambdaBody::Block(b) => {
                        let mut last = Ty::Unknown(UnknownReason::InferenceLimitation);
                        for s_node in b {
                            last = match &s_node.kind {
                                Stmt::Expr(e) => self.infer_expr(e, scope),
                                other => {
                                    self.check_stmt(other, s_node.span.clone(), scope);
                                    Ty::Unknown(UnknownReason::InferenceLimitation)
                                }
                            };
                        }
                        last
                    }
                };
                scope.pop_lambda();
                Ty::Func(
                    params
                        .iter()
                        .map(|p| {
                            p.ty.as_ref()
                                .map(|t| self.resolve_type(&t.kind))
                                .unwrap_or(Ty::Unknown(UnknownReason::InferenceLimitation))
                        })
                        .collect(),
                    Box::new(ret),
                )
            }

            Expr::Duration { value, .. } => {
                self.infer_expr(value, scope);
                Ty::Duration
            }

            Expr::EnumVariant {
                ty: name,
                variant,
                fields,
            } => {
                if let Some(variants) = self.enum_variants.get(name)
                    && !variants.contains(variant)
                {
                    self.err(format!("enum `{name}` has no variant `{variant}`"));
                }
                for (_, v) in fields {
                    self.infer_expr(v, scope);
                }
                Ty::Enum(name.clone(), vec![])
            }
        }
    }

    /// Type a non-call member access on a module binding:
    /// `uuid.DNS`, `validation.email` (task as value), `validation.Watcher`.
    fn module_field_access(&mut self, binding: &str, field: &str) -> Ty {
        let Some(members) = self.module_members.get(binding).cloned() else {
            // Single-file mode (REPL, in-memory check) has no member table;
            // member existence is validated at runtime.
            return Ty::Unknown(UnknownReason::InferenceLimitation);
        };
        match members {
            super::ModuleMembers::Std(ns) => {
                // `std/uuid` exposes namespace-constant UUIDs as fields.
                if ns == "uuid" && matches!(field, "DNS" | "URL" | "OID" | "X500") {
                    return Ty::Uuid;
                }
                // Other std member accesses (mock targets, method references)
                // are not value-level types tracked statically.
                Ty::Unknown(UnknownReason::InferenceLimitation)
            }
            super::ModuleMembers::Local {
                module_name,
                tasks,
                agents,
                types,
            } => {
                if tasks.contains(field) {
                    if let Some(sig) = self.top_tasks.get(field) {
                        return Ty::Func(
                            sig.params.iter().map(|(_, ty)| ty.clone()).collect(),
                            Box::new(sig.return_type.clone()),
                        );
                    }
                    return Ty::Unknown(UnknownReason::InferenceLimitation);
                }
                if agents.contains(field) {
                    // Agent reference — usable with run()/stop()/send().
                    return Ty::Unknown(UnknownReason::InferenceLimitation);
                }
                if types.contains(field) {
                    self.err(format!(
                        "types are not accessed through the module namespace — \
                         import it directly: use {field} from \"./{module_name}.keel\""
                    ));
                    return Ty::Error;
                }
                self.err(format!("module `{module_name}` has no member `{field}`"));
                Ty::Error
            }
        }
    }

    /// Type a member call on a module binding:
    /// `file.read(...)`, `validation.email(...)`.
    fn module_method_call(
        &mut self,
        binding: &str,
        method: &str,
        args: &[CallArg],
        arg_tys: &[Ty],
        span: crate::lexer::Span,
    ) -> Ty {
        let Some(members) = self.module_members.get(binding).cloned() else {
            return Ty::Unknown(UnknownReason::InferenceLimitation);
        };
        match members {
            super::ModuleMembers::Std(ns) => match prelude::catalog_method(&ns, method) {
                Some(entry) => {
                    if !self.agent_capability_allows(&ns, method, binding, span.clone()) {
                        return Ty::Error;
                    }
                    if ns == "ai" && method == "install" {
                        self.check_ai_install_arg(args, span.clone());
                    }
                    self.check_builtin_args(entry, args, arg_tys, span.clone());
                    self.catalog_result_ty(entry, args)
                }
                None => {
                    self.err_at(format!("`std/{ns}` has no method `{method}`"), span);
                    Ty::Error
                }
            },
            super::ModuleMembers::Local {
                module_name,
                tasks,
                agents,
                ..
            } => {
                if tasks.contains(method) {
                    if let Some(sig) = self.top_tasks.get(method).cloned() {
                        self.check_call_args(
                            &sig.params,
                            &sig.required,
                            sig.variadic,
                            args,
                            arg_tys,
                            &format!("task `{module_name}.{method}`"),
                            span.clone(),
                        );
                        return sig.return_type;
                    }
                    return Ty::Unknown(UnknownReason::InferenceLimitation);
                }
                if agents.contains(method) {
                    self.err_at(
                        format!(
                            "`{binding}.{method}` is an agent, not a task — start it \
                             with run({binding}.{method}) or message it with send(...)"
                        ),
                        span,
                    );
                    return Ty::Error;
                }
                self.err_at(
                    format!("module `{module_name}` has no member `{method}`"),
                    span,
                );
                Ty::Error
            }
        }
    }

    /// Validate the argument to `ai.install(X)`: `X` must be a bare type name
    /// that implements `LlmProvider`. Unlike `@provider X`, a built-in backend
    /// name is *not* accepted here — built-ins are selected with `@provider` or a
    /// `provider:` model prefix, and a bare backend name is not a value the
    /// interpreter can install (it resolves to nothing at runtime).
    fn check_ai_install_arg(&mut self, args: &[CallArg], span: crate::lexer::Span) {
        let Some(first) = args.first() else {
            self.err_at(
                "ai.install expects a provider type, e.g. `ai.install(MyProvider)`",
                span,
            );
            return;
        };
        let Expr::Ident(name) = &first.value.kind else {
            self.err_at(
                "ai.install expects a provider type name, e.g. `ai.install(MyProvider)`",
                first.value.span.clone(),
            );
            return;
        };
        if keel_catalog::is_builtin_llm_provider(name) {
            self.err_at(
                format!(
                    "ai.install expects a user-authored provider type implementing \
                     `LlmProvider`, not the built-in backend `{name}` — select a built-in \
                     with `@provider {name}` or a `{name}:` model prefix instead"
                ),
                first.value.span.clone(),
            );
            return;
        }
        self.check_provider_name(name, first.value.span.clone());
    }

    /// Enforce deny-by-default `@tools` at compile time for direct std calls
    /// inside an agent body. Returns `false` (after emitting the error) when
    /// the current agent's `@tools` does not cover `ns.method`. Outside an
    /// agent context, all calls pass — gating is an agent contract.
    ///
    /// Only effectful modules are gated; pure-compute modules pass freely
    /// (see `keel_catalog::module_requires_capability`).
    fn agent_capability_allows(
        &mut self,
        ns: &str,
        method: &str,
        binding: &str,
        span: crate::lexer::Span,
    ) -> bool {
        if !keel_catalog::module_requires_capability(ns) {
            return true;
        }
        let Some(agent_name) = self.current_agent.clone() else {
            return true;
        };
        let Some(info) = self.agents.get(&agent_name) else {
            return true;
        };
        if info.allows_tool(ns, method) {
            return true;
        }
        let hint = if info.tools.is_none() {
            format!("declare `@tools [{ns}]` on the agent, or use `@tools all`")
        } else {
            format!("add `{ns}` to @tools, or use `@tools all`")
        };
        self.err_at(
            format!("agent `{agent_name}` calls `{binding}.{method}` but @tools does not allow it — {hint}"),
            span,
        );
        false
    }

    /// Resolve a catalog entry's declared result into a checker type,
    /// applying the special `as:`-driven inference rules for `ai.classify`
    /// and `ai.extract`.
    fn catalog_result_ty(
        &mut self,
        entry: &'static crate::builtins::BuiltinMethod,
        args: &[CallArg],
    ) -> Ty {
        match entry.result {
            BuiltinResult::Fixed(spec) => prelude::ty_from_spec(spec),
            BuiltinResult::AiClassify => {
                // Return `Nullable(Enum(as:))` when the `as:` argument
                // names a known enum type; otherwise the target is unknown.
                if let Some(as_arg) = args.iter().find(|a| a.name.as_deref() == Some("as"))
                    && let Expr::Ident(enum_name) = &as_arg.value.kind
                    && matches!(self.hir.resolve_global(enum_name).kind, NameKind::Enum)
                {
                    Ty::Nullable(Box::new(Ty::Enum(enum_name.clone(), vec![])))
                } else {
                    Ty::Unknown(UnknownReason::InferenceLimitation)
                }
            }
            BuiltinResult::AiExtract => {
                // Return `Nullable(resolve_type(as:))` when the `as:`
                // argument names a resolvable type.
                let inner = args
                    .iter()
                    .find(|a| a.name.as_deref() == Some("as"))
                    .map(|a| {
                        if let Expr::Ident(type_name) = &a.value.kind {
                            self.resolve_type(&TypeExpr::Named(type_name.clone()))
                        } else {
                            Ty::Unknown(UnknownReason::InferenceLimitation)
                        }
                    })
                    .unwrap_or(Ty::Unknown(UnknownReason::InferenceLimitation));
                Ty::Nullable(Box::new(inner))
            }
            // Runtime-dynamic: return type depends on external context.
            BuiltinResult::Unknown => Ty::Unknown(UnknownReason::ExternalDynamic),
        }
    }

    /// Validate `delegate(...)` call sites at compile time.
    ///
    /// Symbol form: `delegate(Foo.handle, data)`
    ///   — arg[0] is FieldAccess(Ident("Foo"), "handle")
    ///   — arg[1] is the data; type-checked against the handler param
    /// String form: `delegate(Foo, "handle", data)`
    ///   — arg[0] is Ident("Foo")
    ///   — arg[1] is a plain string literal naming the handler
    ///   — arg[2] is the data; type-checked against the handler param
    fn check_delegate_call(&mut self, args: &[CallArg], arg_tys: &[Ty]) -> Ty {
        if let Some(first_arg) = args.first() {
            match first_arg.value.kind.clone() {
                // Symbol form: Foo.handle
                Expr::FieldAccess(obj_expr, handler_name) => {
                    if let Expr::Ident(agent_name) = &obj_expr.as_ref().kind
                        && let Some(agent) = self.agents.get(agent_name)
                    {
                        // Handler existence already checked in the
                        // FieldAccess arm of infer_expr.
                        if args.get(1).is_some()
                            && let Some(Some(param_ty)) = agent.handlers.get(&handler_name).cloned()
                        {
                            self.expect(
                                &arg_tys[1],
                                &param_ty,
                                &format!("argument to `{agent_name}.{handler_name}`"),
                            );
                        }
                    }
                }
                // String form: Foo, "handle"
                Expr::Ident(agent_name)
                    if matches!(self.hir.resolve_global(&agent_name).kind, NameKind::Agent) =>
                {
                    if let Some(second_arg) = args.get(1)
                        && let Expr::StringLit(parts) = &second_arg.value.kind
                    {
                        // Only check plain string literals (no interpolation)
                        // for static resolution.
                        let maybe_handler: Option<String> =
                            parts.iter().try_fold(String::new(), |acc, p| match p {
                                crate::ast::StringPart::Literal(s) => Some(acc + s),
                                _ => None,
                            });
                        if let Some(handler_name) = maybe_handler
                            && let Some(agent) = self.agents.get(&agent_name).cloned()
                        {
                            if !agent.handlers.contains_key(&handler_name) {
                                self.err(format!(
                                    "agent `{agent_name}` has no handler `{handler_name}`; \
                                     declared handlers: {}",
                                    if agent.handlers.is_empty() {
                                        "none".to_string()
                                    } else {
                                        agent
                                            .handlers
                                            .keys()
                                            .map(|s| s.as_str())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    }
                                ));
                            } else if args.get(2).is_some()
                                && let Some(Some(param_ty)) =
                                    agent.handlers.get(&handler_name).cloned()
                            {
                                self.expect(
                                    &arg_tys[2],
                                    &param_ty,
                                    &format!("argument to `{agent_name}.{handler_name}`"),
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ty::None_
    }
}
