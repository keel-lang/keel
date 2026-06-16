//! Type-expression resolution and type-parameter unification.
//!
//! Converts [`TypeExpr`] AST nodes into resolved [`Ty`] values, handling
//! named types, generics, aliases, structs, enums, and nullable/collection
//! wrappers.  Also provides type-parameter unification used by generic-task
//! call sites and variant-field lookup for exhaustiveness checking.

use std::collections::HashMap;

use crate::ast::*;
use crate::types::ty::{Ty, UnknownReason};

use super::Checker;

impl Checker<'_, '_> {
    /// Resolve a [`TypeExpr`] into a concrete [`Ty`] using the checker's
    /// current type environment.
    pub(crate) fn resolve_type(&self, ty: &TypeExpr) -> Ty {
        self.resolve_type_with_env(ty, &HashMap::new())
    }

    /// Resolve a type and emit a compile-time error if it is `map[K, V]` with
    /// an unsupported key type. Built-in hashable types are `str`, `int`, `bool`.
    /// `float` is excluded because NaN violates the Hash/Eq contract.
    pub(crate) fn resolve_and_check_type(&mut self, ty: &TypeExpr) -> Ty {
        self.check_type_visibility(ty);
        let resolved = self.resolve_type(ty);
        if let Ty::Map(key_ty, _) = &resolved {
            match key_ty.as_ref() {
                Ty::Str | Ty::Int | Ty::Bool => {}
                Ty::Float => self.err(
                    "float is not a valid map key type — NaN violates hash equality; use int instead",
                ),
                Ty::Nullable(_) => self.err("nullable types cannot be used as map keys"),
                Ty::Struct { .. } | Ty::Enum(_, _) => self.err(
                    "struct and enum keys are not yet supported as map keys \
                     — implement `interface Hashable` (coming in v0.2); \
                     use str, int, or bool",
                ),
                // Opaque types (Unknown, Dynamic, Error, Unresolved) pass through;
                // the original error, if any, was already reported.
                key if key.is_opaque() => {}
                _ => self.err("map key type must be str, int, or bool"),
            }
        }
        resolved
    }

    /// In module-graph mode, an annotation may only name types declared in
    /// the current module or imported with `use X from ...`. The checker's
    /// tables span the whole graph (the runtime is flat), so a name found in
    /// a table but absent from the visible set means a missing import.
    fn check_type_visibility(&mut self, ty: &TypeExpr) {
        let Some(visible) = self.visible_types.clone() else {
            return;
        };
        let mut names = Vec::new();
        collect_named_types(ty, &mut names);
        for name in names {
            let known = self.enum_variants.contains_key(&name)
                || self.structs.contains_key(&name)
                || self.aliases.contains_key(&name)
                || self.generic_decls.contains_key(&name)
                || self.interfaces.contains_key(&name);
            if known && !visible.contains(&name) {
                self.err(format!(
                    "type `{name}` is declared in another module — import it with \
                     `use {name} from \"./<module>.keel\"`"
                ));
            }
        }
    }

    /// Resolve a type expression, substituting any names found in `env` (type
    /// parameter bindings) before falling back to the normal resolution logic.
    pub(crate) fn resolve_type_with_env(&self, ty: &TypeExpr, env: &HashMap<String, Ty>) -> Ty {
        match ty {
            TypeExpr::Named(n) => {
                if let Some(bound) = env.get(n) {
                    return bound.clone();
                }
                match n.as_str() {
                    "int" => Ty::Int,
                    "float" => Ty::Float,
                    "str" => Ty::Str,
                    "bool" => Ty::Bool,
                    "none" => Ty::None_,
                    "datetime" => Ty::Datetime,
                    "duration" => Ty::Duration,
                    "Uuid" => Ty::Uuid,
                    _ => {
                        if self.enum_variants.contains_key(n) {
                            Ty::Enum(n.clone(), vec![])
                        } else if let Some(fields) = self.structs.get(n) {
                            Ty::Struct {
                                name: Some(n.clone()),
                                fields: fields.clone(),
                            }
                        } else if let Some(t) = self.aliases.get(n) {
                            t.clone()
                        } else {
                            // The name is unrecognised — return Unresolved so
                            // downstream checks suppress cascade errors silently.
                            // No new diagnostic is emitted here; the caller site
                            // may already have reported the problem.
                            Ty::Unresolved(n.clone())
                        }
                    }
                }
            }
            TypeExpr::Nullable(inner) => {
                Ty::Nullable(Box::new(self.resolve_type_with_env(inner, env)))
            }
            TypeExpr::List(inner) => Ty::List(Box::new(self.resolve_type_with_env(inner, env))),
            TypeExpr::Map(k, v) => Ty::Map(
                Box::new(self.resolve_type_with_env(k, env)),
                Box::new(self.resolve_type_with_env(v, env)),
            ),
            TypeExpr::Set(inner) => Ty::Set(Box::new(self.resolve_type_with_env(inner, env))),
            TypeExpr::Struct(fields) => Ty::Struct {
                name: None,
                fields: fields
                    .iter()
                    .map(|f| (f.name.clone(), self.resolve_type_with_env(&f.ty.kind, env)))
                    .collect(),
            },
            TypeExpr::Tuple(items) => Ty::Tuple(
                items
                    .iter()
                    .map(|t| self.resolve_type_with_env(t, env))
                    .collect(),
            ),
            TypeExpr::Func(params, ret) => Ty::Func(
                params
                    .iter()
                    .map(|t| self.resolve_type_with_env(t, env))
                    .collect(),
                Box::new(self.resolve_type_with_env(ret, env)),
            ),
            TypeExpr::Generic(name, args) => {
                // Resolve each type argument in the current env.
                let resolved_args: Vec<Ty> = args
                    .iter()
                    .map(|a| self.resolve_type_with_env(a, env))
                    .collect();
                // Look up the generic declaration and substitute.
                if let Some((type_params, type_def)) = self.generic_decls.get(name).cloned()
                    && type_params.len() == resolved_args.len()
                {
                    // Build substitution map — iterate by ref so resolved_args stays owned.
                    let inner_env: HashMap<String, Ty> = type_params
                        .iter()
                        .cloned()
                        .zip(resolved_args.iter().cloned())
                        .collect();
                    return match &type_def {
                        TypeDef::Struct(fields) => Ty::Struct {
                            name: Some(name.clone()),
                            fields: fields
                                .iter()
                                .map(|f| {
                                    (
                                        f.name.clone(),
                                        self.resolve_type_with_env(&f.ty.kind, &inner_env),
                                    )
                                })
                                .collect(),
                        },
                        TypeDef::Alias(ty_node) => {
                            self.resolve_type_with_env(&ty_node.kind, &inner_env)
                        }
                        // Carry type args so variant field types can be resolved in
                        // pattern-matching arms.
                        TypeDef::SimpleEnum(_) | TypeDef::RichEnum(_) => {
                            Ty::Enum(name.clone(), resolved_args)
                        }
                    };
                }
                // Generic declaration not found or type-arg count mismatch —
                // the construct is recognised but instantiation is not yet
                // implemented for this configuration.
                Ty::Unknown(UnknownReason::UnsupportedFeature(
                    "generic type instantiation",
                ))
            }
            TypeExpr::Dynamic => Ty::Dynamic,
            // SelfType is a synthetic receiver marker; it is replaced with the
            // concrete implementing type before any type-checked method body
            // runs, so it should never reach the resolver in practice.
            TypeExpr::SelfType => Ty::Dynamic,
        }
    }

    // -----------------------------------------------------------------
    // Generic type-parameter unification
    // -----------------------------------------------------------------

    /// Infer type-parameter bindings from a concrete argument type.
    ///
    /// Walks `param_expr` against `arg_ty`, populating `env` with
    /// name → concrete-type mappings for each name in `type_params`.
    /// Handles named params, nullable, list, set, and generic struct/enum
    /// applications. Falls back gracefully when the shape cannot be matched.
    pub(crate) fn unify_type_params(
        &self,
        param_expr: &TypeExpr,
        arg_ty: &Ty,
        type_params: &[String],
        env: &mut HashMap<String, Ty>,
    ) {
        match param_expr {
            TypeExpr::Named(n) if type_params.contains(n) => {
                env.entry(n.clone()).or_insert_with(|| arg_ty.clone());
            }
            TypeExpr::Nullable(inner) => {
                let inner_ty = match arg_ty {
                    Ty::Nullable(t) => (**t).clone(),
                    t => t.clone(),
                };
                self.unify_type_params(inner, &inner_ty, type_params, env);
            }
            TypeExpr::List(inner) => {
                if let Ty::List(t) = arg_ty {
                    self.unify_type_params(inner, t, type_params, env);
                }
            }
            TypeExpr::Set(inner) => {
                if let Ty::Set(t) = arg_ty {
                    self.unify_type_params(inner, t, type_params, env);
                }
            }
            TypeExpr::Generic(generic_name, args) => {
                match arg_ty {
                    // Generic enum: Ty::Enum already carries resolved type args.
                    Ty::Enum(enum_name, type_args) if generic_name == enum_name => {
                        for (a_expr, a_ty) in args.iter().zip(type_args.iter()) {
                            self.unify_type_params(a_expr, a_ty, type_params, env);
                        }
                    }
                    // Generic struct: rebuild positional type args by matching
                    // concrete field types against the generic definition's fields.
                    Ty::Struct {
                        fields: concrete_fields,
                        ..
                    } => {
                        if let Some((inner_params, TypeDef::Struct(gfields))) =
                            self.generic_decls.get(generic_name).cloned()
                        {
                            // Build the inner substitution from generic field type exprs.
                            let mut inner_env: HashMap<String, Ty> = HashMap::new();
                            for gfield in &gfields {
                                if let Some((_, concrete_ty)) =
                                    concrete_fields.iter().find(|(n, _)| *n == gfield.name)
                                {
                                    bind_type_params(
                                        &gfield.ty.kind,
                                        concrete_ty,
                                        &inner_params,
                                        &mut inner_env,
                                    );
                                }
                            }
                            // Unify each arg expr against its resolved concrete type.
                            for (i, a_expr) in args.iter().enumerate() {
                                if let Some(concrete_ty) =
                                    inner_params.get(i).and_then(|p| inner_env.get(p)).cloned()
                                {
                                    self.unify_type_params(a_expr, &concrete_ty, type_params, env);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Resolve the type of a single variant binding, given the subject enum
    /// type, the variant name, the binding name, and its positional index.
    ///
    /// For generic enums (`Ty::Enum(name, type_args)` where `type_args` is
    /// non-empty) the field type is looked up in `generic_decls` and the type
    /// arguments are substituted. For all other cases `Ty::Unknown` is returned
    /// so that existing behaviour is preserved.
    pub(crate) fn resolve_variant_field(
        &self,
        subject_ty: &Ty,
        variant_name: &str,
        binding: &str,
        _idx: usize,
    ) -> Ty {
        let Ty::Enum(enum_name, type_args) = subject_ty.strip_nullable() else {
            return Ty::Unknown(UnknownReason::InferenceLimitation);
        };
        if type_args.is_empty() {
            return Ty::Unknown(UnknownReason::InferenceLimitation);
        }
        let Some((type_params, type_def)) = self.generic_decls.get(enum_name) else {
            return Ty::Unknown(UnknownReason::InferenceLimitation);
        };
        let TypeDef::RichEnum(variants) = type_def else {
            return Ty::Unknown(UnknownReason::InferenceLimitation);
        };
        let Some(variant) = variants.iter().find(|v| v.name == variant_name) else {
            return Ty::Unknown(UnknownReason::InferenceLimitation);
        };
        let Some(fields) = &variant.fields else {
            return Ty::Unknown(UnknownReason::InferenceLimitation);
        };
        let Some(field) = fields.iter().find(|f| f.name == binding) else {
            return Ty::Unknown(UnknownReason::InferenceLimitation);
        };
        if type_params.len() != type_args.len() {
            return Ty::Unknown(UnknownReason::InferenceLimitation);
        }
        let env: HashMap<String, Ty> = type_params
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect();
        self.resolve_type_with_env(&field.ty.kind, &env)
    }

    /// Look up a named field's type from a struct subject type.
    /// Returns `Ty::Unknown(InferenceLimitation)` for opaque subjects or
    /// unknown fields rather than emitting an error — callers that want
    /// a hard diagnostic should do so themselves.
    pub(crate) fn resolve_struct_field(&self, subject_ty: &Ty, field: &str) -> Ty {
        match subject_ty.strip_nullable() {
            Ty::Struct { fields, .. } => fields
                .iter()
                .find(|(n, _)| n == field)
                .map(|(_, t)| t.clone())
                .unwrap_or(Ty::Unknown(UnknownReason::InferenceLimitation)),
            _ => Ty::Unknown(UnknownReason::InferenceLimitation),
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Collect every type name referenced by a type expression.
fn collect_named_types(ty: &TypeExpr, out: &mut Vec<String>) {
    match ty {
        TypeExpr::Named(n) => out.push(n.clone()),
        TypeExpr::Nullable(inner) | TypeExpr::List(inner) | TypeExpr::Set(inner) => {
            collect_named_types(inner, out)
        }
        TypeExpr::Map(k, v) => {
            collect_named_types(k, out);
            collect_named_types(v, out);
        }
        TypeExpr::Struct(fields) => {
            for f in fields {
                collect_named_types(&f.ty.kind, out);
            }
        }
        TypeExpr::Tuple(items) => {
            for item in items {
                collect_named_types(item, out);
            }
        }
        TypeExpr::Func(params, ret) => {
            for p in params {
                collect_named_types(p, out);
            }
            collect_named_types(ret, out);
        }
        TypeExpr::Generic(name, args) => {
            out.push(name.clone());
            for a in args {
                collect_named_types(a, out);
            }
        }
        TypeExpr::Dynamic | TypeExpr::SelfType => {}
    }
}

/// Bind type-parameter names from a `TypeExpr`/`Ty` pair into `env`.
///
/// Free-function counterpart to [`Checker::unify_type_params`] used for the
/// inner-generic-struct case where `&self` is not available.
fn bind_type_params(
    expr: &TypeExpr,
    ty: &Ty,
    type_params: &[String],
    env: &mut HashMap<String, Ty>,
) {
    match (expr, ty) {
        (TypeExpr::Named(n), _) if type_params.contains(n) => {
            env.entry(n.clone()).or_insert_with(|| ty.clone());
        }
        (TypeExpr::Nullable(inner), Ty::Nullable(t)) => {
            bind_type_params(inner, t, type_params, env)
        }
        (TypeExpr::List(inner), Ty::List(t)) => bind_type_params(inner, t, type_params, env),
        (TypeExpr::Set(inner), Ty::Set(t)) => bind_type_params(inner, t, type_params, env),
        _ => {}
    }
}
