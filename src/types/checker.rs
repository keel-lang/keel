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
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Set(Box<Ty>),
    Struct(Vec<(String, Ty)>),
    Tuple(Vec<Ty>),
    Func(Vec<Ty>, Box<Ty>),
    Enum(String),
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
}

#[derive(Debug, Clone)]
struct AgentInfo {
    state_fields: HashMap<String, Ty>,
    /// Collected but not yet used in type checks — populated for future
    /// cross-agent call validation.
    #[allow(dead_code)]
    tasks: HashMap<String, TaskSig>,
    #[allow(dead_code)]
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

pub fn check(program: &Program) -> Vec<TypeError> {
    let mut c = Checker::new();
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
            "Env", "Time", "Log", "Agent", "Cache", "Str", "File", "Json",
        ] {
            prelude.insert(n.to_string());
        }
        // Top-level builtins
        for n in ["run", "stop"] {
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

        Checker {
            errors: Vec::new(),
            enum_variants: HashMap::new(),
            structs: HashMap::new(),
            aliases: HashMap::new(),
            top_tasks: HashMap::new(),
            agents: HashMap::new(),
            current_agent: None,
            current_return_ty: None,
            prelude,
            current_span: None,
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

    #[allow(dead_code)]
    fn err_at(&mut self, msg: impl Into<String>, span: Span) {
        self.errors.push(TypeError::new(msg).at(span));
    }

    // -----------------------------------------------------------------
    // Collection pass
    // -----------------------------------------------------------------

    fn collect(&mut self, program: &Program) {
        for (decl, _) in &program.declarations {
            match decl {
                Decl::Type(t) => self.collect_type_decl(t),
                Decl::Task(t) => {
                    let sig = self.task_sig(t);
                    self.top_tasks.insert(t.name.clone(), sig);
                }
                Decl::Agent(a) => {
                    let info = self.agent_info(a);
                    self.agents.insert(a.name.clone(), info);
                }
                _ => {}
            }
        }
    }

    fn collect_type_decl(&mut self, t: &TypeDecl) {
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
        let params = t
            .params
            .iter()
            .map(|p| {
                let name = match &p.name {
                    Binding::Ident(n) => n.clone(),
                    Binding::Destruct(_) => "_".to_string(),
                };
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
        }
    }

    fn agent_info(&self, a: &AgentDecl) -> AgentInfo {
        let mut state_fields = HashMap::new();
        let mut tasks = HashMap::new();
        let mut handlers = HashSet::new();
        for item in &a.items {
            match item {
                AgentItem::State(fields) => {
                    for f in fields {
                        state_fields.insert(f.name.clone(), self.resolve_type(&f.ty));
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
            tasks,
            handlers,
        }
    }

    // -----------------------------------------------------------------
    // AST type → resolved Ty
    // -----------------------------------------------------------------

    fn resolve_type(&self, ty: &TypeExpr) -> Ty {
        match ty {
            TypeExpr::Named(n) => match n.as_str() {
                "int" => Ty::Int,
                "float" => Ty::Float,
                "str" => Ty::Str,
                "bool" => Ty::Bool,
                "none" => Ty::None_,
                "datetime" => Ty::Datetime,
                "duration" => Ty::Duration,
                _ => {
                    if self.enum_variants.contains_key(n) {
                        Ty::Enum(n.clone())
                    } else if let Some(fields) = self.structs.get(n) {
                        Ty::Struct(fields.clone())
                    } else if let Some(t) = self.aliases.get(n) {
                        t.clone()
                    } else {
                        Ty::Unknown
                    }
                }
            },
            TypeExpr::Nullable(inner) => Ty::Nullable(Box::new(self.resolve_type(inner))),
            TypeExpr::List(inner) => Ty::List(Box::new(self.resolve_type(inner))),
            TypeExpr::Map(k, v) => Ty::Map(
                Box::new(self.resolve_type(k)),
                Box::new(self.resolve_type(v)),
            ),
            TypeExpr::Set(inner) => Ty::Set(Box::new(self.resolve_type(inner))),
            TypeExpr::Struct(fields) => Ty::Struct(
                fields
                    .iter()
                    .map(|f| (f.name.clone(), self.resolve_type(&f.ty)))
                    .collect(),
            ),
            TypeExpr::Tuple(items) => {
                Ty::Tuple(items.iter().map(|t| self.resolve_type(t)).collect())
            }
            TypeExpr::Func(params, ret) => Ty::Func(
                params.iter().map(|t| self.resolve_type(t)).collect(),
                Box::new(self.resolve_type(ret)),
            ),
            TypeExpr::Generic(_, _) => Ty::Unknown,
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

    /// New lexical scope pre-populated with agent-scoped tasks when
    /// `current_agent` is set. Agent tasks are callable by bare name
    /// from anywhere inside the agent body.
    fn fresh_scope(&self) -> Scope {
        let mut scope = Scope::new();
        if let Some(agent_name) = &self.current_agent
            && let Some(info) = self.agents.get(agent_name)
        {
            for (name, sig) in &info.tasks {
                scope.define(
                    name.clone(),
                    Ty::Func(
                        sig.params.iter().map(|(_, t)| t.clone()).collect(),
                        Box::new(sig.return_type.clone()),
                    ),
                );
            }
        }
        scope
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
            let param_ty = self.resolve_type(&p.ty);
            self.bind_to_scope(&p.name, &param_ty, &mut scope);
        }
        self.check_block(&t.body, &mut scope);

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
                    None => inferred,
                };
                self.bind_to_scope(binding, &bound, scope);
            }
            Stmt::SelfAssign { field, value } => {
                let Some(agent_name) = &self.current_agent.clone() else {
                    self.err(format!("`self.{field}` used outside an agent"));
                    return;
                };
                let field_ty = self
                    .agents
                    .get(agent_name)
                    .and_then(|a| a.state_fields.get(field).cloned());
                if field_ty.is_none() {
                    self.err(format!("agent `{agent_name}` has no state field `{field}`"));
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
                if let Pattern::Variant { bindings, .. } = p {
                    for b in bindings {
                        if b != "_" {
                            scope.define(b.clone(), Ty::Unknown);
                        }
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
            Ty::Enum(name) => {
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
                    if let StringPart::Interpolation(e) = p {
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
                        return Ty::Enum(name.clone());
                    }
                    if self.prelude.contains(name) {
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

            Expr::NullFieldAccess(obj, _) => {
                let _ = self.infer_expr(obj, scope);
                Ty::Unknown
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

            Expr::ListLit(items) | Expr::SetLit(items) => {
                let mut element_ty = Ty::Unknown;
                for (i, e) in items.iter().enumerate() {
                    let ty = self.infer_expr(e, scope);
                    if i == 0 {
                        element_ty = ty;
                    }
                }
                Ty::List(Box::new(element_ty))
            }

            Expr::TupleLit(items) => {
                Ty::Tuple(items.iter().map(|e| self.infer_expr(e, scope)).collect())
            }

            Expr::BinaryOp { left, op, right } => {
                let l = self.infer_expr(left, scope);
                let r = self.infer_expr(right, scope);
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
                let _ = self.infer_expr(l, scope);
                self.infer_expr(r, scope)
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
                for a in args {
                    self.infer_expr(&a.value, scope);
                }
                if let Expr::Ident(name) = callee.as_ref()
                    && let Some(sig) = self.top_tasks.get(name).cloned()
                {
                    let expected = sig.params.len();
                    // Count only positional args (named args may map to params by name).
                    let positional: usize = args.iter().filter(|a| a.name.is_none()).count();
                    if positional > expected {
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
                    return sig.return_type.clone();
                }
                let _ = self.infer_expr(callee, scope);
                Ty::Unknown
            }

            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                for a in args {
                    self.infer_expr(&a.value, scope);
                }
                // Special cases for inferring Ai.classify → Enum(T)
                if let Expr::Ident(name) = object.as_ref() {
                    if name == "Ai"
                        && method == "classify"
                        && let Some(as_arg) = args.iter().find(|a| a.name.as_deref() == Some("as"))
                        && let Expr::Ident(enum_name) = &as_arg.value
                        && self.enum_variants.contains_key(enum_name)
                    {
                        let base = Ty::Enum(enum_name.clone());
                        return if args.iter().any(|a| a.name.as_deref() == Some("fallback")) {
                            base
                        } else {
                            Ty::Nullable(Box::new(base))
                        };
                    }
                    if name == "Ai" {
                        match method.as_str() {
                            "draft" | "summarize" | "translate" | "prompt" => {
                                return Ty::Nullable(Box::new(Ty::Str));
                            }
                            "extract" => return Ty::Nullable(Box::new(Ty::Unknown)),
                            "decide" => return Ty::Nullable(Box::new(Ty::Unknown)),
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
                            _ => {}
                        }
                    }
                }
                let obj_ty = self.infer_expr(object, scope);
                match (obj_ty.strip_nullable(), method.as_str()) {
                    (Ty::List(elem), "push" | "filter") => Ty::List(elem.clone()),
                    (Ty::List(_), "len" | "count") => Ty::Int,
                    (Ty::List(_), "is_empty") => Ty::Bool,
                    (Ty::List(_), "contains") => Ty::Bool,
                    (Ty::List(elem), "first" | "last") => Ty::Nullable(elem.clone()),
                    (Ty::List(_), "map") => Ty::List(Box::new(Ty::Unknown)),
                    (Ty::Str, "len" | "count" | "length") => Ty::Int,
                    (Ty::Str, "upper" | "lower" | "trim" | "strip") => Ty::Str,
                    (Ty::Str, "split") => Ty::List(Box::new(Ty::Str)),
                    (Ty::Str, "contains" | "starts_with" | "ends_with" | "is_empty") => Ty::Bool,
                    (Ty::Str, "replace") => Ty::Str,
                    (Ty::Map(_, v), "get") => Ty::Nullable(v.clone()),
                    (Ty::Map(k, _), "keys") => Ty::List(k.clone()),
                    (Ty::Map(_, v), "values") => Ty::List(v.clone()),
                    (Ty::Map(_, _), "len" | "count" | "size") => Ty::Int,
                    (Ty::Map(_, _), "is_empty") => Ty::Bool,
                    (Ty::Map(_, _), "contains" | "has") => Ty::Bool,
                    (Ty::Datetime, "parts") => Ty::Unknown,
                    (Ty::Datetime, "format") => Ty::Nullable(Box::new(Ty::Str)),
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
                let _ = self.block_type(else_body, scope);
                then_ty
            }

            Expr::WhenExpr { subject, arms } => {
                let subject_ty = self.infer_expr(subject, scope);
                // Use a dummy span for when-expressions; the statement-level span
                // (already set in current_span) is used for any errors.
                let span = self.current_span.clone().unwrap_or(0..0);
                self.check_when_arms(&subject_ty, arms, scope, span);
                Ty::Unknown
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
                        for (s, s_span) in b {
                            self.check_stmt(s, s_span.clone(), scope);
                        }
                        Ty::Unknown
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
                Ty::Enum(name.clone())
            }
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
            | "Str"
            | "File"
            | "Json"
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

fn describe_ty(ty: &Ty) -> String {
    match ty {
        Ty::Int => "int".into(),
        Ty::Float => "float".into(),
        Ty::Str => "str".into(),
        Ty::Bool => "bool".into(),
        Ty::None_ => "none".into(),
        Ty::Duration => "duration".into(),
        Ty::Datetime => "datetime".into(),
        Ty::List(inner) => format!("list[{}]", describe_ty(inner)),
        Ty::Map(k, v) => format!("map[{}, {}]", describe_ty(k), describe_ty(v)),
        Ty::Set(inner) => format!("set[{}]", describe_ty(inner)),
        Ty::Struct(_) => "struct".into(),
        Ty::Tuple(items) => {
            let s: Vec<String> = items.iter().map(describe_ty).collect();
            format!("({})", s.join(", "))
        }
        Ty::Func(_, _) => "function".into(),
        Ty::Enum(name) => name.clone(),
        Ty::Unknown => "unknown".into(),
        Ty::Nullable(inner) => format!("{}?", describe_ty(inner)),
        Ty::Dynamic => "dynamic".into(),
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
