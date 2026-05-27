//! Type checker for Keel v0.1.
//!
//! Pragmatic single-pass checker: declarations are collected up front,
//! then each task / agent handler / top-level statement is walked with
//! a stack of lexical scopes. Inference is deliberately shallow — when
//! a type can't be computed cheaply, it falls back to `Ty::Unknown`
//! and no error is reported. The goal is high-signal diagnostics
//! (undefined identifiers, non-exhaustive matches, `self` outside
//! agents, missing `else` on if-expressions, arg-count mismatches) not
//! full Hindley-Milner inference.

mod binop;
mod call;
mod collect;
mod expr;
mod resolve;
mod stmt;

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::lexer::Span;
use crate::types::interface::{self as iface, Signature};
// Re-export so existing call-sites (`crate::types::checker::Ty`, etc.) remain valid.
pub use crate::types::ty::{Ty, TypeError};
// Re-export IDE helpers so `lsp.rs` call-sites remain valid without churn.
pub use crate::ide::hover::type_at;
pub use crate::ide::symbols::{definition_of, ident_at_offset, ident_span_at_offset, usages_of};
use crate::types::prelude::{builtin_interfaces, prelude_names};
use crate::types::resolve::NameIndex;
use crate::types::scope::Scope;
use crate::types::ty::describe_ty;

// ---------------------------------------------------------------------------
// Per-task / per-handler info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TaskSig {
    params: Vec<(String, Ty)>,
    return_type: Ty,
    /// True if the last param is variadic (`...name: T`).
    variadic: bool,
}

#[derive(Debug, Clone)]
struct AgentInfo {
    state_fields: HashMap<String, Ty>,
    readonly_fields: HashSet<String>,
    /// Task signatures exposed through explicit `self.task(...)` calls.
    tasks: HashMap<String, TaskSig>,
    /// Event handlers declared with `on event(param: T)`.
    /// Value is `None` for parameterless handlers, `Some(ty)` when a typed
    /// parameter was declared. Used to validate `Agent.delegate` call sites.
    handlers: HashMap<String, Option<Ty>>,
}

// ---------------------------------------------------------------------------
// Checker state
// ---------------------------------------------------------------------------

pub(crate) struct Checker {
    errors: Vec<TypeError>,
    enum_variants: HashMap<String, Vec<String>>,
    structs: HashMap<String, Vec<(String, Ty)>>,
    aliases: HashMap<String, Ty>,
    /// Known interfaces: interface_name → required method signatures.
    /// Pre-seeded with built-ins (Stringable); extended by `interface` declarations.
    interfaces: HashMap<String, Vec<crate::ast::TaskSig>>,
    /// Type names that implement `Iterable` — used to allow `for x in value`
    /// on struct types.
    iterable_types: HashSet<String>,
    /// Generic type declarations stored as `name → (type_params, body)` for
    /// deferred instantiation when a concrete `Foo[str]` application appears.
    generic_decls: HashMap<String, (Vec<String>, TypeDef)>,
    /// Generic task declarations stored by name so call sites can infer
    /// type arguments from the concrete argument types.
    generic_task_decls: HashMap<String, TaskDecl>,
    top_tasks: HashMap<String, TaskSig>,
    agents: HashMap<String, AgentInfo>,
    current_agent: Option<String>,
    /// Declared return type of the task currently being checked.
    current_return_ty: Option<Ty>,
    /// Pre-seeded names that must not be reported as undefined
    /// (prelude namespaces, built-in types, symbol identifiers, etc.).
    prelude: HashSet<String>,
    /// Span of the statement currently being checked. Set at the top of
    /// `check_stmt` so every `err()` call within a statement — including
    /// errors raised by `infer_expr` — automatically gets a location.
    current_span: Option<Span>,
    /// When true, emit an error for any binding whose type the checker
    /// cannot resolve (falls back to `Ty::Unknown`).
    strict: bool,
    /// Global name index, built by [`name_resolve::build`] at the start of
    /// [`Checker::check_body`] after the declaration-collection pass has
    /// finished.  Consulted by `infer_expr` for every [`crate::ast::Expr::Ident`]
    /// that is not satisfied by the current lexical scope.
    name_index: NameIndex,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[must_use]
pub fn check(program: &Program) -> Vec<TypeError> {
    let mut c = Checker::new();
    c.collect(program);
    c.check_body(program);
    c.errors
}

/// Like `check`, but also emits errors for any binding whose type the
/// checker cannot resolve.  Use `keel check --strict` to surface gaps
/// in type coverage that the normal checker accepts silently.
#[must_use]
pub fn check_strict(program: &Program) -> Vec<TypeError> {
    let mut c = Checker::new();
    c.strict = true;
    c.collect(program);
    c.check_body(program);
    c.errors
}

impl Checker {
    pub(crate) fn new() -> Self {
        Checker {
            errors: Vec::new(),
            enum_variants: HashMap::new(),
            structs: HashMap::new(),
            aliases: HashMap::new(),
            interfaces: builtin_interfaces(),
            iterable_types: HashSet::new(),
            generic_decls: HashMap::new(),
            generic_task_decls: HashMap::new(),
            top_tasks: HashMap::new(),
            agents: HashMap::new(),
            current_agent: None,
            current_return_ty: None,
            prelude: prelude_names(),
            current_span: None,
            strict: false,
            name_index: NameIndex::default(),
        }
    }

    /// Emit an error, automatically attaching the current statement's span
    /// when one is available.
    fn err(&mut self, msg: impl Into<String>) {
        let mut e = TypeError::new(msg);
        if let Some(ref s) = self.current_span {
            e = e.at(s.clone());
        }
        self.errors.push(e);
    }

    #[expect(
        dead_code,
        reason = "kept for diagnostics that need explicit source spans"
    )]
    fn err_at(&mut self, msg: impl Into<String>, span: Span) {
        self.errors.push(TypeError::new(msg).at(span));
    }

    fn check_impl_conformance(&mut self, impl_decl: &ImplDecl) {
        let iface_name = &impl_decl.interface_name;
        let type_name = &impl_decl.type_name;

        let sigs = match self.interfaces.get(iface_name).cloned() {
            Some(s) => s,
            None => {
                self.err(format!(
                    "impl: unknown interface `{iface_name}` — declare it with `interface {iface_name} {{ ... }}`"
                ));
                return;
            }
        };

        let provided: HashSet<&str> = impl_decl.methods.iter().map(|m| m.name.as_str()).collect();

        for sig in &sigs {
            if !provided.contains(sig.name.as_str()) {
                self.err(format!(
                    "impl `{iface_name}` for `{type_name}` is missing required method `{}`",
                    sig.name
                ));
                continue;
            }
            let got_method = impl_decl
                .methods
                .iter()
                .find(|m| m.name == sig.name)
                .unwrap();

            // Arity check (exclude `self`).
            let req_arity = sig
                .params
                .iter()
                .filter(|p| !matches!(&p.name, Binding::Ident(n) if n == "self"))
                .count();
            let got_arity = got_method
                .params
                .iter()
                .filter(|p| !matches!(&p.name, Binding::Ident(n) if n == "self"))
                .count();
            if req_arity != got_arity {
                self.err(format!(
                    "impl `{iface_name}` for `{type_name}`: method `{}` expects {req_arity} parameter(s) but got {got_arity}",
                    sig.name
                ));
            }

            // Return-type check — use the shared typed conformance function so
            // that the checker and the runtime always apply identical rules.
            let env = self.type_env();
            let req_sig = Signature {
                params: vec![],
                ret: sig
                    .return_type
                    .as_ref()
                    .map(|te| iface::resolve_type_expr(te, &env))
                    .unwrap_or(Ty::None_),
            };
            let got_sig = Signature {
                params: vec![],
                ret: got_method
                    .return_type
                    .as_ref()
                    .map(|te| iface::resolve_type_expr(te, &env))
                    .unwrap_or(Ty::None_),
            };
            if !iface::signature_satisfies(&req_sig, &got_sig) {
                // Re-derive display strings for the human-readable error message.
                let req_str = sig
                    .return_type
                    .as_ref()
                    .map(type_display_str)
                    .unwrap_or_else(|| "none".to_string());
                let got_str = got_method
                    .return_type
                    .as_ref()
                    .map(type_display_str)
                    .unwrap_or_else(|| "none".to_string());
                self.err(format!(
                    "impl `{iface_name}` for `{type_name}`: method `{}` must return `{req_str}` but returns `{got_str}`",
                    sig.name
                ));
            }
        }

        // Reject extra methods not declared in the interface.
        for method in &impl_decl.methods {
            if !sigs.iter().any(|s| s.name == method.name) {
                self.err(format!(
                    "impl `{iface_name}` for `{type_name}`: method `{}` is not part of interface `{iface_name}`",
                    method.name
                ));
            }
        }
    }

    /// Build a [`crate::types::interface::TypeEnv`] from this checker's already-
    /// resolved alias table so that conformance checks share the same resolution
    /// context as the runtime.
    fn type_env(&self) -> iface::TypeEnv {
        iface::TypeEnv {
            aliases: self.aliases.clone(),
        }
    }

    /// Structural type equality (ignoring nullability wrapping differences).
    fn types_match(&self, a: &Ty, b: &Ty) -> bool {
        if a.is_opaque() || b.is_opaque() {
            return true;
        }
        match (a, b) {
            (Ty::Int, Ty::Int)
            | (Ty::Float, Ty::Float)
            | (Ty::Str, Ty::Str)
            | (Ty::Bool, Ty::Bool)
            | (Ty::None_, Ty::None_)
            | (Ty::Uuid, Ty::Uuid) => true,
            (Ty::List(a), Ty::List(b)) | (Ty::Set(a), Ty::Set(b)) => {
                self.types_match(a.as_ref(), b.as_ref())
            }
            (Ty::Nullable(a), Ty::Nullable(b)) => self.types_match(a.as_ref(), b.as_ref()),
            (Ty::Enum(a, _), Ty::Enum(b, _)) => a == b,
            (Ty::Struct(af), Ty::Struct(bf)) => {
                af.len() == bf.len()
                    && af
                        .iter()
                        .zip(bf.iter())
                        .all(|((an, at), (bn, bt))| an == bn && self.types_match(at, bt))
            }
            _ => false,
        }
    }

    fn block_type(&mut self, block: &Block, scope: &mut Scope) -> Ty {
        scope.push();
        let mut last = Ty::None_;
        for (stmt, span) in block {
            last = match stmt {
                Stmt::Expr(e) => self.infer_expr(e, scope),
                other => {
                    self.check_stmt(other, span.clone(), scope);
                    Ty::None_
                }
            };
        }
        scope.pop();
        last
    }

    fn expect(&mut self, actual: &Ty, expected: &Ty, context: &str) {
        if actual.is_opaque() {
            return;
        }
        if expected.is_opaque() {
            return;
        }

        // Nullable actual where non-nullable expected — caller must unwrap.
        if matches!(actual, Ty::Nullable(_)) && !matches!(expected, Ty::Nullable(_)) {
            self.err(format!(
                "{context}: expected {}, got {} — use `!` to assert non-null or `??` to provide a fallback",
                describe_ty(expected),
                describe_ty(actual),
            ));
            return;
        }

        let actual_base = actual.strip_nullable();
        let expected_base = expected.strip_nullable();

        // Struct structural compatibility: all expected fields must be present.
        if let (Ty::Struct(actual_fields), Ty::Struct(expected_fields)) =
            (actual_base, expected_base)
        {
            for (exp_name, exp_ty) in expected_fields {
                match actual_fields.iter().find(|(n, _)| n == exp_name) {
                    None => self.err(format!("{context}: missing field `{exp_name}`")),
                    Some((_, act_ty)) => {
                        self.expect(act_ty, exp_ty, &format!("{context}.{exp_name}"));
                    }
                }
            }
            return;
        }

        // Map literal coercion: a `{k: v, ...}` struct literal assigned to a
        // declared `map[K, V]` is treated as a map when keys are strings and
        // every field value matches V. This matches the surface syntax where
        // the same `{...}` form serves as both struct and map literal.
        if let (Ty::Struct(actual_fields), Ty::Map(key_ty, value_ty)) = (actual_base, expected_base)
            && (matches!(key_ty.as_ref(), Ty::Str) || key_ty.is_opaque())
        {
            for (name, act_ty) in actual_fields {
                self.expect(act_ty, value_ty, &format!("{context}[{name}]"));
            }
            return;
        }

        if actual_base != expected_base && !actual_base.is_opaque() {
            self.err(format!(
                "{context}: expected {}, got {}",
                describe_ty(expected),
                describe_ty(actual),
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Interface helpers
// ---------------------------------------------------------------------------

/// Produce a human-readable display string for a `TypeExpr` — used only for
/// error messages in `check_impl_conformance`.  This is intentionally separate
/// from the conformance logic: the typed comparison in
/// [`crate::types::interface::signature_satisfies`] is the source of truth;
/// this function only drives the "must return X but returns Y" message.
fn type_display_str(te: &TypeExpr) -> String {
    match te {
        TypeExpr::Named(n) => n.clone(),
        TypeExpr::Nullable(inner) => format!("{}?", type_display_str(inner)),
        TypeExpr::List(inner) => format!("list[{}]", type_display_str(inner)),
        TypeExpr::Map(k, v) => {
            format!("map[{}, {}]", type_display_str(k), type_display_str(v))
        }
        TypeExpr::Set(inner) => format!("set[{}]", type_display_str(inner)),
        TypeExpr::Tuple(items) => {
            let parts: Vec<_> = items.iter().map(type_display_str).collect();
            format!("({})", parts.join(", "))
        }
        TypeExpr::Func(params, ret) => {
            let ps: Vec<_> = params.iter().map(type_display_str).collect();
            format!("({}) -> {}", ps.join(", "), type_display_str(ret))
        }
        TypeExpr::Generic(name, args) => {
            let as_: Vec<_> = args.iter().map(type_display_str).collect();
            format!("{}[{}]", name, as_.join(", "))
        }
        TypeExpr::Struct(fields) => {
            let fs: Vec<_> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, type_display_str(&f.ty)))
                .collect();
            format!("{{{}}}", fs.join(", "))
        }
        TypeExpr::Dynamic => "dynamic".to_string(),
    }
}
