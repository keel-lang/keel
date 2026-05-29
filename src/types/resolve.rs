// Rust guideline compliant 2026-02-21
//! Name resolution pass — global identifier classification and undefined-name
//! diagnostics.
//!
//! Produces a [`NameIndex`] that classifies every top-level identifier in the
//! program so that expression type-inference consults a single authority
//! instead of performing ad-hoc string comparisons against multiple lookup
//! tables.  The companion [`resolve_names`] function walks the entire program
//! AST and emits "undefined" diagnostics for any identifier that is neither in
//! lexical scope nor present in the global name index.
//!
//! # Scope model
//!
//! The index covers only the **global namespace**: top-level tasks, agents,
//! declared types (enums, structs, aliases), and prelude names.  Lexical scope
//! (local `let` bindings, task parameters, for-loop variables) is tracked by a
//! scope stack inside [`NameResolver`] during the walk.
//!
//! # Build vs. per-expression resolution
//!
//! Because [`crate::ast::Expr`] carries no stable node ID, the index is keyed
//! by name string rather than by expression node.  Statement-level spans (from
//! the `Block = Vec<(Stmt, Span)>` representation) are used as location proxies
//! on undefined-name errors.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::ast::visit::{self, Visitor};
use crate::ast::*;
use crate::lexer::Span;
use crate::types::diagnostics::TypeDiagnostic;

// ---------------------------------------------------------------------------
// ResolvedName
// ---------------------------------------------------------------------------

/// The semantic category of a bare identifier in the global namespace.
///
/// Returned by [`NameIndex::resolve`].  Callers must verify the lexical scope
/// first — a local binding always shadows a global name with the same string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedName {
    /// A top-level task declared with `task <name>(...)`.
    TopTask,
    /// An agent declared with `agent <name> { ... }`.
    Agent,
    /// An enum type name.  Field access on this name resolves to a variant
    /// (e.g. `Urgency.high`).
    Enum,
    /// A struct definition or type alias — not a first-class value.
    TypeName,
    /// A prelude namespace (`Ai`, `Io`, `Env`, …) or a built-in free function
    /// (`uuid`, `typeof`, `min`, `max`) or a built-in type keyword (`int`,
    /// `str`, …).  The caller must use [`crate::types::prelude::catalog_method`]
    /// or a per-function match arm to compute the actual type.
    PreludeNamespace,
    /// Not found in any global namespace.  The caller is responsible for
    /// emitting an "undefined" diagnostic.
    Unresolved,
}

// ---------------------------------------------------------------------------
// NameIndex
// ---------------------------------------------------------------------------

/// Compiled index of global name declarations.
///
/// Built once by [`build`] after the declaration-collection pass completes and
/// stored on the checker for the duration of the body-checking pass.  All
/// [`crate::ast::Expr::Ident`] arms that are not satisfied by the lexical scope
/// delegate here.
pub struct NameIndex {
    map: HashMap<String, ResolvedName>,
}

impl Default for NameIndex {
    /// Returns an empty index (resolves every name to [`ResolvedName::Unresolved`]).
    fn default() -> Self {
        NameIndex {
            map: HashMap::new(),
        }
    }
}

impl NameIndex {
    /// Look up `name` in the global namespace.
    ///
    /// Returns [`ResolvedName::Unresolved`] for names not present in the
    /// index.  The caller must verify the lexical scope before calling this
    /// method — a local binding always shadows a global name.
    #[inline]
    pub fn resolve(&self, name: &str) -> &ResolvedName {
        self.map.get(name).unwrap_or(&ResolvedName::Unresolved)
    }
}

// ---------------------------------------------------------------------------
// build
// ---------------------------------------------------------------------------

/// Build a [`NameIndex`] from the checker's already-populated declaration tables.
///
/// Insertion order determines shadowing priority (last write wins):
/// `prelude` → `type_names` → `enum_names` → `agent_names` → `task_names`.
/// This mirrors the resolution priority in the original checker: a task named
/// `Foo` shadows a type named `Foo`, which shadows a prelude name `Foo`.
///
/// # Arguments
///
/// * `task_names`  — names of top-level `task` declarations
/// * `agent_names` — names of `agent` declarations
/// * `enum_names`  — names of enum `type` declarations
/// * `type_names`  — names of struct / alias `type` declarations
/// * `prelude`     — all built-in free identifiers and namespace names
///
/// This function is intentionally standalone (not a method on `Checker`) so
/// that it can be unit-tested without constructing a full checker.
pub fn build(
    task_names: impl IntoIterator<Item = impl Into<String>>,
    agent_names: impl IntoIterator<Item = impl Into<String>>,
    enum_names: impl IntoIterator<Item = impl Into<String>>,
    type_names: impl IntoIterator<Item = impl Into<String>>,
    prelude: impl IntoIterator<Item = impl Into<String>>,
) -> NameIndex {
    let mut map = HashMap::new();

    // Lower-priority entries first — later insertions shadow earlier ones.
    for name in prelude {
        map.insert(name.into(), ResolvedName::PreludeNamespace);
    }
    for name in type_names {
        map.insert(name.into(), ResolvedName::TypeName);
    }
    for name in enum_names {
        map.insert(name.into(), ResolvedName::Enum);
    }
    for name in agent_names {
        map.insert(name.into(), ResolvedName::Agent);
    }
    for name in task_names {
        map.insert(name.into(), ResolvedName::TopTask);
    }

    NameIndex { map }
}

// ---------------------------------------------------------------------------
// resolve_names — AST walk + undefined diagnostics
// ---------------------------------------------------------------------------

/// Walk every expression in `program`, using `index` for global-name lookup
/// and a per-scope stack for local bindings.  Returns any "undefined" errors
/// found; the `NameIndex` itself is returned unchanged (callers typically call
/// [`build`] first and then pass the result in).
///
/// This is the canonical source of undefined-identifier diagnostics.
/// `infer_expr`'s `Unresolved` arm simply returns `Ty::Error` without emitting
/// its own error — it relies on this pass having already done so.
pub fn resolve_names(program: &Program, index: &NameIndex) -> Vec<TypeDiagnostic> {
    let mut resolver = NameResolver {
        index,
        // Seed one frame for top-level let bindings so `bind()` always has a
        // live frame to write into.
        scopes: vec![HashSet::new()],
        errors: Vec::new(),
    };
    resolver.visit_program(program);
    resolver.errors
}

// ---------------------------------------------------------------------------
// NameResolver
// ---------------------------------------------------------------------------

struct NameResolver<'a> {
    index: &'a NameIndex,
    /// Lexical scope stack.  Each frame is the set of names bound in that scope.
    /// The bottom frame (index 0) holds top-level `let` bindings.
    scopes: Vec<HashSet<String>>,
    /// Accumulated undefined-name errors.
    errors: Vec<TypeDiagnostic>,
}

impl NameResolver<'_> {
    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Bind a plain identifier into the innermost scope.
    fn bind(&mut self, name: &str) {
        if let Some(frame) = self.scopes.last_mut() {
            frame.insert(name.to_owned());
        }
    }

    /// Bind all names introduced by a [`Binding`] (simple, struct-destruct, or
    /// tuple-destruct) into the innermost scope.
    fn bind_binding(&mut self, binding: &Binding) {
        match binding {
            Binding::Ident(name) => self.bind(name),
            Binding::Destruct(DestructPat::Struct(fields)) => {
                for (_, local) in fields {
                    self.bind(local);
                }
            }
            Binding::Destruct(DestructPat::Tuple(names)) => {
                for name in names {
                    self.bind(name);
                }
            }
        }
    }

    /// Returns `true` when `name` is bound in any lexical scope frame.
    fn is_bound(&self, name: &str) -> bool {
        self.scopes.iter().any(|frame| frame.contains(name))
    }

    /// Emit an "undefined" error for `name` at the identifier span.
    fn emit_undefined(&mut self, name: &str, span: Span) {
        self.errors.push(TypeDiagnostic::UndefinedName {
            name: name.to_owned(),
            span,
        });
    }

    /// Open a fresh scope for a task / impl-method body, bind all parameters,
    /// walk the body, then close the scope.
    fn visit_task_body(&mut self, t: &TaskDecl) {
        // Defaults are evaluated at the call site — walk them before any
        // parameter name enters scope.
        for p in &t.params {
            if let Some(default) = &p.default {
                self.visit_expr(default);
            }
        }
        self.push_scope();
        for p in &t.params {
            self.bind_binding(&p.name);
        }
        self.visit_block(&t.body);
        self.pop_scope();
    }

    /// Visit all arms of a `when` statement or expression, managing scope for
    /// `Pattern::Variant` field bindings.
    fn visit_when_arms(&mut self, arms: &[WhenArm]) {
        for arm in arms {
            self.push_scope();
            for p in &arm.patterns {
                // Only rich-variant destructures introduce local bindings.
                // Plain Pattern::Ident is an enum-variant reference, not a capture.
                if let Pattern::Variant { bindings, .. } = p {
                    for b in bindings {
                        if b != "_" {
                            self.bind(b);
                        }
                    }
                }
            }
            if let Some(g) = &arm.guard {
                self.visit_expr(g);
            }
            self.visit_block(&arm.body);
            self.pop_scope();
        }
    }
}

impl Visitor for NameResolver<'_> {
    // ------------------------------------------------------------------
    // Declarations — introduce param bindings before visiting bodies.
    // ------------------------------------------------------------------

    fn visit_decl(&mut self, decl: &Decl, span: &Span) {
        match decl {
            Decl::Task(t) => self.visit_task_body(t),
            // impl methods are TaskDecls that also require a fresh scope.
            Decl::Impl(impl_decl) => {
                for method in &impl_decl.methods {
                    self.visit_task_body(method);
                }
            }
            // Agent items handle their own scope inside visit_agent_item.
            // Delegate everything else (Type, Interface, Extern, Use, Stmt) to
            // the default walker.
            _ => visit::walk_decl(self, decl, span),
        }
    }

    fn visit_agent_item(&mut self, item: &AgentItem) {
        match item {
            AgentItem::Task(t) => self.visit_task_body(t),
            AgentItem::On(h) => {
                self.push_scope();
                if let Some(param) = &h.param {
                    self.bind_binding(&param.name);
                }
                self.visit_block(&h.body);
                self.pop_scope();
            }
            // State field defaults and attribute bodies have no special scope.
            _ => visit::walk_agent_item(self, item),
        }
    }

    // ------------------------------------------------------------------
    // Statements — track scope introductions and current span.
    // ------------------------------------------------------------------

    fn visit_stmt(&mut self, stmt: &Stmt, span: &Span) {
        match stmt {
            // let binding: value is evaluated before the name is in scope.
            Stmt::Let { binding, value, .. } => {
                self.visit_expr(value);
                self.bind_binding(binding);
            }
            // for loop: iter is outside the loop scope; binding and filter
            // are inside (the filter can reference the loop variable).
            Stmt::For {
                binding,
                iter,
                filter,
                body,
            } => {
                self.visit_expr(iter);
                self.push_scope();
                self.bind_binding(binding);
                if let Some(pred) = filter {
                    self.visit_expr(pred);
                }
                self.visit_block(body);
                self.pop_scope();
            }
            Stmt::When { subject, arms } => {
                self.visit_expr(subject);
                self.visit_when_arms(arms);
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.visit_expr(cond);
                self.push_scope();
                self.visit_block(then_body);
                self.pop_scope();
                if let Some(eb) = else_body {
                    self.push_scope();
                    self.visit_block(eb);
                    self.pop_scope();
                }
            }
            Stmt::While { cond, body } => {
                self.visit_expr(cond);
                self.push_scope();
                self.visit_block(body);
                self.pop_scope();
            }
            Stmt::TryCatch { body, catches } => {
                self.push_scope();
                self.visit_block(body);
                self.pop_scope();
                for catch in catches {
                    self.push_scope();
                    self.bind(&catch.name);
                    self.visit_block(&catch.body);
                    self.pop_scope();
                }
            }
            _ => visit::walk_stmt(self, stmt, span),
        }
    }

    // ------------------------------------------------------------------
    // Expressions — check identifiers; manage lambda / when-expr scope.
    // ------------------------------------------------------------------

    fn visit_expr(&mut self, spanned: &SpannedExpr) {
        let expr = &spanned.kind;
        match expr {
            Expr::Ident(name) => {
                if !self.is_bound(name)
                    && matches!(self.index.resolve(name), ResolvedName::Unresolved)
                {
                    self.emit_undefined(name, spanned.span.clone());
                }
            }
            Expr::Lambda { params, body } => {
                self.push_scope();
                for p in params {
                    self.bind(&p.name);
                }
                match body {
                    LambdaBody::Expr(e) => self.visit_expr(e),
                    LambdaBody::Block(b) => self.visit_block(b),
                }
                self.pop_scope();
            }
            Expr::WhenExpr { subject, arms } => {
                self.visit_expr(subject);
                self.visit_when_arms(arms);
            }
            _ => visit::walk_expr(self, spanned),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(
        tasks: &[&str],
        agents: &[&str],
        enums: &[&str],
        types: &[&str],
        prelude: &[&str],
    ) -> NameIndex {
        build(
            tasks.iter().copied(),
            agents.iter().copied(),
            enums.iter().copied(),
            types.iter().copied(),
            prelude.iter().copied(),
        )
    }

    #[test]
    fn resolves_top_task() {
        let ix = idx(&["greet"], &[], &[], &[], &[]);
        assert_eq!(ix.resolve("greet"), &ResolvedName::TopTask);
    }

    #[test]
    fn resolves_agent() {
        let ix = idx(&[], &["Bot"], &[], &[], &[]);
        assert_eq!(ix.resolve("Bot"), &ResolvedName::Agent);
    }

    #[test]
    fn resolves_enum() {
        let ix = idx(&[], &[], &["Urgency"], &[], &[]);
        assert_eq!(ix.resolve("Urgency"), &ResolvedName::Enum);
    }

    #[test]
    fn resolves_type_name() {
        let ix = idx(&[], &[], &[], &["User"], &[]);
        assert_eq!(ix.resolve("User"), &ResolvedName::TypeName);
    }

    #[test]
    fn resolves_prelude() {
        let ix = idx(&[], &[], &[], &[], &["Ai", "Io", "uuid"]);
        assert_eq!(ix.resolve("Ai"), &ResolvedName::PreludeNamespace);
        assert_eq!(ix.resolve("uuid"), &ResolvedName::PreludeNamespace);
    }

    #[test]
    fn unresolved_for_unknown_name() {
        let ix = idx(&[], &[], &[], &[], &[]);
        assert_eq!(ix.resolve("x"), &ResolvedName::Unresolved);
    }

    #[test]
    fn task_shadows_prelude() {
        // A user-declared task named `uuid` shadows the prelude built-in.
        let ix = idx(&["uuid"], &[], &[], &[], &["uuid"]);
        assert_eq!(ix.resolve("uuid"), &ResolvedName::TopTask);
    }

    #[test]
    fn enum_shadows_type_name() {
        // An enum declaration wins over a same-named struct/alias.
        let ix = idx(&[], &[], &["Status"], &["Status"], &[]);
        assert_eq!(ix.resolve("Status"), &ResolvedName::Enum);
    }

    #[test]
    fn default_is_all_unresolved() {
        let ix = NameIndex::default();
        assert_eq!(ix.resolve("anything"), &ResolvedName::Unresolved);
    }
}
