//! Expression type inference for the type checker.
//!
//! `infer_expr` recursively infers the [`Ty`] of every expression in the AST.
//! The inference is deliberately shallow — when a type cannot be computed cheaply
//! it falls back to [`Ty::Unknown`] and no error is emitted.  High-signal
//! diagnostics (undefined identifiers, wrong operator types, arity mismatches,
//! non-exhaustive `when`) are reported as errors via [`Checker::err`].

use std::collections::HashMap;

use crate::ast::*;
use crate::types::prelude::{self, BuiltinResult};
use crate::types::scope::Scope;
use crate::types::ty::{describe_ty, Ty};

use super::{
    binop::{check_binop, infer_binary},
    Checker,
};

impl Checker {
    pub(crate) fn infer_expr(&mut self, expr: &Expr, scope: &mut Scope) -> Ty {
        match expr {
            Expr::Integer(_) => Ty::Int,
            Expr::Float(_) => Ty::Float,
            Expr::Bool(_) => Ty::Bool,
            Expr::None_ => Ty::None_,

            Expr::StringLit(parts) => {
                for p in parts {
                    if let StringPart::Interpolation(e, _spec) = p {
                        self.infer_expr(e, scope);
                    }
                }
                Ty::Str
            }

            Expr::Ident(name) => {
                if let Some(t) = scope.get(name) {
                    return t.clone();
                }
                if let Some(t) = self.top_tasks.get(name) {
                    return Ty::Func(
                        t.params.iter().map(|(_, ty)| ty.clone()).collect(),
                        Box::new(t.return_type.clone()),
                    );
                }
                if self.agents.contains_key(name) {
                    return Ty::Unknown; // AgentRef placeholder
                }
                if self.enum_variants.contains_key(name)
                    || self.structs.contains_key(name)
                    || self.aliases.contains_key(name)
                    || self.prelude.contains(name)
                {
                    return Ty::Unknown;
                }
                self.err(format!("undefined: `{name}`"));
                Ty::Unknown
            }

            Expr::SelfAccess(field) => {
                let Some(agent_name) = self.current_agent.clone() else {
                    self.err(format!("`self.{field}` used outside an agent"));
                    return Ty::Unknown;
                };
                if let Some(t) = self
                    .agents
                    .get(&agent_name)
                    .and_then(|a| a.state_fields.get(field))
                {
                    return t.clone();
                }
                self.err(format!("agent `{agent_name}` has no state field `{field}`"));
                Ty::Unknown
            }

            Expr::SelfRef => Ty::Unknown,

            Expr::FieldAccess(obj, field) => {
                // Enum variant shortcut: `Urgency.medium`.
                if let Expr::Ident(name) = obj.as_ref() {
                    if let Some(variants) = self.enum_variants.get(name) {
                        if !variants.contains(field) {
                            self.err(format!("enum `{name}` has no variant `{field}`"));
                        }
                        return Ty::Enum(name.clone(), vec![]);
                    }
                    if self.prelude.contains(name) {
                        if name == "Uuid"
                            && matches!(field.as_str(), "DNS" | "URL" | "OID" | "X500")
                        {
                            return Ty::Uuid;
                        }
                        return Ty::Unknown;
                    }
                    // Agent handler reference: `Foo.process` — validate the handler exists.
                    if let Some(agent) = self.agents.get(name) {
                        if !agent.handlers.contains_key(field) {
                            self.err(format!(
                                "agent `{name}` has no handler `{field}`; \
                                 declared handlers: {}",
                                if agent.handlers.is_empty() {
                                    "none".to_string()
                                } else {
                                    agent.handlers.keys()
                                        .map(|s| s.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                }
                            ));
                        }
                        return Ty::Unknown; // handler-ref placeholder
                    }
                }
                let obj_ty = self.infer_expr(obj, scope);
                match obj_ty.strip_nullable() {
                    Ty::Struct(fields) => fields
                        .iter()
                        .find(|(n, _)| n == field)
                        .map(|(_, t)| t.clone())
                        .unwrap_or(Ty::Unknown),
                    _ => Ty::Unknown,
                }
            }

            Expr::NullFieldAccess(obj, field) => {
                let obj_ty = self.infer_expr(obj, scope);
                let field_ty = match obj_ty.strip_nullable() {
                    Ty::Struct(fields) => fields
                        .iter()
                        .find(|(n, _)| n == field)
                        .map(|(_, t)| t.clone())
                        .unwrap_or(Ty::Unknown),
                    _ => Ty::Unknown,
                };
                Ty::Nullable(Box::new(field_ty))
            }

            Expr::NullAssert(e) => {
                let ty = self.infer_expr(e, scope);
                match ty {
                    Ty::Nullable(inner) => *inner,
                    other => other,
                }
            }

            Expr::StructLit(fields) => {
                use crate::ast::MapLitKey;
                let has_int = fields.iter().any(|(k, _)| matches!(k, MapLitKey::Int(_)));
                let has_bool = fields.iter().any(|(k, _)| matches!(k, MapLitKey::Bool(_)));
                let has_str = fields
                    .iter()
                    .any(|(k, _)| matches!(k, MapLitKey::Ident(_) | MapLitKey::Str(_)));

                if has_int || has_bool {
                    if (has_int && has_bool) || (has_str && (has_int || has_bool)) {
                        self.err(
                            "map literal has mixed key types — all keys must be \
                             the same type (str, int, or bool)"
                                .to_string(),
                        );
                        for (_, v) in fields {
                            self.infer_expr(v, scope);
                        }
                        return Ty::Unknown;
                    }
                    let key_ty = if has_int { Ty::Int } else { Ty::Bool };
                    let mut val_ty = Ty::Unknown;
                    for (_, v) in fields {
                        let t = self.infer_expr(v, scope);
                        if val_ty == Ty::Unknown {
                            val_ty = t;
                        } else {
                            self.expect(&t, &val_ty, "map literal value");
                        }
                    }
                    Ty::Map(Box::new(key_ty), Box::new(val_ty))
                } else {
                    let mut inferred: Vec<(String, Ty)> = Vec::with_capacity(fields.len());
                    for (k, v) in fields {
                        let ty = self.infer_expr(v, scope);
                        inferred.push((k.as_str().unwrap_or("").to_string(), ty));
                    }
                    Ty::Struct(inferred)
                }
            }

            Expr::StructSpreadUpdate { base, overrides } => {
                let base_ty = self.infer_expr(base, scope);
                let base_fields = match base_ty.strip_nullable() {
                    Ty::Struct(fields) => fields.clone(),
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
                    Ty::Unknown | Ty::Dynamic => {
                        for (_, v) in overrides {
                            self.infer_expr(v, scope);
                        }
                        return Ty::Unknown;
                    }
                    other => {
                        self.err(format!(
                            "spread-update base must be a struct or map, got {}",
                            describe_ty(other)
                        ));
                        for (_, v) in overrides {
                            self.infer_expr(v, scope);
                        }
                        return Ty::Unknown;
                    }
                };
                let mut result_fields = base_fields.clone();
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
                        result_fields[pos] = (k.clone(), val_ty);
                    } else {
                        self.err(format!(
                            "unknown field `{}` in spread-update — not present in base struct",
                            k
                        ));
                    }
                }
                Ty::Struct(result_fields)
            }

            Expr::ListLit(items) => {
                let mut element_ty = Ty::Unknown;
                for (i, e) in items.iter().enumerate() {
                    let ty = self.infer_expr(e, scope);
                    if i == 0 {
                        element_ty = ty;
                    }
                }
                Ty::List(Box::new(element_ty))
            }

            Expr::SetLit(items) => {
                let mut element_ty = Ty::Unknown;
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
                        Ty::Unknown | Ty::Dynamic => Ty::Unknown,
                        other => {
                            self.err(format!("cannot negate {}", describe_ty(other)));
                            Ty::Unknown
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
                    Ty::Unknown | Ty::Dynamic => r_ty,
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
                        if !matches!(idx_ty.strip_nullable(), Ty::Int | Ty::Unknown | Ty::Dynamic) {
                            self.err(format!(
                                "subscript index must be int, got {}",
                                describe_ty(&idx_ty)
                            ));
                        }
                        *elem.clone()
                    }
                    Ty::Str => {
                        if !matches!(idx_ty.strip_nullable(), Ty::Int | Ty::Unknown | Ty::Dynamic) {
                            self.err(format!(
                                "subscript index must be int, got {}",
                                describe_ty(&idx_ty)
                            ));
                        }
                        Ty::Str
                    }
                    Ty::Unknown | Ty::Dynamic => Ty::Unknown,
                    other => {
                        self.err(format!(
                            "subscript `[i]` is not supported on {}; \
                             lists and strings support subscript access",
                            describe_ty(other)
                        ));
                        Ty::Unknown
                    }
                }
            }

            Expr::Range(start, end) => {
                let s = self.infer_expr(start, scope);
                let e = self.infer_expr(end, scope);
                if !matches!(s.strip_nullable(), Ty::Int | Ty::Unknown | Ty::Dynamic) {
                    self.err(format!("range start must be int, got {}", describe_ty(&s)));
                }
                if !matches!(e.strip_nullable(), Ty::Int | Ty::Unknown | Ty::Dynamic) {
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
                if let Expr::SelfAccess(task_name) = callee.as_ref() {
                    let Some(agent_name) = self.current_agent.clone() else {
                        self.err(format!("`self.{task_name}(...)` used outside an agent"));
                        return Ty::Unknown;
                    };
                    let Some(sig) = self
                        .agents
                        .get(&agent_name)
                        .and_then(|agent| agent.tasks.get(task_name))
                        .cloned()
                    else {
                        self.err(format!("agent `{agent_name}` has no task `{task_name}`"));
                        return Ty::Unknown;
                    };
                    let expected = sig.params.len();
                    let positional = args
                        .iter()
                        .filter(|arg| arg.name.is_none() && !arg.spread)
                        .count();
                    if !sig.variadic && positional > expected {
                        let param_names: Vec<&str> =
                            sig.params.iter().map(|(name, _)| name.as_str()).collect();
                        let hint = if param_names.is_empty() {
                            "task takes no arguments".to_string()
                        } else {
                            format!("expected: {}", param_names.join(", "))
                        };
                        self.err(format!(
                            "task `{agent_name}.{task_name}` takes {expected} argument(s), got {positional} — {hint}"
                        ));
                    }
                    self.check_call_args(
                        &sig.params,
                        sig.variadic,
                        args,
                        &arg_tys,
                        &format!("task `{agent_name}.{task_name}`"),
                    );
                    return sig.return_type;
                }
                if let Expr::Ident(name) = callee.as_ref()
                    && let Some(sig) = self.top_tasks.get(name).cloned()
                {
                    let expected = sig.params.len();
                    // Count only non-spread positional args (named args may map to params by name).
                    let positional: usize = args
                        .iter()
                        .filter(|a| a.name.is_none() && !a.spread)
                        .count();
                    if !sig.variadic && positional > expected {
                        let param_names: Vec<&str> =
                            sig.params.iter().map(|(n, _)| n.as_str()).collect();
                        let hint = if param_names.is_empty() {
                            "task takes no arguments".to_string()
                        } else {
                            format!("expected: {}", param_names.join(", "))
                        };
                        self.err(format!(
                            "task `{name}` takes {expected} argument(s), got {positional} — {hint}"
                        ));
                    }
                    // For generic tasks, infer type params from argument types,
                    // substitute into param types, then check each arg.
                    if let Some(td) = self.generic_task_decls.get(name).cloned() {
                        let mut type_env: HashMap<String, Ty> = HashMap::new();
                        for (param, arg_ty) in td.params.iter().zip(arg_tys.iter()) {
                            self.unify_type_params(
                                &param.ty,
                                arg_ty,
                                &td.type_params,
                                &mut type_env,
                            );
                        }
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
                                    self.resolve_type_with_env(&p.ty, &type_env),
                                )
                            })
                            .collect();
                        self.check_call_args(
                            &resolved_params,
                            td_variadic,
                            args,
                            &arg_tys,
                            &format!("task `{name}`"),
                        );
                        if let Some(ret_expr) = &td.return_type {
                            return self.resolve_type_with_env(ret_expr, &type_env);
                        }
                        return Ty::None_;
                    }
                    self.check_call_args(
                        &sig.params,
                        sig.variadic,
                        args,
                        &arg_tys,
                        &format!("task `{name}`"),
                    );
                    return sig.return_type.clone();
                }
                // Typed inference for prelude free functions.
                if let Expr::Ident(name) = callee.as_ref()
                    && name == "uuid"
                {
                    return Ty::Uuid;
                }
                if let Expr::Ident(name) = callee.as_ref()
                    && name == "typeof"
                {
                    return Ty::Str;
                }

                // Typed inference for prelude free functions min/max.
                if let Expr::Ident(name) = callee.as_ref()
                    && matches!(name.as_str(), "min" | "max")
                {
                    // Validate by: is a function if present.
                    if let Some(by_ty) = args
                        .iter()
                        .zip(arg_tys.iter())
                        .find(|(a, _)| a.name.as_deref() == Some("by"))
                        .map(|(_, ty)| ty)
                        && !matches!(by_ty, Ty::Func(..) | Ty::Unknown | Ty::Dynamic)
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
                                    Ty::List(inner) | Ty::Set(inner) => *inner.clone(),
                                    _ => ty.clone(),
                                }
                            } else {
                                ty.clone()
                            }
                        })
                        .collect();
                    let elem_ty = match positional_tys.as_slice() {
                        [] => Ty::Unknown,
                        [Ty::List(inner)] => *inner.clone(),
                        [single] => single.clone(),
                        slice if slice.iter().all(|t| self.types_match(t, &slice[0])) => {
                            slice[0].clone()
                        }
                        slice => {
                            let types: Vec<String> = slice.iter().map(describe_ty).collect();
                            self.err(format!(
                                "`{name}`: arguments must all have the same type, got {}",
                                types.join(", ")
                            ));
                            Ty::Unknown
                        }
                    };
                    return Ty::Nullable(Box::new(elem_ty));
                }
                let _ = self.infer_expr(callee, scope);
                Ty::Unknown
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
                if matches!(object.as_ref(), Expr::SelfRef) {
                    let Some(agent_name) = self.current_agent.clone() else {
                        self.err(format!("`self.{method}(...)` used outside an agent"));
                        return Ty::Unknown;
                    };
                    let Some(sig) = self
                        .agents
                        .get(&agent_name)
                        .and_then(|agent| agent.tasks.get(method))
                        .cloned()
                    else {
                        self.err(format!("agent `{agent_name}` has no task `{method}`"));
                        return Ty::Unknown;
                    };
                    let expected = sig.params.len();
                    let positional = args
                        .iter()
                        .filter(|arg| arg.name.is_none() && !arg.spread)
                        .count();
                    if !sig.variadic && positional > expected {
                        let param_names: Vec<&str> =
                            sig.params.iter().map(|(name, _)| name.as_str()).collect();
                        let hint = if param_names.is_empty() {
                            "task takes no arguments".to_string()
                        } else {
                            format!("expected: {}", param_names.join(", "))
                        };
                        self.err(format!(
                            "task `{agent_name}.{method}` takes {expected} argument(s), got {positional} — {hint}"
                        ));
                    }
                    self.check_call_args(
                        &sig.params,
                        sig.variadic,
                        args,
                        &arg_tys,
                        &format!("task `{agent_name}.{method}`"),
                    );
                    return sig.return_type;
                }
                // Special cases for inferring Ai.classify → Enum(T)
                if let Expr::Ident(name) = object.as_ref() {
                    if self.agents.contains_key(name) {
                        self.err(format!(
                            "direct agent task calls like `{name}.{method}(...)` are unsupported; use `self.{method}(...)` inside that agent or mailbox APIs such as `Agent.send(...)` / `Agent.delegate(...)`"
                        ));
                        return Ty::Unknown;
                    }
                    // Validate Agent.delegate call sites at compile time.
                    //
                    // Symbol form:  Agent.delegate(Foo.handle, data)
                    //   — arg[0] is FieldAccess(Ident("Foo"), "handle")
                    //   — arg[1] is the data; its type is checked against the handler param
                    //
                    // String form:  Agent.delegate(Foo, "handle", data)
                    //   — arg[0] is Ident("Foo")
                    //   — arg[1] is a plain string literal naming the handler
                    //   — arg[2] is the data; its type is checked against the handler param
                    if name == "Agent" && method == "delegate" {
                        if let Some(first_arg) = args.first() {
                            match &first_arg.value {
                                // Symbol form: Foo.handle
                                Expr::FieldAccess(obj_expr, handler_name) => {
                                    if let Expr::Ident(agent_name) = obj_expr.as_ref()
                                        && let Some(agent) = self.agents.get(agent_name)
                                    {
                                        // handler existence is already checked in FieldAccess
                                        if let Some(data_arg) = args.get(1) {
                                            if let Some(Some(param_ty)) =
                                                agent.handlers.get(handler_name).cloned()
                                            {
                                                self.expect(
                                                    &arg_tys[1],
                                                    &param_ty,
                                                    &format!(
                                                        "argument to `{agent_name}.{handler_name}`"
                                                    ),
                                                );
                                            }
                                            let _ = data_arg; // already inferred in arg_tys
                                        }
                                    }
                                }
                                // String form: Foo, "handle"
                                Expr::Ident(agent_name)
                                    if self.agents.contains_key(agent_name) =>
                                {
                                    let agent_name = agent_name.clone();
                                    if let Some(second_arg) = args.get(1) {
                                        if let Expr::StringLit(parts) = &second_arg.value {
                                            // Only check when the string is a plain literal
                                            // (no interpolation) so we can resolve it statically.
                                            let maybe_handler: Option<String> =
                                                parts.iter().try_fold(
                                                    String::new(),
                                                    |acc, p| match p {
                                                        crate::ast::StringPart::Literal(s) => {
                                                            Some(acc + s)
                                                        }
                                                        _ => None,
                                                    },
                                                );
                                            if let Some(handler_name) = maybe_handler {
                                                let agent = self.agents.get(&agent_name).unwrap();
                                                if !agent.handlers.contains_key(&handler_name) {
                                                    self.err(format!(
                                                        "agent `{agent_name}` has no handler \
                                                         `{handler_name}`; declared handlers: {}",
                                                        if agent.handlers.is_empty() {
                                                            "none".to_string()
                                                        } else {
                                                            agent.handlers
                                                                .keys()
                                                                .map(|s| s.as_str())
                                                                .collect::<Vec<_>>()
                                                                .join(", ")
                                                        }
                                                    ));
                                                } else if let Some(data_arg) = args.get(2) {
                                                    if let Some(Some(param_ty)) = agent
                                                        .handlers
                                                        .get(&handler_name)
                                                        .cloned()
                                                    {
                                                        self.expect(
                                                            &arg_tys[2],
                                                            &param_ty,
                                                            &format!(
                                                                "argument to \
                                                                 `{agent_name}.{handler_name}`"
                                                            ),
                                                        );
                                                    }
                                                    let _ = data_arg;
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        return Ty::None_;
                    }
                    // Resolve namespace method return types from the catalog.
                    // This replaces the former hand-maintained per-namespace
                    // match arms and ensures checker, LSP, and docs stay in sync.
                    if let Some(entry) = prelude::catalog_method(name, method.as_str()) {
                        return match entry.result {
                            BuiltinResult::Fixed(spec) => prelude::ty_from_spec(spec),
                            BuiltinResult::AiClassify => {
                                // Return `Nullable(Enum(as:))` when the `as:`
                                // argument names a known enum type; otherwise
                                // fall back to Unknown.
                                if let Some(as_arg) =
                                    args.iter().find(|a| a.name.as_deref() == Some("as"))
                                    && let Expr::Ident(enum_name) = &as_arg.value
                                    && self.enum_variants.contains_key(enum_name)
                                {
                                    Ty::Nullable(Box::new(Ty::Enum(enum_name.clone(), vec![])))
                                } else {
                                    Ty::Unknown
                                }
                            }
                            BuiltinResult::AiExtract => {
                                // Return `Nullable(resolve_type(as:))` when
                                // the `as:` argument names a resolvable type.
                                let inner = args
                                    .iter()
                                    .find(|a| a.name.as_deref() == Some("as"))
                                    .map(|a| {
                                        if let Expr::Ident(type_name) = &a.value {
                                            self.resolve_type(&TypeExpr::Named(type_name.clone()))
                                        } else {
                                            Ty::Unknown
                                        }
                                    })
                                    .unwrap_or(Ty::Unknown);
                                Ty::Nullable(Box::new(inner))
                            }
                            BuiltinResult::Unknown => Ty::Unknown,
                        };
                    }
                }
                let obj_ty = self.infer_expr(object, scope);
                match (obj_ty.strip_nullable(), method.as_str()) {
                    (Ty::List(elem), "push" | "filter" | "sort" | "reverse" | "take" | "skip") => {
                        Ty::List(elem.clone())
                    }
                    (Ty::List(_), "flatten") => Ty::List(Box::new(Ty::Unknown)),
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
                                Ty::Unknown
                            }
                            None => Ty::Unknown,
                        };
                        Ty::List(Box::new(Ty::Tuple(vec![*elem_a.clone(), elem_b])))
                    }
                    (Ty::List(_), "map") => Ty::List(Box::new(Ty::Unknown)),
                    (Ty::List(_), "reduce" | "sum" | "min" | "max") => Ty::Unknown,
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
                    (Ty::Int, "abs" | "floor" | "ceil" | "round") => Ty::Int,
                    (Ty::Float, "abs" | "floor" | "ceil" | "round") => Ty::Float,
                    (Ty::Datetime, "parts") => Ty::Unknown,
                    (Ty::Datetime, "format") => Ty::Nullable(Box::new(Ty::Str)),
                    (Ty::Uuid, "to_str" | "format") => Ty::Str,
                    (Ty::Uuid, "version") => Ty::Int,
                    (Ty::DbConnection, "query") => {
                        Ty::List(Box::new(Ty::Map(Box::new(Ty::Str), Box::new(Ty::Dynamic))))
                    }
                    (Ty::DbConnection, "exec") => Ty::Int,
                    _ => Ty::Unknown,
                }
            }

            Expr::Cast { expr, ty } => {
                self.infer_expr(expr, scope);
                self.resolve_type(ty)
            }

            Expr::IfExpr {
                cond,
                then_body,
                else_body,
            } => {
                let c = self.infer_expr(cond, scope);
                self.expect(&c, &Ty::Bool, "`if` condition");
                let then_ty = self.block_type(then_body, scope);
                let else_ty = self.block_type(else_body, scope);
                // When one branch exits via `return` its block_type is None_.
                // In that case propagate the other branch's type. When both
                // are concrete, verify they match.
                match (&then_ty, &else_ty) {
                    (Ty::None_, other)
                        if !matches!(other, Ty::None_ | Ty::Unknown | Ty::Dynamic) =>
                    {
                        other.clone()
                    }
                    (_, Ty::None_) => then_ty,
                    _ => {
                        if !matches!(then_ty, Ty::Unknown | Ty::Dynamic | Ty::None_)
                            && !matches!(else_ty, Ty::Unknown | Ty::Dynamic | Ty::None_)
                        {
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
                // Unify arm result types.
                let mut result_ty = Ty::None_;
                for arm in arms {
                    scope.push();
                    for p in &arm.patterns {
                        if let Pattern::Variant {
                            name: variant_name,
                            bindings,
                        } = p
                        {
                            for (idx, b) in bindings.iter().enumerate() {
                                if b == "_" {
                                    continue;
                                }
                                let field_ty =
                                    self.resolve_variant_field(&subject_ty, variant_name, b, idx);
                                scope.define(b.clone(), field_ty);
                            }
                        }
                    }
                    let arm_ty = self.block_type(&arm.body, scope);
                    scope.pop();
                    match (&result_ty, &arm_ty) {
                        (Ty::None_, _) => result_ty = arm_ty,
                        (_, Ty::None_ | Ty::Unknown | Ty::Dynamic) => {}
                        _ if matches!(result_ty, Ty::Unknown | Ty::Dynamic) => {}
                        _ => {
                            self.expect(
                                &arm_ty,
                                &result_ty,
                                "`when` expression arms must all have the same type",
                            );
                        }
                    }
                }
                result_ty
            }

            Expr::Lambda { params, body } => {
                scope.push();
                for p in params {
                    let ty =
                        p.ty.as_ref()
                            .map(|t| self.resolve_type(t))
                            .unwrap_or(Ty::Unknown);
                    scope.define(p.name.clone(), ty);
                }
                let ret = match body {
                    LambdaBody::Expr(e) => self.infer_expr(e, scope),
                    LambdaBody::Block(b) => {
                        let mut last = Ty::Unknown;
                        for (s, s_span) in b {
                            last = match s {
                                Stmt::Expr(e) => self.infer_expr(e, scope),
                                other => {
                                    self.check_stmt(other, s_span.clone(), scope);
                                    Ty::Unknown
                                }
                            };
                        }
                        last
                    }
                };
                scope.pop();
                Ty::Func(
                    params
                        .iter()
                        .map(|p| {
                            p.ty.as_ref()
                                .map(|t| self.resolve_type(t))
                                .unwrap_or(Ty::Unknown)
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
}
