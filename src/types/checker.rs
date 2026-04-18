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
        TypeError { message: msg.into(), span: None }
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
        match self { Ty::Nullable(inner) => inner, _ => self }
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
    /// Pre-seeded names that must not be reported as undefined
    /// (prelude namespaces, built-in types, symbol identifiers, etc.).
    prelude: HashSet<String>,
}

/// Chained lexical scope: newer scopes on the back of the vec.
struct Scope {
    frames: Vec<HashMap<String, Ty>>,
}

impl Scope {
    fn new() -> Self {
        Scope { frames: vec![HashMap::new()] }
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
            if let Some(t) = f.get(name) { return Some(t); }
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
        for n in ["Ai", "Io", "Http", "Email", "Search", "Db", "Memory",
                 "Schedule", "Async", "Control", "Env", "Time", "Log", "Agent"] {
            prelude.insert(n.to_string());
        }
        // Top-level builtins
        for n in ["run", "stop"] {
            prelude.insert(n.to_string());
        }
        // Built-in type names
        for n in ["int", "float", "str", "bool", "none", "datetime", "duration", "dynamic",
                  "list", "map", "set", "Result", "Message", "SearchResult", "Memory",
                  "HttpResponse", "Decision", "Error",
                  "AIError", "NetworkError", "TimeoutError", "NullError",
                  "TypeError", "ParseError"] {
            prelude.insert(n.to_string());
        }
        // Symbol identifiers used as hint args (see runtime::SYMBOL_IDENTS)
        // and attribute-value keywords (`@memory persistent`, etc.).
        for n in ["sentence", "sentences", "line", "lines", "word", "words",
                  "paragraph", "paragraphs",
                  "bullets", "prose", "json",
                  "exponential", "linear", "fixed",
                  "google", "bing", "arxiv",
                  "text", "html", "markdown",
                  "persistent", "session"] {
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
            prelude,
        }
    }

    fn err(&mut self, msg: impl Into<String>) {
        self.errors.push(TypeError::new(msg));
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
            .map(|p| (p.name.clone(), self.resolve_type(&p.ty)))
            .collect();
        let return_type = t
            .return_type
            .as_ref()
            .map(|ty| self.resolve_type(ty))
            .unwrap_or(Ty::None_);
        TaskSig { params, return_type }
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
        AgentInfo { state_fields, tasks, handlers }
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
                    } else if let Some(t) = self.aliases.get(n) {
                        t.clone()
                    } else {
                        // Struct, unresolved, or user type — best effort.
                        Ty::Unknown
                    }
                }
            },
            TypeExpr::Nullable(inner) => Ty::Nullable(Box::new(self.resolve_type(inner))),
            TypeExpr::List(inner) => Ty::List(Box::new(self.resolve_type(inner))),
            TypeExpr::Map(k, v) => Ty::Map(Box::new(self.resolve_type(k)), Box::new(self.resolve_type(v))),
            TypeExpr::Set(inner) => Ty::Set(Box::new(self.resolve_type(inner))),
            TypeExpr::Struct(fields) => Ty::Struct(
                fields.iter().map(|f| (f.name.clone(), self.resolve_type(&f.ty))).collect(),
            ),
            TypeExpr::Tuple(items) => Ty::Tuple(items.iter().map(|t| self.resolve_type(t)).collect()),
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
                Decl::Stmt((stmt, _)) => {
                    let mut scope = Scope::new();
                    self.check_stmt(stmt, &mut scope);
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
        if let Some(agent_name) = &self.current_agent {
            if let Some(info) = self.agents.get(agent_name) {
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
        }
        scope
    }

    fn check_task(&mut self, t: &TaskDecl) {
        let mut scope = self.fresh_scope();
        for p in &t.params {
            scope.define(p.name.clone(), self.resolve_type(&p.ty));
        }
        self.check_block(&t.body, &mut scope);
    }

    fn check_on_handler(&mut self, h: &OnHandler) {
        let mut scope = self.fresh_scope();
        if let Some(p) = &h.param {
            scope.define(p.name.clone(), self.resolve_type(&p.ty));
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
        for (stmt, _) in block {
            self.check_stmt(stmt, scope);
        }
        scope.pop();
    }

    fn check_stmt(&mut self, stmt: &Stmt, scope: &mut Scope) {
        match stmt {
            Stmt::Let { name, ty, value } => {
                let inferred = self.infer_expr(value, scope);
                let bound = match ty {
                    Some(t) => self.resolve_type(t),
                    None => inferred,
                };
                scope.define(name.clone(), bound);
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
                    self.infer_expr(e, scope);
                }
            }
            Stmt::For { binding, iter, filter, body } => {
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
                scope.define(binding.clone(), element_ty);
                if let Some(pred) = filter {
                    let pty = self.infer_expr(pred, scope);
                    self.expect(&pty, &Ty::Bool, "for-where predicate");
                }
                for (s, _) in body {
                    self.check_stmt(s, scope);
                }
                scope.pop();
            }
            Stmt::If { cond, then_body, else_body } => {
                let cond_ty = self.infer_expr(cond, scope);
                self.expect(&cond_ty, &Ty::Bool, "`if` condition");
                self.check_block(then_body, scope);
                if let Some(eb) = else_body {
                    self.check_block(eb, scope);
                }
            }
            Stmt::When { subject, arms } => {
                let subject_ty = self.infer_expr(subject, scope);
                self.check_when_arms(&subject_ty, arms, scope);
            }
            Stmt::TryCatch { body, catches } => {
                self.check_block(body, scope);
                for c in catches {
                    scope.push();
                    let ty = self.resolve_type(&c.ty);
                    scope.define(c.name.clone(), ty);
                    for (s, _) in &c.body {
                        self.check_stmt(s, scope);
                    }
                    scope.pop();
                }
            }
            Stmt::Expr(e) => {
                self.infer_expr(e, scope);
            }
        }
    }

    fn check_when_arms(&mut self, subject_ty: &Ty, arms: &[WhenArm], scope: &mut Scope) {
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
            for (s, _) in &arm.body {
                self.check_stmt(s, scope);
            }
            scope.pop();
        }

        // Exhaustiveness
        match subject_ty.strip_nullable() {
            Ty::Enum(name) => {
                if has_wildcard { return; }
                if let Some(variants) = self.enum_variants.get(name) {
                    let missing: Vec<&String> = variants.iter().filter(|v| !covered.contains(*v)).collect();
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
            Expr::Now => Ty::Datetime,

            Expr::StringLit(parts) => {
                for p in parts {
                    if let StringPart::Interpolation(e) = p {
                        self.infer_expr(e, scope);
                    }
                }
                Ty::Str
            }

            Expr::Ident(name) => {
                if let Some(t) = scope.get(name) { return t.clone(); }
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
                    Ty::Struct(fields) => {
                        fields.iter().find(|(n, _)| n == field).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown)
                    }
                    _ => Ty::Unknown,
                }
            }

            Expr::NullFieldAccess(obj, _) => {
                let _ = self.infer_expr(obj, scope);
                Ty::Unknown
            }

            Expr::NullAssert(e) => self.infer_expr(e, scope),

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
                    if i == 0 { element_ty = ty; }
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

            Expr::Pipeline(l, r) => {
                let _ = self.infer_expr(l, scope);
                self.infer_expr(r, scope)
            }

            Expr::Call { callee, args } => {
                for a in args { self.infer_expr(&a.value, scope); }
                if let Expr::Ident(name) = callee.as_ref() {
                    if let Some(sig) = self.top_tasks.get(name).cloned() {
                        let expected = sig.params.len();
                        // Count only positional args (named args may map to params by name).
                        let positional: usize = args.iter().filter(|a| a.name.is_none()).count();
                        if positional > expected {
                            self.err(format!(
                                "task `{name}` takes {expected} argument(s), got {positional}"
                            ));
                        }
                        return sig.return_type.clone();
                    }
                }
                let _ = self.infer_expr(callee, scope);
                Ty::Unknown
            }

            Expr::MethodCall { object, method, args } => {
                for a in args { self.infer_expr(&a.value, scope); }
                // Special cases for inferring Ai.classify → Enum(T)
                if let Expr::Ident(name) = object.as_ref() {
                    if name == "Ai" && method == "classify" {
                        if let Some(as_arg) = args.iter().find(|a| a.name.as_deref() == Some("as")) {
                            if let Expr::Ident(enum_name) = &as_arg.value {
                                if self.enum_variants.contains_key(enum_name) {
                                    let base = Ty::Enum(enum_name.clone());
                                    return if args.iter().any(|a| a.name.as_deref() == Some("fallback")) {
                                        base
                                    } else {
                                        Ty::Nullable(Box::new(base))
                                    };
                                }
                            }
                        }
                    }
                    if name == "Ai" {
                        match method.as_str() {
                            "draft" | "summarize" | "translate" | "prompt" => return Ty::Nullable(Box::new(Ty::Str)),
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
                }
                let _ = self.infer_expr(object, scope);
                Ty::Unknown
            }

            Expr::Cast { expr, ty } => {
                self.infer_expr(expr, scope);
                self.resolve_type(ty)
            }

            Expr::IfExpr { cond, then_body, else_body } => {
                let c = self.infer_expr(cond, scope);
                self.expect(&c, &Ty::Bool, "`if` condition");
                let then_ty = self.block_type(then_body, scope);
                let _ = self.block_type(else_body, scope);
                then_ty
            }

            Expr::WhenExpr { subject, arms } => {
                let subject_ty = self.infer_expr(subject, scope);
                self.check_when_arms(&subject_ty, arms, scope);
                Ty::Unknown
            }

            Expr::Lambda { params, body } => {
                scope.push();
                for p in params {
                    let ty = p.ty.as_ref().map(|t| self.resolve_type(t)).unwrap_or(Ty::Unknown);
                    scope.define(p.name.clone(), ty);
                }
                let ret = match body {
                    LambdaBody::Expr(e) => self.infer_expr(e, scope),
                    LambdaBody::Block(b) => {
                        for (s, _) in b { self.check_stmt(s, scope); }
                        Ty::Unknown
                    }
                };
                scope.pop();
                Ty::Func(
                    params.iter().map(|p| {
                        p.ty.as_ref().map(|t| self.resolve_type(t)).unwrap_or(Ty::Unknown)
                    }).collect(),
                    Box::new(ret),
                )
            }

            Expr::Duration { value, .. } => {
                self.infer_expr(value, scope);
                Ty::Duration
            }

            Expr::EnumVariant(name, variant) => {
                if let Some(variants) = self.enum_variants.get(name) {
                    if !variants.contains(variant) {
                        self.err(format!("enum `{name}` has no variant `{variant}`"));
                    }
                }
                Ty::Enum(name.clone())
            }
        }
    }

    fn block_type(&mut self, block: &Block, scope: &mut Scope) -> Ty {
        scope.push();
        let mut last = Ty::None_;
        for (stmt, _) in block {
            last = match stmt {
                Stmt::Expr(e) => self.infer_expr(e, scope),
                other => {
                    self.check_stmt(other, scope);
                    Ty::None_
                }
            };
        }
        scope.pop();
        last
    }

    fn expect(&mut self, actual: &Ty, expected: &Ty, context: &str) {
        if matches!(actual, Ty::Unknown | Ty::Dynamic) { return; }
        let actual_stripped = actual.strip_nullable();
        if actual_stripped != expected && !matches!(actual_stripped, Ty::Unknown | Ty::Dynamic) {
            self.err(format!(
                "{context}: expected {}, got {}",
                describe_ty(expected),
                describe_ty(actual)
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
        Add | Sub | Mul | Div | Mod => {
            match (lb, rb) {
                (Ty::Int, Ty::Int) => Ty::Int,
                (Ty::Float, Ty::Float) => Ty::Float,
                (Ty::Float, Ty::Int) | (Ty::Int, Ty::Float) => Ty::Float,
                (Ty::Str, Ty::Str) if matches!(op, Add) => Ty::Str,
                _ => Ty::Unknown,
            }
        }
        Eq | Neq | Lt | Gt | Lte | Gte => Ty::Bool,
        And | Or => Ty::Bool,
    }
}
