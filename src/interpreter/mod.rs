//! Tree-walking interpreter for Keel v0.1.
//!
//! Evaluates programs against the new AST: agent bodies with
//! `@attribute` clauses, namespace-dispatched calls (`Ai.classify`,
//! `Io.notify`, `Schedule.every`, …) resolved through the runtime
//! prelude, and structured `self.` state mutation.
//!
//! This is a deliberately compact v0.1 implementation — enough to run
//! the `.keel` examples end-to-end. The type checker, formatter and
//! VM remain stubbed until a follow-up commit.

pub mod environment;
pub mod value;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use miette::{NamedSource, Result};

use crate::ast::*;
use environment::Environment;
use value::Value;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

fn runtime_error(msg: impl Into<String>) -> miette::Report {
    miette::miette!("{}", msg.into())
}

// ---------------------------------------------------------------------------
// Runtime structures
// ---------------------------------------------------------------------------

pub type BuiltinFn = Arc<
    dyn for<'a> Fn(
            &'a mut Interpreter,
            Vec<CallArgValue>,
        ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct CallArgValue {
    pub name: Option<String>,
    pub value: Value,
}

/// A prelude namespace (e.g. `Io`, `Schedule`) — a map of method name
/// to implementation. The interpreter resolves `Ns.method(...)` by
/// looking up the namespace in the root scope, then the method in its
/// map.
#[derive(Clone)]
pub struct Namespace {
    pub name: String,
    pub methods: HashMap<String, BuiltinFn>,
}

#[derive(Clone)]
pub struct AgentDef {
    pub name: String,
    pub attributes: Vec<AttributeDecl>,
    pub state_fields: Vec<StateField>,
    pub tasks: Vec<TaskDecl>,
    pub handlers: Vec<OnHandler>,
}

/// Live agent instance — state + a reference to its declaration.
pub struct AgentInstance {
    pub def: AgentDef,
    pub state: HashMap<String, Value>,
}

// ---------------------------------------------------------------------------
// Interpreter state
// ---------------------------------------------------------------------------

pub struct Interpreter {
    /// Top-level name → value (types, tasks, agent defs, namespaces).
    pub globals: HashMap<String, Value>,
    /// Agent definitions available to `run(...)`.
    pub agents: HashMap<String, AgentDef>,
    /// Live agent instances started via `run()`, keyed by agent name.
    pub live_agents: Arc<Mutex<HashMap<String, Arc<Mutex<AgentInstance>>>>>,
    /// Currently-executing agent (for `self.` access inside tasks).
    pub current_agent: Option<Arc<Mutex<AgentInstance>>>,
    /// Prelude namespaces installed at startup.
    pub namespaces: HashMap<String, Namespace>,
    /// Source for diagnostics (optional).
    pub source: Option<NamedSource<String>>,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Interpreter {
            globals: HashMap::new(),
            agents: HashMap::new(),
            live_agents: Arc::new(Mutex::new(HashMap::new())),
            current_agent: None,
            namespaces: HashMap::new(),
            source: None,
        };
        crate::runtime::install_prelude(&mut interp);
        interp
    }

    /// Register a namespace (called by runtime::install_prelude).
    pub fn register_namespace(&mut self, ns: Namespace) {
        let name = ns.name.clone();
        self.globals
            .insert(name.clone(), Value::Namespace(name.clone()));
        self.namespaces.insert(name, ns);
    }

    /// Register a top-level function (e.g. `run`, `stop`). Top-level
    /// functions are stored as namespace `__global`'s methods and
    /// exposed as bare names via a thin Value wrapper.
    pub fn register_top_fn(&mut self, name: &str, f: BuiltinFn) {
        self.globals
            .insert(name.to_string(), Value::BuiltinFn(name.to_string()));
        self.namespaces
            .entry("__global".to_string())
            .or_insert_with(|| Namespace {
                name: "__global".to_string(),
                methods: HashMap::new(),
            })
            .methods
            .insert(name.to_string(), f);
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

pub async fn run_with_source(program: Program, source: Option<NamedSource<String>>) -> Result<()> {
    let mut interp = Interpreter::new();
    interp.source = source;
    interp.execute(program).await
}

impl Interpreter {
    pub async fn execute(&mut self, program: Program) -> Result<()> {
        // Two-pass: register all declarations first, then execute statements.
        for (decl, _span) in &program.declarations {
            self.register_decl(decl)?;
        }
        for (decl, _span) in &program.declarations {
            if let Decl::Stmt((stmt, _)) = decl {
                let mut env = Environment::new();
                self.exec_stmt(stmt, &mut env).await?;
            }
        }
        // If any agents are still live (started with run), park until
        // Ctrl+C. For the MVP we simply return — the installed agent
        // handlers drive execution via tokio tasks.
        loop {
            let live = self.live_agents.lock().unwrap().len();
            if live == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            // Break after a while so tests and one-shot programs exit.
            // Real agent lifetime management is a follow-up.
            if std::env::var("KEEL_ONESHOT").is_ok() {
                break;
            }
        }
        Ok(())
    }

    fn register_decl(&mut self, decl: &Decl) -> Result<()> {
        match decl {
            Decl::Type(t) => {
                // Bind the type name as a Namespace-like value so that
                // `Mood.neutral` resolves and `as: Mood` finds a
                // defined identifier. The runtime doesn't yet use type
                // info — the type checker will.
                self.globals.insert(t.name.clone(), Value::Namespace(t.name.clone()));
                Ok(())
            }
            Decl::Interface(_) | Decl::Extern(_) | Decl::Use(_) => Ok(()),
            Decl::Task(t) => {
                self.globals.insert(t.name.clone(), Value::Task(t.name.clone(), t.clone()));
                Ok(())
            }
            Decl::Agent(a) => {
                let def = AgentDef {
                    name: a.name.clone(),
                    attributes: a.items.iter().filter_map(|it| match it {
                        AgentItem::Attribute(attr) => Some(attr.clone()),
                        _ => None,
                    }).collect(),
                    state_fields: a.items.iter().filter_map(|it| match it {
                        AgentItem::State(fields) => Some(fields.clone()),
                        _ => None,
                    }).flatten().collect(),
                    tasks: a.items.iter().filter_map(|it| match it {
                        AgentItem::Task(t) => Some(t.clone()),
                        _ => None,
                    }).collect(),
                    handlers: a.items.iter().filter_map(|it| match it {
                        AgentItem::On(h) => Some(h.clone()),
                        _ => None,
                    }).collect(),
                };
                self.globals.insert(a.name.clone(), Value::AgentRef(a.name.clone()));
                self.agents.insert(a.name.clone(), def);
                Ok(())
            }
            Decl::Stmt(_) => Ok(()), // executed in pass 2
        }
    }
}

// ---------------------------------------------------------------------------
// Statement evaluation
// ---------------------------------------------------------------------------

impl Interpreter {
    pub fn exec_stmt<'a>(
        &'a mut self,
        stmt: &'a Stmt,
        env: &'a mut Environment,
    ) -> Pin<Box<dyn Future<Output = Result<StmtOutcome>> + Send + 'a>> {
        Box::pin(async move {
            match stmt {
                Stmt::Let { name, value, .. } => {
                    let v = self.eval_expr(value, env).await?;
                    env.define(name.clone(), v);
                    Ok(StmtOutcome::Normal)
                }
                Stmt::SelfAssign { field, value } => {
                    let v = self.eval_expr(value, env).await?;
                    if let Some(agent) = &self.current_agent {
                        agent.lock().unwrap().state.insert(field.clone(), v);
                        Ok(StmtOutcome::Normal)
                    } else {
                        Err(runtime_error(format!(
                            "`self.{field}` used outside an agent"
                        )))
                    }
                }
                Stmt::Expr(e) => {
                    let v = self.eval_expr(e, env).await?;
                    Ok(StmtOutcome::Value(v))
                }
                Stmt::Return(opt) => {
                    let v = match opt {
                        Some(e) => self.eval_expr(e, env).await?,
                        None => Value::None,
                    };
                    Ok(StmtOutcome::Return(v))
                }
                Stmt::For { binding, iter, filter, body } => {
                    let iter_v = self.eval_expr(iter, env).await?;
                    let items = match iter_v {
                        Value::List(items) => items,
                        other => return Err(runtime_error(format!(
                            "`for` expects a list, got {}",
                            other.type_name()
                        ))),
                    };
                    for item in items {
                        env.push_scope();
                        env.define(binding.clone(), item);
                        if let Some(pred) = filter {
                            let matched = self.eval_expr(pred, env).await?.is_truthy();
                            if !matched {
                                env.pop_scope();
                                continue;
                            }
                        }
                        match self.exec_block(body, env).await? {
                            StmtOutcome::Return(v) => {
                                env.pop_scope();
                                return Ok(StmtOutcome::Return(v));
                            }
                            _ => {}
                        }
                        env.pop_scope();
                    }
                    Ok(StmtOutcome::Normal)
                }
                Stmt::If { cond, then_body, else_body } => {
                    let c = self.eval_expr(cond, env).await?;
                    if c.is_truthy() {
                        self.exec_block(then_body, env).await
                    } else if let Some(eb) = else_body {
                        self.exec_block(eb, env).await
                    } else {
                        Ok(StmtOutcome::Normal)
                    }
                }
                Stmt::When { subject, arms } => {
                    let s = self.eval_expr(subject, env).await?;
                    for arm in arms {
                        if let Some(bindings) = self.match_patterns(&arm.patterns, &s) {
                            env.push_scope();
                            for (k, v) in bindings {
                                env.define(k, v);
                            }
                            if let Some(guard) = &arm.guard {
                                if !self.eval_expr(guard, env).await?.is_truthy() {
                                    env.pop_scope();
                                    continue;
                                }
                            }
                            let out = self.exec_block(&arm.body, env).await?;
                            env.pop_scope();
                            return Ok(out);
                        }
                    }
                    Ok(StmtOutcome::Normal)
                }
                Stmt::TryCatch { body, catches: _ } => {
                    // v0.1: catch clauses not fully wired yet. Execute
                    // body; propagate any error for now.
                    self.exec_block(body, env).await
                }
            }
        })
    }

    async fn exec_block(&mut self, block: &Block, env: &mut Environment) -> Result<StmtOutcome> {
        let mut last = Value::None;
        for (stmt, _) in block {
            match self.exec_stmt(stmt, env).await? {
                StmtOutcome::Return(v) => return Ok(StmtOutcome::Return(v)),
                StmtOutcome::Value(v) => last = v,
                StmtOutcome::Normal => last = Value::None,
            }
        }
        Ok(StmtOutcome::Value(last))
    }

    fn match_patterns(&self, patterns: &[Pattern], value: &Value) -> Option<Vec<(String, Value)>> {
        for p in patterns {
            if let Some(b) = self.match_pattern(p, value) {
                return Some(b);
            }
        }
        None
    }

    fn match_pattern(&self, pattern: &Pattern, value: &Value) -> Option<Vec<(String, Value)>> {
        match pattern {
            Pattern::Wildcard => Some(vec![]),
            Pattern::Ident(name) => {
                // Matches an enum variant by name (e.g. `low`, `high`).
                if let Value::EnumVariant(_, variant) = value {
                    if variant == name {
                        return Some(vec![]);
                    }
                }
                None
            }
            Pattern::Literal(e) => {
                let lit = match e {
                    Expr::Integer(n) => Value::Integer(*n),
                    Expr::Float(f) => Value::Float(*f),
                    Expr::StringLit(parts) => {
                        // Only literal-only strings match here.
                        let mut s = String::new();
                        for p in parts {
                            if let StringPart::Literal(t) = p {
                                s.push_str(t);
                            } else {
                                return None;
                            }
                        }
                        Value::String(s)
                    }
                    Expr::Bool(b) => Value::Bool(*b),
                    _ => return None,
                };
                if &lit == value {
                    Some(vec![])
                } else {
                    None
                }
            }
            Pattern::Variant { name, bindings } => {
                if let Value::EnumVariant(_ty, variant) = value {
                    if variant == name {
                        // v0.1: rich enum variant values aren't yet stored
                        // with their fields on the Value side. Return
                        // empty bindings; full destructure lands later.
                        return Some(bindings.iter().map(|b| (b.clone(), Value::None)).collect());
                    }
                }
                None
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum StmtOutcome {
    /// Statement executed; no value produced (e.g. a Let).
    Normal,
    /// Expression statement produced a value.
    Value(Value),
    /// `return` reached — propagate to enclosing task.
    Return(Value),
}

// ---------------------------------------------------------------------------
// Expression evaluation
// ---------------------------------------------------------------------------

impl Interpreter {
    pub fn eval_expr<'a>(
        &'a mut self,
        expr: &'a Expr,
        env: &'a mut Environment,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
        Box::pin(async move {
            match expr {
                Expr::Integer(n) => Ok(Value::Integer(*n)),
                Expr::Float(f) => Ok(Value::Float(*f)),
                Expr::Bool(b) => Ok(Value::Bool(*b)),
                Expr::None_ => Ok(Value::None),
                Expr::Now => Ok(Value::String(chrono_now_iso())),

                Expr::StringLit(parts) => {
                    let mut out = String::new();
                    for p in parts {
                        match p {
                            StringPart::Literal(s) => out.push_str(s),
                            StringPart::Interpolation(e) => {
                                let v = self.eval_expr(e, env).await?;
                                out.push_str(&v.as_string());
                            }
                        }
                    }
                    Ok(Value::String(out))
                }

                Expr::Ident(name) => self.lookup_ident(name, env),

                Expr::SelfAccess(field) => {
                    if let Some(agent) = &self.current_agent {
                        let inst = agent.lock().unwrap();
                        inst.state.get(field).cloned().ok_or_else(|| {
                            runtime_error(format!("Agent has no state field `{field}`"))
                        })
                    } else {
                        Err(runtime_error(format!("`self.{field}` used outside an agent")))
                    }
                }

                Expr::FieldAccess(obj, field) => {
                    // Enum variant access: `Urgency.medium`. If `obj` is
                    // a bare identifier naming a registered type, produce
                    // an EnumVariant directly (don't evaluate `obj`, which
                    // might not be bound as a Value).
                    if let Expr::Ident(name) = obj.as_ref() {
                        if self.agents.get(name).is_none()
                            && self.globals.get(name).map_or(true, |v| matches!(v, Value::Namespace(_)))
                            && is_pascal_case(name)
                        {
                            return Ok(Value::EnumVariant(name.clone(), field.clone()));
                        }
                    }
                    let obj_v = self.eval_expr(obj, env).await?;
                    match &obj_v {
                        Value::Namespace(ns_name) => {
                            Ok(Value::EnumVariant(ns_name.clone(), field.clone()))
                        }
                        Value::Map(m) => {
                            if let Some(v) = m.get(field) {
                                return Ok(v.clone());
                            }
                            // Fall through to property-style method call.
                            let out = self.call_method_on_value(obj_v.clone(), field, vec![], env).await;
                            out.map_err(|_| runtime_error(format!("Map has no field `{field}`")))
                        }
                        _ => {
                            // Zero-arg method fallback for properties
                            // like `.count`, `.length`, `.is_empty`.
                            self.call_method_on_value(obj_v.clone(), field, vec![], env).await
                                .map_err(|_| runtime_error(format!(
                                    "Cannot access `.{field}` on {}",
                                    obj_v.type_name()
                                )))
                        }
                    }
                }

                Expr::NullFieldAccess(obj, field) => {
                    let obj_v = self.eval_expr(obj, env).await?;
                    if matches!(obj_v, Value::None) {
                        Ok(Value::None)
                    } else {
                        let field_access = Expr::FieldAccess(obj.clone(), field.clone());
                        self.eval_expr(&field_access, env).await
                    }
                }

                Expr::NullAssert(e) => {
                    let v = self.eval_expr(e, env).await?;
                    if matches!(v, Value::None) {
                        Err(runtime_error("NullError: `!.` on none"))
                    } else {
                        Ok(v)
                    }
                }

                Expr::StructLit(fields) => {
                    let mut m = HashMap::new();
                    for (k, v) in fields {
                        let val = self.eval_expr(v, env).await?;
                        m.insert(k.clone(), val);
                    }
                    Ok(Value::Map(m))
                }

                Expr::ListLit(items) => {
                    let mut out = Vec::with_capacity(items.len());
                    for it in items {
                        out.push(self.eval_expr(it, env).await?);
                    }
                    Ok(Value::List(out))
                }

                Expr::SetLit(items) => {
                    let mut out = Vec::with_capacity(items.len());
                    for it in items {
                        out.push(self.eval_expr(it, env).await?);
                    }
                    Ok(Value::List(out)) // v0.1: sets share list repr
                }

                Expr::TupleLit(items) => {
                    let mut out = Vec::with_capacity(items.len());
                    for it in items {
                        out.push(self.eval_expr(it, env).await?);
                    }
                    Ok(Value::List(out))
                }

                Expr::BinaryOp { left, op, right } => {
                    let l = self.eval_expr(left, env).await?;
                    let r = self.eval_expr(right, env).await?;
                    eval_binary(*op, l, r)
                }

                Expr::UnaryOp { op, expr: inner } => {
                    let v = self.eval_expr(inner, env).await?;
                    match op {
                        UnOp::Neg => match v {
                            Value::Integer(n) => Ok(Value::Integer(-n)),
                            Value::Float(f) => Ok(Value::Float(-f)),
                            other => Err(runtime_error(format!(
                                "Cannot negate {}", other.type_name()
                            ))),
                        },
                        UnOp::Not => Ok(Value::Bool(!v.is_truthy())),
                    }
                }

                Expr::NullCoalesce(left, right) => {
                    let l = self.eval_expr(left, env).await?;
                    if matches!(l, Value::None) {
                        self.eval_expr(right, env).await
                    } else {
                        Ok(l)
                    }
                }

                Expr::Pipeline(left, right) => {
                    // `x |> f` ≡ `f(x)` (single positional argument)
                    let l = self.eval_expr(left, env).await?;
                    let args = vec![CallArgValue { name: None, value: l }];
                    self.call_value(right, args, env).await
                }

                Expr::Call { callee, args } => {
                    let arg_values = self.eval_args(args, env).await?;
                    self.call_value(callee, arg_values, env).await
                }

                Expr::MethodCall { object, method, args } => {
                    let arg_values = self.eval_args(args, env).await?;
                    // If object is a namespace, dispatch to its method.
                    let obj_val = self.eval_expr(object, env).await?;
                    if let Value::Namespace(ns) = &obj_val {
                        let ns_name = ns.clone();
                        return self.call_namespace_method(&ns_name, method, arg_values).await;
                    }
                    // AgentRef.task(...) — cross-agent task call
                    if let Value::AgentRef(name) = &obj_val {
                        return self.call_agent_task(name, method, arg_values).await;
                    }
                    // Otherwise: method on a value (e.g., list.map).
                    self.call_method_on_value(obj_val, method, arg_values, env).await
                }

                Expr::Cast { expr: inner, ty: _ } => {
                    // v0.1: casts are runtime-checked elsewhere; here we
                    // just evaluate the inner expression.
                    self.eval_expr(inner, env).await
                }

                Expr::IfExpr { cond, then_body, else_body } => {
                    let c = self.eval_expr(cond, env).await?;
                    if c.is_truthy() {
                        match self.exec_block(then_body, env).await? {
                            StmtOutcome::Value(v) | StmtOutcome::Return(v) => Ok(v),
                            StmtOutcome::Normal => Ok(Value::None),
                        }
                    } else {
                        match self.exec_block(else_body, env).await? {
                            StmtOutcome::Value(v) | StmtOutcome::Return(v) => Ok(v),
                            StmtOutcome::Normal => Ok(Value::None),
                        }
                    }
                }

                Expr::WhenExpr { subject, arms } => {
                    let s = self.eval_expr(subject, env).await?;
                    for arm in arms {
                        if let Some(bindings) = self.match_patterns(&arm.patterns, &s) {
                            env.push_scope();
                            for (k, v) in bindings {
                                env.define(k, v);
                            }
                            if let Some(g) = &arm.guard {
                                if !self.eval_expr(g, env).await?.is_truthy() {
                                    env.pop_scope();
                                    continue;
                                }
                            }
                            let result = match self.exec_block(&arm.body, env).await? {
                                StmtOutcome::Value(v) | StmtOutcome::Return(v) => v,
                                StmtOutcome::Normal => Value::None,
                            };
                            env.pop_scope();
                            return Ok(result);
                        }
                    }
                    Ok(Value::None)
                }

                Expr::Lambda { params, body } => Ok(Value::Closure(params.clone(), body.clone())),

                Expr::Duration { value, unit } => {
                    let v = self.eval_expr(value, env).await?;
                    let n = v.as_int().ok_or_else(|| runtime_error("duration value must be int"))?;
                    Ok(Value::Duration(Value::duration_seconds(n, *unit)))
                }

                Expr::EnumVariant(ty, variant) => Ok(Value::EnumVariant(ty.clone(), variant.clone())),
            }
        })
    }

    fn lookup_ident(&self, name: &str, env: &Environment) -> Result<Value> {
        if let Some(v) = env.get(name) {
            return Ok(v.clone());
        }
        if let Some(v) = self.globals.get(name) {
            return Ok(v.clone());
        }
        // Agent-scoped tasks (resolvable only while current_agent is set).
        if let Some(agent) = &self.current_agent {
            let def = agent.lock().unwrap().def.clone();
            if let Some(task) = def.tasks.iter().find(|t| t.name == name) {
                return Ok(Value::Task(task.name.clone(), task.clone()));
            }
        }
        Err(runtime_error(format!("Undefined: `{name}`")))
    }

    async fn eval_args(&mut self, args: &[CallArg], env: &mut Environment) -> Result<Vec<CallArgValue>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let v = self.eval_expr(&a.value, env).await?;
            out.push(CallArgValue { name: a.name.clone(), value: v });
        }
        Ok(out)
    }

    fn call_value<'a>(
        &'a mut self,
        callee: &'a Expr,
        args: Vec<CallArgValue>,
        env: &'a mut Environment,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'a>> {
        Box::pin(async move {
            let callee_v = self.eval_expr(callee, env).await?;
            match callee_v {
                Value::Task(name, decl) => self.call_task(&name, &decl, args).await,
                Value::BuiltinFn(name) => self.call_namespace_method("__global", &name, args).await,
                Value::Namespace(_) => Err(runtime_error("Cannot call a namespace directly")),
                Value::Closure(params, body) => self.call_closure(&params, &body, args).await,
                other => Err(runtime_error(format!("Cannot call {}", other.type_name()))),
            }
        })
    }

    pub async fn call_closure(
        &mut self,
        params: &[LambdaParam],
        body: &LambdaBody,
        args: Vec<CallArgValue>,
    ) -> Result<Value> {
        let mut env = Environment::new();
        for (i, p) in params.iter().enumerate() {
            let v = args.get(i).map(|a| a.value.clone()).unwrap_or(Value::None);
            env.define(p.name.clone(), v);
        }
        match body {
            LambdaBody::Expr(e) => self.eval_expr(e, &mut env).await,
            LambdaBody::Block(block) => match self.exec_block(block, &mut env).await? {
                StmtOutcome::Value(v) | StmtOutcome::Return(v) => Ok(v),
                StmtOutcome::Normal => Ok(Value::None),
            },
        }
    }

    async fn call_task(&mut self, _name: &str, decl: &TaskDecl, args: Vec<CallArgValue>) -> Result<Value> {
        let mut env = Environment::new();
        // Bind params by position (named args not wired for user tasks yet).
        for (i, p) in decl.params.iter().enumerate() {
            let v = args.get(i).map(|a| a.value.clone()).unwrap_or(Value::None);
            env.define(p.name.clone(), v);
        }
        match self.exec_block(&decl.body, &mut env).await? {
            StmtOutcome::Value(v) | StmtOutcome::Return(v) => Ok(v),
            StmtOutcome::Normal => Ok(Value::None),
        }
    }

    async fn call_namespace_method(
        &mut self,
        ns_name: &str,
        method: &str,
        args: Vec<CallArgValue>,
    ) -> Result<Value> {
        let f = {
            let ns = self.namespaces.get(ns_name).ok_or_else(|| {
                runtime_error(format!("Unknown namespace: `{ns_name}`"))
            })?;
            ns.methods.get(method).cloned().ok_or_else(|| {
                runtime_error(format!("Namespace `{ns_name}` has no method `{method}`"))
            })?
        };
        f(self, args).await
    }

    async fn call_method_on_value(
        &mut self,
        obj: Value,
        method: &str,
        args: Vec<CallArgValue>,
        _env: &mut Environment,
    ) -> Result<Value> {
        // Minimal built-in methods for v0.1. Extend as examples need.
        match (&obj, method) {
            (Value::String(s), "length") => Ok(Value::Integer(s.chars().count() as i64)),
            (Value::String(s), "is_empty") => Ok(Value::Bool(s.is_empty())),
            (Value::String(s), "to_str") => Ok(Value::String(s.clone())),
            (Value::String(s), "contains") => {
                let needle = args.first().map(|a| a.value.as_string()).unwrap_or_default();
                Ok(Value::Bool(s.contains(&needle)))
            }
            (Value::List(items), "count") => Ok(Value::Integer(items.len() as i64)),
            (Value::List(items), "is_empty") => Ok(Value::Bool(items.is_empty())),
            (Value::List(items), "first") => Ok(items.first().cloned().unwrap_or(Value::None)),
            (Value::List(items), "last") => Ok(items.last().cloned().unwrap_or(Value::None)),
            (Value::List(items), "map") => {
                let closure = args.first().map(|a| a.value.clone()).ok_or_else(|| {
                    runtime_error("map expects a function argument")
                })?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("map argument must be a function")),
                };
                let mut out = Vec::with_capacity(items.len());
                for item in items.clone() {
                    let res = self.call_closure(&params, &body, vec![CallArgValue { name: None, value: item }]).await?;
                    out.push(res);
                }
                Ok(Value::List(out))
            }
            (Value::List(items), "filter") => {
                let closure = args.first().map(|a| a.value.clone()).ok_or_else(|| {
                    runtime_error("filter expects a function argument")
                })?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("filter argument must be a function")),
                };
                let mut out = Vec::new();
                for item in items.clone() {
                    let res = self.call_closure(&params, &body, vec![CallArgValue { name: None, value: item.clone() }]).await?;
                    if res.is_truthy() {
                        out.push(item);
                    }
                }
                Ok(Value::List(out))
            }
            (Value::Integer(n), "to_str") => Ok(Value::String(n.to_string())),
            (Value::Float(f), "to_str") => Ok(Value::String(f.to_string())),
            (Value::Bool(b), "to_str") => Ok(Value::String(b.to_string())),
            (Value::EnumVariant(_, v), "to_str") => Ok(Value::String(v.clone())),
            _ => Err(runtime_error(format!(
                "Method `{method}` not available on {}",
                obj.type_name()
            ))),
        }
    }

    async fn call_agent_task(&mut self, agent_name: &str, task_name: &str, args: Vec<CallArgValue>) -> Result<Value> {
        let def = self.agents.get(agent_name).cloned().ok_or_else(|| {
            runtime_error(format!("Unknown agent: `{agent_name}`"))
        })?;
        let task = def.tasks.iter().find(|t| t.name == task_name).cloned().ok_or_else(|| {
            runtime_error(format!("Agent `{agent_name}` has no task `{task_name}`"))
        })?;
        self.call_task(task_name, &task, args).await
    }
}

// ---------------------------------------------------------------------------
// Agent lifecycle (used by Agent.run builtin)
// ---------------------------------------------------------------------------

impl Interpreter {
    pub async fn start_agent(&mut self, agent_name: &str) -> Result<()> {
        let def = self.agents.get(agent_name).cloned().ok_or_else(|| {
            runtime_error(format!("Unknown agent: `{agent_name}`"))
        })?;
        let mut state = HashMap::new();
        for f in &def.state_fields {
            let mut tmp_env = Environment::new();
            state.insert(f.name.clone(), self.eval_expr(&f.default, &mut tmp_env).await?);
        }
        let inst = Arc::new(Mutex::new(AgentInstance { def: def.clone(), state }));
        self.live_agents.lock().unwrap().insert(agent_name.to_string(), inst.clone());

        // Run @on_start block, if any.
        let on_start = def.attributes.iter().find(|a| a.name == "on_start").cloned();
        if let Some(attr) = on_start {
            if let AttributeBody::Block(body) = attr.body {
                let prev = self.current_agent.take();
                self.current_agent = Some(inst.clone());
                let mut env = Environment::new();
                self.exec_block(&body, &mut env).await?;
                self.current_agent = prev;
            }
        }
        Ok(())
    }

    pub async fn stop_agent(&mut self, agent_name: &str) -> Result<()> {
        self.live_agents.lock().unwrap().remove(agent_name);
        Ok(())
    }

    pub async fn run_agent_task(&mut self, agent_name: &str, task_name: &str, args: Vec<CallArgValue>) -> Result<Value> {
        let inst = self.live_agents.lock().unwrap().get(agent_name).cloned();
        let inst = match inst {
            Some(i) => i,
            None => return Err(runtime_error(format!("Agent `{agent_name}` is not running"))),
        };
        let def = inst.lock().unwrap().def.clone();
        let task = def.tasks.iter().find(|t| t.name == task_name).cloned().ok_or_else(|| {
            runtime_error(format!("Agent `{agent_name}` has no task `{task_name}`"))
        })?;
        let prev = self.current_agent.take();
        self.current_agent = Some(inst);
        let result = self.call_task(task_name, &task, args).await;
        self.current_agent = prev;
        result
    }
}

// ---------------------------------------------------------------------------
// Binary ops
// ---------------------------------------------------------------------------

fn eval_binary(op: BinOp, l: Value, r: Value) -> Result<Value> {
    use BinOp::*;
    match (op, &l, &r) {
        (Add, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
        (Sub, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
        (Mul, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
        (Div, Value::Integer(a), Value::Integer(b)) => {
            if *b == 0 { return Err(runtime_error("Division by zero")); }
            Ok(Value::Integer(a / b))
        }
        (Mod, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a % b)),
        (Add, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Sub, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (Mul, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (Div, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
        (Add, Value::String(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
        (Eq, a, b) => Ok(Value::Bool(a == b)),
        (Neq, a, b) => Ok(Value::Bool(a != b)),
        (Lt, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a < b)),
        (Gt, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a > b)),
        (Lte, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a <= b)),
        (Gte, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a >= b)),
        (And, a, b) => Ok(Value::Bool(a.is_truthy() && b.is_truthy())),
        (Or, a, b) => Ok(Value::Bool(a.is_truthy() || b.is_truthy())),
        _ => Err(runtime_error(format!(
            "Cannot apply `{:?}` to {} and {}",
            op, l.type_name(), r.type_name()
        ))),
    }
}

fn is_pascal_case(s: &str) -> bool {
    s.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
}

fn chrono_now_iso() -> String {
    // Deliberately primitive; the Time stdlib namespace will expose
    // proper datetime types later.
    format!("{:?}", std::time::SystemTime::now())
}
