//! Shared interface-conformance logic used by both the type-checker and the
//! runtime.
//!
//! The two phases that verify `impl Interface for Type` declarations — the
//! static checker in [`crate::types::checker`] and the runtime registration
//! pass in [`crate::interpreter::decl`] — used to maintain separate string-
//! serialisation helpers that had already diverged (different `Map` formats,
//! different wildcard handling for `Struct`/`Generic` return types).  This
//! module replaces both with a single typed path:
//!
//! 1. Build a [`TypeEnv`] from the type declarations visible at each phase.
//! 2. Use [`resolve_type_expr`] to lower a [`TypeExpr`] to a [`Ty`] value.
//! 3. Call [`signature_satisfies`] to decide conformance.
//!
//! # Covariance rules
//!
//! | Required type          | Accepted actual type       |
//! |------------------------|---------------------------|
//! | `Ty::Dynamic`          | anything                  |
//! | `Ty::List(Ty::Dynamic)`| any `Ty::List(_)`         |
//! | everything else        | exact `PartialEq` match   |
//!
//! The old `"unknown"` wildcard (checker-only) is intentionally dropped:
//! `Struct` and `Generic` return types now require an exact structural match,
//! making `keel check` as strict as `keel run` for these cases.

use std::collections::HashMap;

use crate::ast::{TypeDef, TypeExpr};

use super::checker::Ty;

// ---------------------------------------------------------------------------
// TypeEnv
// ---------------------------------------------------------------------------

/// Minimal type-resolution context for interface conformance checks.
///
/// Only alias information is strictly required: primitive types are resolved
/// via the fixed mapping in [`resolve_type_expr`], and all other named types
/// (enums, named structs, unrecognised generics) are represented as nominal
/// [`Ty::Enum`] values whose identity comes from the name + resolved type
/// arguments, not from their structural definition.
///
/// Both the checker and the runtime build their own `TypeEnv` from the type
/// declarations they have already processed; the two envs do not need to be
/// identical because conformance is checked independently in each phase.
#[derive(Debug, Default)]
pub struct TypeEnv {
    /// Resolved alias types: `"Timestamp" → Ty::Datetime`, etc.
    pub aliases: HashMap<String, Ty>,
}

impl TypeEnv {
    /// Create an empty environment (no aliases known).
    pub fn new() -> Self {
        Self::default()
    }

    /// Populate `aliases` by scanning every `type Name = <expr>` declaration
    /// in `declarations`.
    ///
    /// A single forward pass is used; aliases that reference other aliases
    /// declared later in the file will fall back to their nominal
    /// `Ty::Enum(name, [])` representation rather than being fully resolved.
    /// Circular aliases produce a nominal type and do not recurse infinitely.
    pub fn collect_aliases<'a>(
        &mut self,
        declarations: impl IntoIterator<Item = &'a crate::ast::Decl>,
    ) {
        for decl in declarations {
            if let crate::ast::Decl::Type(t) = decl
                && let TypeDef::Alias(te_node) = &t.def
            {
                let resolved = resolve_type_expr(&te_node.kind, self);
                self.aliases.insert(t.name.clone(), resolved);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TypeExpr → Ty resolution
// ---------------------------------------------------------------------------

/// Lower a [`TypeExpr`] to a [`Ty`] using `env` for alias look-ups.
///
/// Named types that are not primitive keywords and not found in `env.aliases`
/// are represented as `Ty::Enum(name, [])` — a nominal carrier that preserves
/// the type name for identity comparison without requiring a full structural
/// expansion.  Generic applications (`TypeExpr::Generic`) similarly resolve to
/// `Ty::Enum(name, resolved_args)` so that `Result[str, int]` and
/// `Result[bool, str]` produce distinct `Ty` values and are correctly rejected
/// by conformance checks.
pub fn resolve_type_expr(te: &TypeExpr, env: &TypeEnv) -> Ty {
    match te {
        TypeExpr::Named(n) => resolve_named(n, env),
        TypeExpr::Nullable(inner) => Ty::Nullable(Box::new(resolve_type_expr(inner, env))),
        TypeExpr::List(inner) => Ty::List(Box::new(resolve_type_expr(inner, env))),
        TypeExpr::Map(k, v) => Ty::Map(
            Box::new(resolve_type_expr(k, env)),
            Box::new(resolve_type_expr(v, env)),
        ),
        TypeExpr::Set(inner) => Ty::Set(Box::new(resolve_type_expr(inner, env))),
        TypeExpr::Struct(fields) => Ty::Struct {
            name: None,
            fields: fields
                .iter()
                .map(|f| (f.name.clone(), resolve_type_expr(&f.ty.kind, env)))
                .collect(),
        },
        TypeExpr::Tuple(items) => {
            Ty::Tuple(items.iter().map(|t| resolve_type_expr(t, env)).collect())
        }
        TypeExpr::Func(params, ret) => Ty::Func(
            params.iter().map(|t| resolve_type_expr(t, env)).collect(),
            Box::new(resolve_type_expr(ret, env)),
        ),
        TypeExpr::Generic(name, args) => {
            // Resolve type arguments but keep the application nominal — there
            // is no generic-declaration context here. The name + resolved args
            // together form a unique identity for comparison purposes.
            let resolved_args: Vec<Ty> = args.iter().map(|a| resolve_type_expr(a, env)).collect();
            Ty::Enum(name.clone(), resolved_args)
        }
        TypeExpr::Dynamic => Ty::Dynamic,
        // SelfType is a synthetic receiver marker; conformance checking
        // filters self params by binding name before reaching this function.
        TypeExpr::SelfType => Ty::Dynamic,
    }
}

/// Resolve a named type string to its [`Ty`] representation.
fn resolve_named(name: &str, env: &TypeEnv) -> Ty {
    match name {
        "int" => Ty::Int,
        "float" => Ty::Float,
        "str" => Ty::Str,
        "bool" => Ty::Bool,
        "none" => Ty::None_,
        "datetime" => Ty::Datetime,
        "duration" => Ty::Duration,
        "Uuid" => Ty::Uuid,
        _ => {
            if let Some(aliased) = env.aliases.get(name) {
                aliased.clone()
            } else {
                // Enum, named struct, or unrecognised identifier — use a
                // nominal representation so different names stay distinct.
                Ty::Enum(name.to_owned(), vec![])
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Signature and conformance check
// ---------------------------------------------------------------------------

/// The resolved parameter and return types for a single interface method.
#[derive(Debug, Clone)]
pub struct Signature {
    /// Resolved types of all parameters **excluding** `self`.
    pub params: Vec<Ty>,
    /// Resolved return type (`Ty::None_` when the method has no explicit
    /// return type annotation).
    pub ret: Ty,
}

/// Returns `true` if `actual` satisfies the `required` interface signature.
///
/// The comparison applies the covariance rules described in the module-level
/// documentation: `Dynamic` in the required position is a wildcard; otherwise
/// structural `PartialEq` is used.
///
pub fn signature_satisfies(required: &Signature, actual: &Signature) -> bool {
    if required.params.len() != actual.params.len() {
        return false;
    }
    for (req_ty, act_ty) in required.params.iter().zip(&actual.params) {
        if !ty_satisfies(req_ty, act_ty) {
            return false;
        }
    }
    ty_satisfies(&required.ret, &actual.ret)
}

/// Returns `true` if `actual` satisfies the `required` type position.
fn ty_satisfies(required: &Ty, actual: &Ty) -> bool {
    match required {
        // Explicit wildcard: `dynamic` in the required position accepts anything.
        Ty::Dynamic => true,
        // `list[dynamic]` in the required position accepts any list type.
        Ty::List(inner) if matches!(inner.as_ref(), Ty::Dynamic) => {
            matches!(actual, Ty::List(_))
        }
        _ => required == actual,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Field, Node, TypeExpr};

    fn env() -> TypeEnv {
        TypeEnv::new()
    }

    fn te_named(name: &str) -> TypeExpr {
        TypeExpr::Named(name.to_owned())
    }

    /// Synthetic `Node<TypeExpr>` for use in `Field.ty` and other `Node<TypeExpr>` positions.
    fn te_spanned(name: &str) -> Node<TypeExpr> {
        Node::synthetic(TypeExpr::Named(name.to_owned()))
    }

    fn resolve(te: &TypeExpr) -> Ty {
        resolve_type_expr(te, &env())
    }

    // ── resolve_type_expr primitives ─────────────────────────────────────

    #[test]
    fn resolve_primitives() {
        assert_eq!(resolve(&te_named("str")), Ty::Str);
        assert_eq!(resolve(&te_named("int")), Ty::Int);
        assert_eq!(resolve(&te_named("float")), Ty::Float);
        assert_eq!(resolve(&te_named("bool")), Ty::Bool);
        assert_eq!(resolve(&te_named("none")), Ty::None_);
        assert_eq!(resolve(&te_named("datetime")), Ty::Datetime);
        assert_eq!(resolve(&te_named("duration")), Ty::Duration);
        assert_eq!(resolve(&te_named("Uuid")), Ty::Uuid);
        assert_eq!(resolve(&TypeExpr::Dynamic), Ty::Dynamic);
    }

    #[test]
    fn resolve_unknown_named_is_nominal() {
        // Unrecognised names are nominal Enum carriers, not Unknown.
        assert_eq!(
            resolve(&te_named("Urgency")),
            Ty::Enum("Urgency".to_owned(), vec![])
        );
    }

    #[test]
    fn resolve_generic_carries_name_and_args() {
        let te = TypeExpr::Generic("Result".to_owned(), vec![te_named("str"), te_named("int")]);
        assert_eq!(
            resolve(&te),
            Ty::Enum("Result".to_owned(), vec![Ty::Str, Ty::Int])
        );
    }

    #[test]
    fn resolve_generic_distinct_args_produce_distinct_ty() {
        let te_a = TypeExpr::Generic("Result".to_owned(), vec![te_named("str"), te_named("int")]);
        let te_b = TypeExpr::Generic("Result".to_owned(), vec![te_named("bool"), te_named("str")]);
        assert_ne!(resolve(&te_a), resolve(&te_b));
    }

    #[test]
    fn resolve_alias() {
        let mut e = TypeEnv::new();
        e.aliases.insert("Timestamp".to_owned(), Ty::Datetime);
        assert_eq!(resolve_type_expr(&te_named("Timestamp"), &e), Ty::Datetime);
    }

    #[test]
    fn resolve_inline_struct_fields() {
        let te = TypeExpr::Struct(vec![
            Field {
                name: "body".to_owned(),
                ty: te_spanned("str"),
            },
            Field {
                name: "count".to_owned(),
                ty: te_spanned("int"),
            },
        ]);
        assert_eq!(
            resolve(&te),
            Ty::Struct {
                name: None,
                fields: vec![("body".to_owned(), Ty::Str), ("count".to_owned(), Ty::Int),],
            }
        );
    }

    // ── signature_satisfies ──────────────────────────────────────────────

    fn sig(ret: Ty) -> Signature {
        Signature {
            params: vec![],
            ret,
        }
    }

    #[test]
    fn exact_match_passes() {
        assert!(signature_satisfies(&sig(Ty::Str), &sig(Ty::Str)));
    }

    #[test]
    fn mismatch_fails() {
        assert!(!signature_satisfies(&sig(Ty::Str), &sig(Ty::Int)));
    }

    #[test]
    fn dynamic_required_accepts_anything() {
        assert!(signature_satisfies(&sig(Ty::Dynamic), &sig(Ty::Str)));
        assert!(signature_satisfies(&sig(Ty::Dynamic), &sig(Ty::Int)));
        assert!(signature_satisfies(
            &sig(Ty::Dynamic),
            &sig(Ty::List(Box::new(Ty::Str)))
        ));
    }

    #[test]
    fn list_dynamic_required_accepts_any_list() {
        let req = sig(Ty::List(Box::new(Ty::Dynamic)));
        assert!(signature_satisfies(&req, &sig(Ty::List(Box::new(Ty::Str)))));
        assert!(signature_satisfies(&req, &sig(Ty::List(Box::new(Ty::Int)))));
        assert!(!signature_satisfies(&req, &sig(Ty::Str)));
    }

    #[test]
    fn generic_mismatch_fails() {
        // Result[str, int] vs Result[bool, str] — the bug that existed before
        // this module was introduced.
        let req = sig(Ty::Enum("Result".to_owned(), vec![Ty::Str, Ty::Int]));
        let got = sig(Ty::Enum("Result".to_owned(), vec![Ty::Bool, Ty::Str]));
        assert!(!signature_satisfies(&req, &got));
    }

    #[test]
    fn generic_exact_match_passes() {
        let ty = Ty::Enum("Result".to_owned(), vec![Ty::Str, Ty::Int]);
        assert!(signature_satisfies(&sig(ty.clone()), &sig(ty)));
    }

    #[test]
    fn param_count_mismatch_fails() {
        let req = Signature {
            params: vec![Ty::Str],
            ret: Ty::Str,
        };
        let got = Signature {
            params: vec![],
            ret: Ty::Str,
        };
        assert!(!signature_satisfies(&req, &got));
    }

    #[test]
    fn signature_satisfies_matching_return_types() {
        let env = TypeEnv::new();
        let required = Signature {
            params: vec![],
            ret: resolve_type_expr(&TypeExpr::Named("str".into()), &env),
        };
        let actual = Signature {
            params: vec![],
            ret: resolve_type_expr(&TypeExpr::Named("str".into()), &env),
        };
        assert!(signature_satisfies(&required, &actual));
    }
}
