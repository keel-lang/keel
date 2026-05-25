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

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::lexer::Span;

// ---------------------------------------------------------------------------
// Error shape
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct TypeError {
    pub message: String,
    pub span: Option<Span>,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl TypeError {
    fn new(msg: impl Into<String>) -> Self {
        TypeError {
            message: msg.into(),
            span: None,
        }
    }
    fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

// ---------------------------------------------------------------------------
// Types (resolved, not AST-level)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Int,
    Float,
    Str,
    Bool,
    None_,
    Duration,
    Datetime,
    Uuid,
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Set(Box<Ty>),
    Struct(Vec<(String, Ty)>),
    Tuple(Vec<Ty>),
    Func(Vec<Ty>, Box<Ty>),
    /// Enum type. The second field carries the resolved type arguments for
    /// generic enums (e.g. `Pair[str, int]` → `Enum("Pair", [Str, Int])`).
    /// For non-generic enums the vec is empty.
    Enum(String, Vec<Ty>),
    /// Unresolved or unsupported — skip further checks.
    Unknown,
    Nullable(Box<Ty>),
    Dynamic,
}

impl Ty {
    fn strip_nullable(&self) -> &Ty {
        match self {
            Ty::Nullable(inner) => inner,
            _ => self,
        }
    }
}

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
    #[expect(
        dead_code,
        reason = "collected for planned cross-agent event handler validation"
    )]
    handlers: HashSet<String>,
}

// ---------------------------------------------------------------------------
// Checker state
// ---------------------------------------------------------------------------

struct Checker {
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
}

/// Chained lexical scope: newer scopes on the back of the vec.
struct Scope {
    frames: Vec<HashMap<String, Ty>>,
}

impl Scope {
    fn new() -> Self {
        Scope {
            frames: vec![HashMap::new()],
        }
    }
    fn push(&mut self) {
        self.frames.push(HashMap::new());
    }
    fn pop(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }
    fn define(&mut self, name: String, ty: Ty) {
        if let Some(f) = self.frames.last_mut() {
            f.insert(name, ty);
        }
    }
    fn get(&self, name: &str) -> Option<&Ty> {
        for f in self.frames.iter().rev() {
            if let Some(t) = f.get(name) {
                return Some(t);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[must_use]
pub fn check(program: &Program) -> Vec<TypeError> {
    let mut c = Checker::new();
    c.collect(program);
    c.check(program);
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
    c.check(program);
    c.errors
}

impl Checker {
    fn new() -> Self {
        let mut prelude = HashSet::new();
        // Prelude namespaces
        for n in [
            "Ai", "Io", "Http", "Email", "Search", "Db", "Memory", "Schedule", "Async", "Control",
            "Env", "Time", "Log", "Agent", "Cache", "File", "Json", "Random", "Uuid", "Crypto", "Math", "Shell",
        ] {
            prelude.insert(n.to_string());
        }
        // Top-level builtins
        for n in ["run", "stop", "min", "max", "uuid", "typeof"] {
            prelude.insert(n.to_string());
        }
        // Built-in type names
        for n in [
            "int",
            "float",
            "str",
            "bool",
            "none",
            "datetime",
            "duration",
            "Uuid",
            "dynamic",
            "list",
            "map",
            "set",
            "Result",
            "Message",
            "SearchResult",
            "Memory",
            "HttpResponse",
            "Decision",
            "Error",
            "AIError",
            "NetworkError",
            "TimeoutError",
            "NullError",
            "TypeError",
            "ParseError",
        ] {
            prelude.insert(n.to_string());
        }
        // Symbol identifiers used as hint args (see runtime::SYMBOL_IDENTS)
        // and attribute-value keywords (`@memory persistent`, etc.).
        for n in [
            "sentence",
            "sentences",
            "line",
            "lines",
            "word",
            "words",
            "paragraph",
            "paragraphs",
            "bullets",
            "prose",
            "json",
            "exponential",
            "linear",
            "fixed",
            "google",
            "bing",
            "arxiv",
            "text",
            "html",
            "markdown",
            "persistent",
            "session",
        ] {
            prelude.insert(n.to_string());
        }

        // Built-in interface names (Stringable, Comparable, …) are not keywords —
        // they're identifiers resolved at runtime.  Adding them to the prelude
        // prevents spurious "undefined identifier" errors when the checker
        // encounters `impl Stringable for Foo` before seeing any declaration of
        // `Stringable` in the source file.
        for iface in [
            "Stringable",
            "Comparable",
            "Equatable",
            "Serializable",
            "Iterable",
        ] {
            prelude.insert(iface.to_string());
        }

        Checker {
            errors: Vec::new(),
            enum_variants: HashMap::new(),
            structs: HashMap::new(),
            aliases: HashMap::new(),
            interfaces: checker_builtin_interfaces(),
            iterable_types: HashSet::new(),
            generic_decls: HashMap::new(),
            generic_task_decls: HashMap::new(),
            top_tasks: HashMap::new(),
            agents: HashMap::new(),
            current_agent: None,
            current_return_ty: None,
            prelude,
            current_span: None,
            strict: false,
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

    // -----------------------------------------------------------------
    // Collection pass
    // -----------------------------------------------------------------

    fn collect(&mut self, program: &Program) {
        // First pass: register all interface declarations so impl blocks can
        // reference them regardless of source order.
        const BUILTIN_IFACES: &[&str] = &[
            "Stringable",
            "Comparable",
            "Equatable",
            "Serializable",
            "Iterable",
        ];
        for (decl, _) in &program.declarations {
            if let Decl::Interface(iface) = decl {
                if BUILTIN_IFACES.contains(&iface.name.as_str()) {
                    self.err(format!(
                        "`{}` is a built-in interface and cannot be redeclared",
                        iface.name
                    ));
                    continue;
                }
                self.interfaces
                    .insert(iface.name.clone(), iface.methods.clone());
            }
        }

        for (decl, _) in &program.declarations {
            match decl {
                Decl::Type(t) => self.collect_type_decl(t),
                Decl::Task(t) => {
                    if !t.type_params.is_empty() {
                        self.generic_task_decls.insert(t.name.clone(), t.clone());
                    }
                    let sig = self.task_sig(t);
                    self.top_tasks.insert(t.name.clone(), sig);
                }
                Decl::Agent(a) => {
                    let info = self.agent_info(a);
                    self.agents.insert(a.name.clone(), info);
                }
                Decl::Impl(impl_decl) => {
                    self.check_impl_conformance(impl_decl);
                    if impl_decl.interface_name == "Iterable" {
                        self.iterable_types.insert(impl_decl.type_name.clone());
                    }
                }
                _ => {}
            }
        }
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

            // Return-type check.
            let req_ret = sig
                .return_type
                .as_ref()
                .map(type_expr_str)
                .unwrap_or_else(|| "none".to_string());
            let got_ret = got_method
                .return_type
                .as_ref()
                .map(type_expr_str)
                .unwrap_or_else(|| "none".to_string());
            if !checker_return_types_match(&req_ret, &got_ret) {
                self.err(format!(
                    "impl `{iface_name}` for `{type_name}`: method `{}` must return `{req_ret}` but returns `{got_ret}`",
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

    fn collect_type_decl(&mut self, t: &TypeDecl) {
        if !t.type_params.is_empty() {
            // Generic type — defer body resolution until instantiation.
            // For enum types, still register variant names for exhaustiveness checking.
            match &t.def {
                TypeDef::SimpleEnum(vs) => {
                    self.enum_variants.insert(t.name.clone(), vs.clone());
                }
                TypeDef::RichEnum(vs) => {
                    self.enum_variants
                        .insert(t.name.clone(), vs.iter().map(|v| v.name.clone()).collect());
                }
                _ => {}
            }
            self.generic_decls
                .insert(t.name.clone(), (t.type_params.clone(), t.def.clone()));
            return;
        }
        match &t.def {
            TypeDef::SimpleEnum(vs) => {
                self.enum_variants.insert(t.name.clone(), vs.clone());
            }
            TypeDef::RichEnum(vs) => {
                self.enum_variants
                    .insert(t.name.clone(), vs.iter().map(|v| v.name.clone()).collect());
            }
            TypeDef::Struct(fields) => {
                let f: Vec<_> = fields
                    .iter()
                    .map(|f| (f.name.clone(), self.resolve_type(&f.ty)))
                    .collect();
                self.structs.insert(t.name.clone(), f);
            }
            TypeDef::Alias(ty) => {
                let resolved = self.resolve_type(ty);
                self.aliases.insert(t.name.clone(), resolved);
            }
        }
    }

    fn task_sig(&self, t: &TaskDecl) -> TaskSig {
        let variadic = t.params.last().is_some_and(|p| p.variadic);
        let params = t
            .params
            .iter()
            .map(|p| {
                let name = match &p.name {
                    Binding::Ident(n) => n.clone(),
                    Binding::Destruct(_) => "_".to_string(),
                };
                // Variadic params are `list[T]` inside the body but `T` at call sites.
                // The sig stores the element type so call-site checks compare each arg to T.
                (name, self.resolve_type(&p.ty))
            })
            .collect();
        let return_type = t
            .return_type
            .as_ref()
            .map(|ty| self.resolve_type(ty))
            .unwrap_or(Ty::None_);
        TaskSig {
            params,
            return_type,
            variadic,
        }
    }

    fn agent_info(&self, a: &AgentDecl) -> AgentInfo {
        let mut state_fields = HashMap::new();
        let mut readonly_fields = HashSet::new();
        let mut tasks = HashMap::new();
        let mut handlers = HashSet::new();
        for item in &a.items {
            match item {
                AgentItem::State(fields) => {
                    for f in fields {
                        state_fields.insert(f.name.clone(), self.resolve_type(&f.ty));
                        if f.readonly {
                            readonly_fields.insert(f.name.clone());
                        }
                    }
                }
                AgentItem::Task(t) => {
                    tasks.insert(t.name.clone(), self.task_sig(t));
                }
                AgentItem::On(h) => {
                    handlers.insert(h.event.clone());
                }
                AgentItem::Attribute(_) => {}
            }
        }
        AgentInfo {
            state_fields,
            readonly_fields,
            tasks,
            handlers,
        }
    }

    // -----------------------------------------------------------------
    // AST type → resolved Ty
    // -----------------------------------------------------------------

    fn resolve_type(&self, ty: &TypeExpr) -> Ty {
        self.resolve_type_with_env(ty, &HashMap::new())
    }

    /// Resolve a type expression, substituting any names found in `env` (type
    /// parameter bindings) before falling back to the normal resolution logic.
    fn resolve_type_with_env(&self, ty: &TypeExpr, env: &HashMap<String, Ty>) -> Ty {
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
                            Ty::Struct(fields.clone())
                        } else if let Some(t) = self.aliases.get(n) {
                            t.clone()
                        } else {
                            Ty::Unknown
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
            TypeExpr::Struct(fields) => Ty::Struct(
                fields
                    .iter()
                    .map(|f| (f.name.clone(), self.resolve_type_with_env(&f.ty, env)))
                    .collect(),
            ),
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
                        TypeDef::Struct(fields) => Ty::Struct(
                            fields
                                .iter()
                                .map(|f| {
                                    (
                                        f.name.clone(),
                                        self.resolve_type_with_env(&f.ty, &inner_env),
                                    )
                                })
                                .collect(),
                        ),
                        TypeDef::Alias(ty) => self.resolve_type_with_env(ty, &inner_env),
                        // Carry type args so variant field types can be resolved in
                        // pattern-matching arms.
                        TypeDef::SimpleEnum(_) | TypeDef::RichEnum(_) => {
                            Ty::Enum(name.clone(), resolved_args)
                        }
                    };
                }
                Ty::Unknown
            }
            TypeExpr::Dynamic => Ty::Dynamic,
        }
    }

    // -----------------------------------------------------------------
    // Validation pass
    // -----------------------------------------------------------------

    fn check(&mut self, program: &Program) {
        for (decl, _) in &program.declarations {
            match decl {
                Decl::Task(t) => {
                    self.current_agent = None;
                    self.check_task(t);
                }
                Decl::Agent(a) => {
                    self.current_agent = Some(a.name.clone());
                    for item in &a.items {
                        match item {
                            AgentItem::Task(t) => self.check_task(t),
                            AgentItem::On(h) => self.check_on_handler(h),
                            AgentItem::Attribute(attr) => self.check_attribute(attr),
                            AgentItem::State(_) => {}
                        }
                    }
                    self.current_agent = None;
                }
                Decl::Stmt((stmt, span)) => {
                    let mut scope = Scope::new();
                    self.check_stmt(stmt, span.clone(), &mut scope);
                }
                _ => {}
            }
        }
    }

    /// New lexical scope for a task, handler, or attribute body.
    /// Agent-owned tasks are explicit `self.task(...)` calls rather than
    /// bare names injected into lexical scope.
    fn fresh_scope(&self) -> Scope {
        Scope::new()
    }

    /// Bind `binding` to `ty` in `scope`, expanding destructure patterns field by field.
    fn bind_to_scope(&mut self, binding: &Binding, ty: &Ty, scope: &mut Scope) {
        match binding {
            Binding::Ident(name) => {
                scope.define(name.clone(), ty.clone());
            }
            Binding::Destruct(DestructPat::Struct(fields)) => {
                let struct_fields: Vec<(String, Ty)> = match ty.strip_nullable() {
                    Ty::Struct(f) => f.clone(),
                    Ty::Unknown | Ty::Dynamic => {
                        for (_, local) in fields {
                            scope.define(local.clone(), Ty::Unknown);
                        }
                        return;
                    }
                    other => {
                        self.err(format!(
                            "cannot destructure {} as a struct",
                            describe_ty(other)
                        ));
                        for (_, local) in fields {
                            scope.define(local.clone(), Ty::Unknown);
                        }
                        return;
                    }
                };
                for (source, local) in fields {
                    let field_ty = struct_fields
                        .iter()
                        .find(|(n, _)| n == source)
                        .map(|(_, t)| t.clone())
                        .unwrap_or_else(|| {
                            self.err(format!("field `{source}` not found in struct"));
                            Ty::Unknown
                        });
                    scope.define(local.clone(), field_ty);
                }
            }
            Binding::Destruct(DestructPat::Tuple(names)) => {
                let elem_tys: Vec<Ty> = match ty.strip_nullable() {
                    Ty::Tuple(items) => items.clone(),
                    Ty::Unknown | Ty::Dynamic => {
                        for name in names {
                            scope.define(name.clone(), Ty::Unknown);
                        }
                        return;
                    }
                    other => {
                        self.err(format!(
                            "cannot destructure {} as a tuple",
                            describe_ty(other)
                        ));
                        for name in names {
                            scope.define(name.clone(), Ty::Unknown);
                        }
                        return;
                    }
                };
                if names.len() != elem_tys.len() {
                    self.err(format!(
                        "tuple destructure expects {} element(s), got {}",
                        elem_tys.len(),
                        names.len()
                    ));
                }
                for (i, name) in names.iter().enumerate() {
                    let t = elem_tys.get(i).cloned().unwrap_or(Ty::Unknown);
                    scope.define(name.clone(), t);
                }
            }
        }
    }

    fn check_task(&mut self, t: &TaskDecl) {
        let declared_return = t.return_type.as_ref().map(|ty| self.resolve_type(ty));
        let prev_return_ty = self.current_return_ty.take();
        self.current_return_ty = declared_return;

        let mut scope = self.fresh_scope();
        for p in &t.params {
            let elem_ty = self.resolve_type(&p.ty);
            // Variadic params are visible inside the body as `list[T]`.
            let param_ty = if p.variadic {
                Ty::List(Box::new(elem_ty))
            } else {
                elem_ty
            };
            self.bind_to_scope(&p.name, &param_ty, &mut scope);
        }

        // Only check implicit return when the last statement is an expression.
        // Control-flow statements (return, when, if, for, try/catch) manage
        // their own return paths; checking them here produces false positives.
        let last_is_expr = t
            .body
            .last()
            .map(|(s, _)| matches!(s, Stmt::Expr(_)))
            .unwrap_or(false);
        let implicit_ty = self.block_type(&t.body, &mut scope);
        if last_is_expr
            && let Some(expected) = &self.current_return_ty.clone()
            && !matches!(expected, Ty::None_ | Ty::Unknown | Ty::Dynamic)
        {
            self.expect(&implicit_ty, expected, "implicit return");
        }

        self.current_return_ty = prev_return_ty;
    }

    fn check_on_handler(&mut self, h: &OnHandler) {
        let mut scope = self.fresh_scope();
        if let Some(p) = &h.param {
            let param_ty = self.resolve_type(&p.ty);
            self.bind_to_scope(&p.name, &param_ty, &mut scope);
        }
        self.check_block(&h.body, &mut scope);
    }

    fn check_attribute(&mut self, attr: &AttributeDecl) {
        match &attr.body {
            AttributeBody::Block(body) => {
                let mut scope = self.fresh_scope();
                self.check_block(body, &mut scope);
            }
            AttributeBody::Expr(e) => {
                let mut scope = self.fresh_scope();
                self.infer_expr(e, &mut scope);
            }
            AttributeBody::Tools(entries) => {
                let mut scope = self.fresh_scope();
                for entry in entries {
                    if let Some(cond) = &entry.condition {
                        let ty = self.infer_expr(cond, &mut scope);
                        if !matches!(ty, Ty::Bool | Ty::Unknown) {
                            self.err(format!(
                                "`when` guard on `{}` must be a bool expression",
                                entry.namespace
                            ));
                        }
                    }
                }
            }
        }
    }

    fn check_block(&mut self, block: &Block, scope: &mut Scope) {
        scope.push();
        for (stmt, span) in block {
            self.check_stmt(stmt, span.clone(), scope);
        }
        scope.pop();
    }

    fn check_stmt(&mut self, stmt: &Stmt, span: Span, scope: &mut Scope) {
        self.current_span = Some(span.clone());
        match stmt {
            Stmt::Let { binding, ty, value } => {
                let inferred = self.infer_expr(value, scope);
                let bound = match ty {
                    Some(t) => {
                        let declared = self.resolve_type(t);
                        // Only check when declared type is concrete — Unknown
                        // means the checker couldn't resolve it (e.g. a named
                        // user-defined type), so a mismatch would be a false positive.
                        if let Binding::Ident(name) = binding
                            && !matches!(declared, Ty::Unknown | Ty::Dynamic)
                        {
                            self.expect(&inferred, &declared, &format!("`{name}`"));
                        }
                        declared
                    }
                    None => {
                        if self.strict
                            && matches!(inferred, Ty::Unknown)
                            && let Binding::Ident(name) = binding
                        {
                            self.err(format!(
                                "cannot infer type of `{name}`; consider adding a type annotation"
                            ));
                        }
                        inferred
                    }
                };
                self.bind_to_scope(binding, &bound, scope);
            }
            Stmt::SelfAssign { field, value } => {
                let Some(agent_name) = &self.current_agent.clone() else {
                    self.err(format!("`self.{field}` used outside an agent"));
                    return;
                };
                let (field_exists, is_readonly) = self
                    .agents
                    .get(agent_name)
                    .map(|a| {
                        (
                            a.state_fields.contains_key(field),
                            a.readonly_fields.contains(field),
                        )
                    })
                    .unwrap_or((false, false));
                if !field_exists {
                    self.err(format!("agent `{agent_name}` has no state field `{field}`"));
                }
                if is_readonly {
                    self.err(format!(
                        "cannot assign to `self.{field}`: field is declared readonly"
                    ));
                }
                self.infer_expr(value, scope);
            }
            Stmt::Return(opt) => {
                if let Some(e) = opt {
                    let actual = self.infer_expr(e, scope);
                    if let Some(expected) = self.current_return_ty.clone()
                        && !matches!(expected, Ty::None_ | Ty::Unknown | Ty::Dynamic)
                    {
                        self.expect(&actual, &expected, "return value");
                    }
                }
            }
            Stmt::For {
                binding,
                iter,
                filter,
                body,
            } => {
                let iter_ty = self.infer_expr(iter, scope);
                let element_ty = match iter_ty.strip_nullable() {
                    Ty::List(inner) => *inner.clone(),
                    Ty::Unknown | Ty::Dynamic => Ty::Unknown,
                    Ty::Struct(fields) => {
                        // Allow iterating over a struct that implements Iterable.
                        // Find the struct's type name by matching its field set.
                        let field_names: std::collections::HashSet<&str> =
                            fields.iter().map(|(n, _)| n.as_str()).collect();
                        let is_iterable = self.structs.iter().any(|(type_name, schema)| {
                            let schema_names: std::collections::HashSet<&str> =
                                schema.iter().map(|(n, _)| n.as_str()).collect();
                            schema_names == field_names && self.iterable_types.contains(type_name)
                        });
                        if is_iterable {
                            Ty::Unknown
                        } else {
                            self.err("`for` expects a list, got struct".to_string());
                            Ty::Unknown
                        }
                    }
                    other => {
                        self.err(format!("`for` expects a list, got {}", describe_ty(other)));
                        Ty::Unknown
                    }
                };
                scope.push();
                self.bind_to_scope(binding, &element_ty, scope);
                if let Some(pred) = filter {
                    let pty = self.infer_expr(pred, scope);
                    self.expect(&pty, &Ty::Bool, "for-if guard");
                }
                for (s, s_span) in body {
                    self.check_stmt(s, s_span.clone(), scope);
                }
                // Restore span to the for-statement after processing body
                self.current_span = Some(span.clone());
                scope.pop();
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_ty = self.infer_expr(cond, scope);
                self.expect(&cond_ty, &Ty::Bool, "`if` condition");
                self.check_block(then_body, scope);
                if let Some(eb) = else_body {
                    self.check_block(eb, scope);
                }
            }
            Stmt::When { subject, arms } => {
                let subject_ty = self.infer_expr(subject, scope);
                self.check_when_arms(&subject_ty, arms, scope, span);
            }
            Stmt::TryCatch { body, catches } => {
                self.check_block(body, scope);
                for c in catches {
                    scope.push();
                    let ty = self.resolve_type(&c.ty);
                    scope.define(c.name.clone(), ty);
                    for (s, s_span) in &c.body {
                        self.check_stmt(s, s_span.clone(), scope);
                    }
                    scope.pop();
                }
            }
            Stmt::AugAssign { name, op, rhs } => {
                let var_ty = scope.get(name).cloned().unwrap_or_else(|| {
                    self.err(format!(
                        "augmented assignment to undefined variable `{name}`"
                    ));
                    Ty::Unknown
                });
                let rhs_ty = self.infer_expr(rhs, scope);
                if let Some(msg) = check_binop(*op, &var_ty, &rhs_ty) {
                    self.err(msg);
                }
            }
            Stmt::Raise(e) => {
                self.infer_expr(e, scope);
            }
            Stmt::While { cond, body } => {
                let cond_ty = self.infer_expr(cond, scope);
                self.expect(&cond_ty, &Ty::Bool, "`while` condition");
                self.check_block(body, scope);
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Expr(e) => {
                self.infer_expr(e, scope);
            }
        }
    }

    fn check_when_arms(
        &mut self,
        subject_ty: &Ty,
        arms: &[WhenArm],
        scope: &mut Scope,
        when_span: Span,
    ) {
        let mut has_wildcard = false;
        let mut covered: HashSet<String> = HashSet::new();
        for arm in arms {
            for p in &arm.patterns {
                match p {
                    Pattern::Wildcard => has_wildcard = true,
                    Pattern::Ident(name) | Pattern::Variant { name, .. } => {
                        covered.insert(name.clone());
                    }
                    Pattern::Literal(_) => {}
                }
            }
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
                        let field_ty = self.resolve_variant_field(subject_ty, variant_name, b, idx);
                        scope.define(b.clone(), field_ty);
                    }
                }
            }
            if let Some(g) = &arm.guard {
                let g_ty = self.infer_expr(g, scope);
                self.expect(&g_ty, &Ty::Bool, "`when` guard");
            }
            for (s, s_span) in &arm.body {
                self.check_stmt(s, s_span.clone(), scope);
            }
            scope.pop();
        }

        // Restore the when-statement's span so exhaustiveness errors point
        // to the `when` line, not to whatever was last checked inside arms.
        self.current_span = Some(when_span);

        // Exhaustiveness
        match subject_ty.strip_nullable() {
            Ty::Enum(name, _) => {
                if has_wildcard {
                    return;
                }
                if let Some(variants) = self.enum_variants.get(name) {
                    let missing: Vec<&String> =
                        variants.iter().filter(|v| !covered.contains(*v)).collect();
                    if !missing.is_empty() {
                        let names: Vec<String> = missing.iter().map(|s| s.to_string()).collect();
                        self.err(format!(
                            "non-exhaustive `when` on enum `{name}` — missing: {}",
                            names.join(", ")
                        ));
                    }
                }
            }
            Ty::Unknown | Ty::Dynamic => {
                // Shallow inference: don't insist on wildcard for unknown subjects.
            }
            _ => {
                if !has_wildcard {
                    self.err(format!(
                        "`when` on non-enum type `{}` requires a `_` wildcard arm",
                        describe_ty(subject_ty)
                    ));
                }
            }
        }
    }

    /// Infer type-parameter bindings from a concrete argument type.
    ///
    /// Walks `param_expr` against `arg_ty`, populating `env` with
    /// name → concrete-type mappings for each name in `type_params`.
    /// Handles named params, nullable, list, set, and generic struct/enum
    /// applications. Falls back gracefully when the shape cannot be matched.
    fn unify_type_params(
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
                    Ty::Struct(concrete_fields) => {
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
                                        &gfield.ty,
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
    fn resolve_variant_field(
        &self,
        subject_ty: &Ty,
        variant_name: &str,
        binding: &str,
        _idx: usize,
    ) -> Ty {
        let Ty::Enum(enum_name, type_args) = subject_ty.strip_nullable() else {
            return Ty::Unknown;
        };
        if type_args.is_empty() {
            return Ty::Unknown;
        }
        let Some((type_params, type_def)) = self.generic_decls.get(enum_name) else {
            return Ty::Unknown;
        };
        let TypeDef::RichEnum(variants) = type_def else {
            return Ty::Unknown;
        };
        let Some(variant) = variants.iter().find(|v| v.name == variant_name) else {
            return Ty::Unknown;
        };
        let Some(fields) = &variant.fields else {
            return Ty::Unknown;
        };
        let Some(field) = fields.iter().find(|f| f.name == binding) else {
            return Ty::Unknown;
        };
        if type_params.len() != type_args.len() {
            return Ty::Unknown;
        }
        let env: HashMap<String, Ty> = type_params
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect();
        self.resolve_type_with_env(&field.ty, &env)
    }

    // -----------------------------------------------------------------
    // Expression inference
    // -----------------------------------------------------------------

    fn infer_expr(&mut self, expr: &Expr, scope: &mut Scope) -> Ty {
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
                let mut inferred: Vec<(String, Ty)> = Vec::with_capacity(fields.len());
                for (k, v) in fields {
                    let ty = self.infer_expr(v, scope);
                    inferred.push((k.clone(), ty));
                }
                Ty::Struct(inferred)
            }

            Expr::StructSpreadUpdate { base, overrides } => {
                let base_ty = self.infer_expr(base, scope);
                let base_fields = match base_ty.strip_nullable() {
                    Ty::Struct(fields) => fields.clone(),
                    Ty::Unknown | Ty::Dynamic => {
                        for (_, v) in overrides {
                            self.infer_expr(v, scope);
                        }
                        return Ty::Unknown;
                    }
                    other => {
                        self.err(format!(
                            "spread-update base must be a struct, got {}",
                            describe_ty(other)
                        ));
                        for (_, v) in overrides {
                            self.infer_expr(v, scope);
                        }
                        return Ty::Unknown;
                    }
                };
                let mut result_fields = base_fields.clone();
                for (k, v) in overrides {
                    let val_ty = self.infer_expr(v, scope);
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
                if !matches!(idx_ty.strip_nullable(), Ty::Int | Ty::Unknown | Ty::Dynamic) {
                    self.err(format!(
                        "subscript index must be int, got {}",
                        describe_ty(&idx_ty)
                    ));
                }
                match obj_ty.strip_nullable() {
                    Ty::List(elem) => *elem.clone(),
                    Ty::Str => Ty::Str,
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
                    if name == "Ai"
                        && method == "classify"
                        && let Some(as_arg) = args.iter().find(|a| a.name.as_deref() == Some("as"))
                        && let Expr::Ident(enum_name) = &as_arg.value
                        && self.enum_variants.contains_key(enum_name)
                    {
                        let base = Ty::Enum(enum_name.clone(), vec![]);
                        return Ty::Nullable(Box::new(base));
                    }
                    if name == "Ai" {
                        match method.as_str() {
                            "draft" | "summarize" | "translate" | "prompt" => {
                                return Ty::Nullable(Box::new(Ty::Str));
                            }
                            "extract" | "decide" => {
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
                                return Ty::Nullable(Box::new(inner));
                            }
                            _ => {}
                        }
                    }
                    if name == "Io" {
                        match method.as_str() {
                            "ask" => return Ty::Str,
                            "confirm" => return Ty::Bool,
                            "notify" | "show" => return Ty::None_,
                            _ => {}
                        }
                    }
                    if name == "Env" {
                        match method.as_str() {
                            "get" => return Ty::Nullable(Box::new(Ty::Str)),
                            "require" => return Ty::Str,
                            _ => {}
                        }
                    }
                    if name == "Time" {
                        match method.as_str() {
                            "now" => return Ty::Datetime,
                            "parse" => return Ty::Nullable(Box::new(Ty::Datetime)),
                            "epoch_ms" => return Ty::Int,
                            _ => {}
                        }
                    }
                    if name == "File" {
                        match method.as_str() {
                            "read" => return Ty::Str,
                            "write" | "mkdir" | "remove" | "copy" | "move" => return Ty::None_,
                            "exists" => return Ty::Bool,
                            "list" | "glob" => return Ty::List(Box::new(Ty::Str)),
                            "mktemp" => return Ty::Str,
                            _ => {}
                        }
                    }
                    if name == "Random" {
                        match method.as_str() {
                            "float" => return Ty::Float,
                            "int" => return Ty::Int,
                            "bool" => return Ty::Bool,
                            _ => {}
                        }
                    }
                    if name == "Uuid" {
                        match method.as_str() {
                            "v4" | "v7" | "v5" => return Ty::Uuid,
                            "parse" => return Ty::Nullable(Box::new(Ty::Uuid)),
                            _ => {}
                        }
                    }
                    if name == "Crypto" {
                        match method.as_str() {
                            "sha224" | "sha256" | "sha384" | "sha512" | "sha512_224"
                            | "sha512_256" | "hmac_sha224" | "hmac_sha256" | "hmac_sha384"
                            | "hmac_sha512" | "hmac_sha512_224" | "hmac_sha512_256" | "token" => {
                                return Ty::Str;
                            }
                            "random_bytes" => return Ty::List(Box::new(Ty::Int)),
                            _ => {}
                        }
                    }
                    if name == "Math" {
                        match method.as_str() {
                            "PI" | "E" | "sqrt" | "pow" | "exp" | "log" | "log2" | "log10"
                            | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2" => {
                                return Ty::Float;
                            }
                            _ => {}
                        }
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

    /// Check inferred argument types against declared parameter types.
    /// Positional args fill params in order; named args match by param name
    /// (mirroring the interpreter's Python-style keyword-argument convention).
    /// When `variadic` is true the last param is a rest-parameter (`...name: T`):
    ///   - plain positional args beyond the fixed params are each checked as `T`
    ///   - spread args (`...expr`) must be `list[T]` or `set[T]`
    fn check_call_args(
        &mut self,
        params: &[(String, Ty)],
        variadic: bool,
        args: &[crate::ast::CallArg],
        arg_tys: &[Ty],
        callee: &str,
    ) {
        if !variadic && args.iter().any(|a| a.spread) {
            self.err(format!(
                "{callee}: spread args (`...`) require a variadic callee"
            ));
            return;
        }
        let named: HashMap<&str, &Ty> = args
            .iter()
            .zip(arg_tys.iter())
            .filter_map(|(a, ty)| a.name.as_deref().map(|n| (n, ty)))
            .collect();
        // Plain positional args — not named, not spread.
        let positional: Vec<&Ty> = args
            .iter()
            .zip(arg_tys.iter())
            .filter(|(a, _)| a.name.is_none() && !a.spread)
            .map(|(_, ty)| ty)
            .collect();

        let fixed_params = if variadic && !params.is_empty() {
            &params[..params.len() - 1]
        } else {
            params
        };

        let mut pos_idx = 0;
        for (param_name, param_ty) in fixed_params {
            let arg_ty = if let Some(ty) = named.get(param_name.as_str()) {
                *ty
            } else if let Some(ty) = positional.get(pos_idx) {
                pos_idx += 1;
                *ty
            } else {
                continue;
            };
            self.expect(arg_ty, param_ty, &format!("{callee} arg `{param_name}`"));
        }

        if variadic && let Some((var_name, elem_ty)) = params.last() {
            // Check each remaining plain positional arg against the element type.
            for arg_ty in positional.iter().skip(pos_idx) {
                self.expect(
                    arg_ty,
                    elem_ty,
                    &format!("{callee} variadic arg `{var_name}`"),
                );
            }
            // Check spread args: each must be list[T] or set[T].
            for (_a, arg_ty) in args.iter().zip(arg_tys.iter()).filter(|(a, _)| a.spread) {
                let expected_list = Ty::List(Box::new(elem_ty.clone()));
                let expected_set = Ty::Set(Box::new(elem_ty.clone()));
                let ok = match arg_ty {
                    Ty::List(inner) | Ty::Set(inner) => self.types_match(inner.as_ref(), elem_ty),
                    _ => false,
                };
                if !ok {
                    self.err(format!(
                        "{callee}: spread arg `...` must be `{}` or `{}`, got `{}`",
                        describe_ty(&expected_list),
                        describe_ty(&expected_set),
                        describe_ty(arg_ty),
                    ));
                }
            }
        }
    }

    /// Structural type equality (ignoring nullability wrapping differences).
    fn types_match(&self, a: &Ty, b: &Ty) -> bool {
        match (a, b) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => true,
            (Ty::Dynamic, _) | (_, Ty::Dynamic) => true,
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
        if matches!(actual, Ty::Unknown | Ty::Dynamic) {
            return;
        }
        if matches!(expected, Ty::Unknown | Ty::Dynamic) {
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
            && matches!(key_ty.as_ref(), Ty::Str | Ty::Unknown | Ty::Dynamic)
        {
            for (name, act_ty) in actual_fields {
                self.expect(act_ty, value_ty, &format!("{context}[{name}]"));
            }
            return;
        }

        if actual_base != expected_base && !matches!(actual_base, Ty::Unknown | Ty::Dynamic) {
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
// Hover support — `type_at(text, offset)` returns a one-line type label
// for the identifier under the cursor. Used by the LSP `hover` handler.
// ---------------------------------------------------------------------------

/// Get the identifier and its span at the given byte offset.
pub fn ident_at_offset(text: &str, offset: usize) -> Option<String> {
    use crate::lexer::Token;
    use logos::Logos;

    for (result, span) in Token::lexer(text).spanned() {
        if span.start > offset {
            break;
        }
        if span.end < offset {
            continue;
        }
        if let Ok(Token::Ident(n)) = result {
            return Some(n);
        }
    }
    None
}

/// Get the span of the identifier at the given byte offset.
pub fn ident_span_at_offset(text: &str, offset: usize) -> Option<crate::lexer::Span> {
    use crate::lexer::Token;
    use logos::Logos;

    for (result, span) in Token::lexer(text).spanned() {
        if span.start > offset {
            break;
        }
        if span.end < offset {
            continue;
        }
        if let Ok(Token::Ident(_)) = result {
            return Some(span);
        }
    }
    None
}

/// Find the declaration span of an identifier at the given offset.
/// Returns the span of the declared name in task/agent/type declarations.
pub fn definition_of(text: &str, offset: usize) -> Option<crate::lexer::Span> {
    use crate::lexer::Token;
    use logos::Logos;

    let name = ident_at_offset(text, offset)?;

    let tokens: Vec<(Token, crate::lexer::Span)> = Token::lexer(text)
        .spanned()
        .filter_map(|(r, s)| r.ok().map(|t| (t, s)))
        .collect();

    for i in 0..tokens.len().saturating_sub(1) {
        match (&tokens[i].0, &tokens[i + 1].0) {
            (Token::Task | Token::Agent | Token::Type, Token::Ident(n)) if n == &name => {
                return Some(tokens[i + 1].1.clone());
            }
            _ => {}
        }
    }
    None
}

/// Find all spans of usages of the given identifier name.
pub fn usages_of(text: &str, name: &str) -> Vec<crate::lexer::Span> {
    use crate::lexer::Token;
    use logos::Logos;

    Token::lexer(text)
        .spanned()
        .filter_map(|(r, s)| r.ok().map(|t| (t, s)))
        .filter_map(|(tok, span)| {
            if let Token::Ident(n) = tok
                && n == name
            {
                return Some(span);
            }
            None
        })
        .collect()
}

/// Resolve the inferred type for the identifier at `offset` (UTF-8 byte
/// offset into `text`). Returns `None` if the cursor is not on an
/// identifier or the identifier can't be resolved.
pub fn type_at(text: &str, offset: usize) -> Option<String> {
    let name = ident_at_offset(text, offset)?;

    if matches!(
        name.as_str(),
        "Ai" | "Io"
            | "Http"
            | "Shell"
            | "Email"
            | "Search"
            | "Db"
            | "Memory"
            | "Schedule"
            | "Async"
            | "Control"
            | "Env"
            | "Time"
            | "Log"
            | "Agent"
            | "Cache"
            | "File"
            | "Json"
            | "Random"
            | "Uuid"
            | "Crypto"
            | "Math"
    ) {
        return Some(format!("namespace `{name}`"));
    }
    if matches!(
        name.as_str(),
        "int"
            | "float"
            | "str"
            | "bool"
            | "none"
            | "datetime"
            | "duration"
            | "Uuid"
            | "list"
            | "map"
            | "set"
            | "dynamic"
    ) {
        return Some(format!("type `{name}`"));
    }

    let named = miette::NamedSource::new("file", text.to_string());
    let tokens = crate::lexer::lex(text, &named).ok()?;
    let program = crate::parser::parse(tokens, text.len(), &named).ok()?;

    let mut bindings: HashMap<String, Ty> = HashMap::new();
    let mut checker = Checker::new();
    checker.collect(&program);
    collect_decl_bindings(&program, &mut checker, &mut bindings);

    bindings.get(&name).map(describe_ty)
}

fn insert_binding(binding: &Binding, ty: Ty, _c: &mut Checker, out: &mut HashMap<String, Ty>) {
    match binding {
        Binding::Ident(name) => {
            out.insert(name.clone(), ty);
        }
        Binding::Destruct(DestructPat::Struct(fields)) => {
            let struct_fields = match &ty {
                Ty::Struct(f) => f.clone(),
                _ => vec![],
            };
            for (source, local) in fields {
                let field_ty = struct_fields
                    .iter()
                    .find(|(n, _)| n == source)
                    .map(|(_, t)| t.clone())
                    .unwrap_or(Ty::Unknown);
                out.insert(local.clone(), field_ty);
            }
        }
        Binding::Destruct(DestructPat::Tuple(names)) => {
            let elem_tys = match ty {
                Ty::Tuple(items) => items,
                _ => vec![],
            };
            for (i, name) in names.iter().enumerate() {
                let t = elem_tys.get(i).cloned().unwrap_or(Ty::Unknown);
                out.insert(name.clone(), t);
            }
        }
    }
}

fn collect_decl_bindings(program: &Program, c: &mut Checker, out: &mut HashMap<String, Ty>) {
    for (decl, _) in &program.declarations {
        match decl {
            Decl::Stmt((stmt, _)) => collect_stmt_bindings(stmt, c, out),
            Decl::Task(t) => {
                for p in &t.params {
                    insert_binding(&p.name, c.resolve_type(&p.ty), c, out);
                }
                for (s, _) in &t.body {
                    collect_stmt_bindings(s, c, out);
                }
            }
            Decl::Agent(decl) => {
                for it in &decl.items {
                    match it {
                        AgentItem::State(fields) => {
                            for sf in fields {
                                out.insert(sf.name.clone(), c.resolve_type(&sf.ty));
                            }
                        }
                        AgentItem::Task(t) => {
                            for p in &t.params {
                                insert_binding(&p.name, c.resolve_type(&p.ty), c, out);
                            }
                            for (s, _) in &t.body {
                                collect_stmt_bindings(s, c, out);
                            }
                        }
                        AgentItem::On(h) => {
                            if let Some(p) = &h.param {
                                insert_binding(&p.name, c.resolve_type(&p.ty), c, out);
                            }
                            for (s, _) in &h.body {
                                collect_stmt_bindings(s, c, out);
                            }
                        }
                        AgentItem::Attribute(attr) => {
                            if let AttributeBody::Block(block) = &attr.body {
                                for (s, _) in block {
                                    collect_stmt_bindings(s, c, out);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_stmt_bindings(stmt: &Stmt, c: &mut Checker, out: &mut HashMap<String, Ty>) {
    match stmt {
        Stmt::Let { binding, ty, value } => {
            let mut scope = Scope::new();
            let inferred = c.infer_expr(value, &mut scope);
            let bound = ty.as_ref().map(|t| c.resolve_type(t)).unwrap_or(inferred);
            insert_binding(binding, bound, c, out);
        }
        Stmt::For {
            binding,
            iter,
            body,
            ..
        } => {
            let mut scope = Scope::new();
            let iter_ty = c.infer_expr(iter, &mut scope);
            let elem = match iter_ty.strip_nullable() {
                Ty::List(inner) => *inner.clone(),
                _ => Ty::Unknown,
            };
            insert_binding(binding, elem, c, out);
            for (s, _) in body {
                collect_stmt_bindings(s, c, out);
            }
        }
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            for (s, _) in then_body {
                collect_stmt_bindings(s, c, out);
            }
            if let Some(eb) = else_body {
                for (s, _) in eb {
                    collect_stmt_bindings(s, c, out);
                }
            }
        }
        Stmt::When { arms, .. } => {
            for arm in arms {
                for (s, _) in &arm.body {
                    collect_stmt_bindings(s, c, out);
                }
            }
        }
        Stmt::While { body, .. } => {
            for (s, _) in body {
                collect_stmt_bindings(s, c, out);
            }
        }
        Stmt::TryCatch { body, catches } => {
            for (s, _) in body {
                collect_stmt_bindings(s, c, out);
            }
            for catch in catches {
                out.insert(catch.name.clone(), c.resolve_type(&catch.ty));
                for (s, _) in &catch.body {
                    collect_stmt_bindings(s, c, out);
                }
            }
        }
        _ => {}
    }
}

/// Bind type-parameter names from a `TypeExpr`/`Ty` pair into `env`.
///
/// Free-function counterpart to `Checker::unify_type_params` used for the
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

fn describe_ty(ty: &Ty) -> String {
    match ty {
        Ty::Int => "int".into(),
        Ty::Float => "float".into(),
        Ty::Str => "str".into(),
        Ty::Bool => "bool".into(),
        Ty::None_ => "none".into(),
        Ty::Duration => "duration".into(),
        Ty::Datetime => "datetime".into(),
        Ty::Uuid => "Uuid".into(),
        Ty::List(inner) => format!("list[{}]", describe_ty(inner)),
        Ty::Map(k, v) => format!("map[{}, {}]", describe_ty(k), describe_ty(v)),
        Ty::Set(inner) => format!("set[{}]", describe_ty(inner)),
        Ty::Struct(_) => "struct".into(),
        Ty::Tuple(items) => {
            let s: Vec<String> = items.iter().map(describe_ty).collect();
            format!("({})", s.join(", "))
        }
        Ty::Func(_, _) => "function".into(),
        Ty::Enum(name, _) => name.clone(),
        Ty::Unknown => "unknown".into(),
        Ty::Nullable(inner) => format!("{}?", describe_ty(inner)),
        Ty::Dynamic => "dynamic".into(),
    }
}

fn op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Lte => "<=",
        BinOp::Gte => ">=",
        BinOp::Eq => "==",
        BinOp::Neq => "!=",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}

fn check_binop(op: BinOp, l: &Ty, r: &Ty) -> Option<String> {
    let lb = l.strip_nullable();
    let rb = r.strip_nullable();

    if matches!(lb, Ty::Unknown | Ty::Dynamic) || matches!(rb, Ty::Unknown | Ty::Dynamic) {
        return None;
    }

    let ok = match op {
        BinOp::Add => {
            matches!(
                (lb, rb),
                (Ty::Int, Ty::Int)
                    | (Ty::Float, Ty::Float)
                    | (Ty::Int, Ty::Float)
                    | (Ty::Float, Ty::Int)
                    | (Ty::Str, Ty::Str)
                    | (Ty::Datetime, Ty::Duration)
                    | (Ty::Duration, Ty::Datetime)
                    | (Ty::Duration, Ty::Duration)
            ) || matches!((lb, rb), (Ty::List(_), Ty::List(_)))
        }

        BinOp::Sub => matches!(
            (lb, rb),
            (Ty::Int, Ty::Int)
                | (Ty::Float, Ty::Float)
                | (Ty::Int, Ty::Float)
                | (Ty::Float, Ty::Int)
                | (Ty::Datetime, Ty::Duration)
                | (Ty::Datetime, Ty::Datetime)
                | (Ty::Duration, Ty::Duration)
        ),

        BinOp::Mul | BinOp::Div | BinOp::Mod => matches!(
            (lb, rb),
            (Ty::Int, Ty::Int)
                | (Ty::Float, Ty::Float)
                | (Ty::Int, Ty::Float)
                | (Ty::Float, Ty::Int)
        ),

        BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => matches!(
            (lb, rb),
            (Ty::Int, Ty::Int)
                | (Ty::Float, Ty::Float)
                | (Ty::Int, Ty::Float)
                | (Ty::Float, Ty::Int)
                | (Ty::Str, Ty::Str)
                | (Ty::Datetime, Ty::Datetime)
                | (Ty::Duration, Ty::Duration)
        ),

        BinOp::Eq | BinOp::Neq | BinOp::And | BinOp::Or => true,
    };

    if ok {
        None
    } else {
        Some(format!(
            "cannot apply `{}` to {} and {}",
            op_symbol(op),
            describe_ty(lb),
            describe_ty(rb)
        ))
    }
}

fn infer_binary(op: BinOp, l: &Ty, r: &Ty) -> Ty {
    use BinOp::*;
    let lb = l.strip_nullable();
    let rb = r.strip_nullable();
    match op {
        Add | Sub | Mul | Div | Mod => match (lb, rb) {
            (Ty::Int, Ty::Int) => Ty::Int,
            (Ty::Float, Ty::Float) => Ty::Float,
            (Ty::Float, Ty::Int) | (Ty::Int, Ty::Float) => Ty::Float,
            (Ty::Str, Ty::Str) if matches!(op, Add) => Ty::Str,
            (Ty::List(le), Ty::List(_)) if matches!(op, Add) => Ty::List(le.clone()),
            _ => Ty::Unknown,
        },
        Eq | Neq | Lt | Gt | Lte | Gte => Ty::Bool,
        And | Or => Ty::Bool,
    }
}

// ---------------------------------------------------------------------------
// Interface helpers
// ---------------------------------------------------------------------------

/// Stringify a `TypeExpr` for conformance comparison — mirrors the logic in
/// `interpreter::decl::type_expr_to_string` so the two checks stay in sync.
fn type_expr_str(te: &TypeExpr) -> String {
    match te {
        TypeExpr::Named(n) => n.clone(),
        TypeExpr::Nullable(inner) => format!("{}?", type_expr_str(inner)),
        TypeExpr::List(inner) => format!("[{}]", type_expr_str(inner)),
        TypeExpr::Map(k, v) => format!("[{}: {}]", type_expr_str(k), type_expr_str(v)),
        TypeExpr::Set(inner) => format!("set[{}]", type_expr_str(inner)),
        TypeExpr::Tuple(items) => {
            let parts: Vec<_> = items.iter().map(type_expr_str).collect();
            format!("({})", parts.join(", "))
        }
        TypeExpr::Func(params, ret) => {
            let ps: Vec<_> = params.iter().map(type_expr_str).collect();
            format!("({}) -> {}", ps.join(", "), type_expr_str(ret))
        }
        TypeExpr::Dynamic => "dynamic".to_string(),
        TypeExpr::Struct(_) | TypeExpr::Generic(_, _) => "unknown".to_string(),
    }
}

fn checker_return_types_match(req: &str, got: &str) -> bool {
    if req == got {
        return true;
    }
    // "unknown" covers Struct/Generic return types — accept any concrete type at v0.1.
    if req == "unknown" {
        return true;
    }
    // "dynamic" is TypeExpr::Dynamic — an explicit wildcard in interface signatures.
    if req == "dynamic" {
        return true;
    }
    // list[dynamic] or list[unknown] in an interface sig accept any list[T].
    if (req == "[dynamic]" || req == "[unknown]") && got.starts_with('[') {
        return true;
    }
    false
}

fn checker_builtin_interfaces() -> HashMap<String, Vec<crate::ast::TaskSig>> {
    let mut map = HashMap::new();

    let self_param = || Param {
        name: Binding::Ident("self".to_string()),
        ty: TypeExpr::Named("__impl_self__".to_string()),
        default: None,
        variadic: false,
    };
    let dynamic_param = |name: &str| Param {
        name: Binding::Ident(name.to_string()),
        ty: TypeExpr::Dynamic,
        default: None,
        variadic: false,
    };

    map.insert(
        "Stringable".to_string(),
        vec![crate::ast::TaskSig {
            name: "to_str".to_string(),
            params: vec![self_param()],
            return_type: Some(TypeExpr::Named("str".to_string())),
        }],
    );
    map.insert(
        "Serializable".to_string(),
        vec![crate::ast::TaskSig {
            name: "to_json".to_string(),
            params: vec![self_param()],
            return_type: Some(TypeExpr::Named("str".to_string())),
        }],
    );
    map.insert(
        "Comparable".to_string(),
        vec![crate::ast::TaskSig {
            name: "compare".to_string(),
            params: vec![self_param(), dynamic_param("other")],
            return_type: Some(TypeExpr::Named("int".to_string())),
        }],
    );
    map.insert(
        "Equatable".to_string(),
        vec![crate::ast::TaskSig {
            name: "equals".to_string(),
            params: vec![self_param(), dynamic_param("other")],
            return_type: Some(TypeExpr::Named("bool".to_string())),
        }],
    );
    map.insert(
        "Iterable".to_string(),
        vec![crate::ast::TaskSig {
            name: "items".to_string(),
            params: vec![self_param()],
            return_type: Some(TypeExpr::List(Box::new(TypeExpr::Dynamic))),
        }],
    );
    map
}
