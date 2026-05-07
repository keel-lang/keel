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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use miette::{NamedSource, Result};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

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

/// Runtime event posted to the interpreter's mailbox from tokio tasks
/// (schedulers, message dispatchers, …) and consumed by `execute`.
#[derive(Debug)]
pub enum Event {
    /// Fire a registered closure on behalf of an agent.
    FireClosure { agent_name: String, closure_id: u64 },
    /// Deliver a message to an agent's `on <event>` handler.
    Dispatch {
        agent_name: String,
        event: String,
        data: Value,
    },
    /// Fire a closure with arguments and return the result through a oneshot channel (JSON-encoded).
    FireClosureWithArgs {
        closure_id: u64,
        request_json: String,
        response_tx: tokio::sync::oneshot::Sender<String>,
    },
    /// Shut down the event loop.
    Shutdown,
}

/// A scheduled closure awaiting firing via an `Event::FireClosure`.
#[derive(Clone)]
pub struct ScheduledClosure {
    pub agent_name: String,
    pub params: Vec<LambdaParam>,
    pub body: LambdaBody,
}

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
    /// Simple-enum type name → variant names. Populated from `type X = a | b` declarations.
    pub enum_types: HashMap<String, Vec<String>>,
    /// Struct type name → field (name, type_string) pairs. Populated from `type T { f: ty }`.
    pub struct_types: HashMap<String, Vec<(String, String)>>,
    /// Shared Ollama client for `Ai.*` operations.
    pub llm: Arc<crate::runtime::llm::LlmClient>,
    /// Sender for runtime events. Spawned tokio tasks (scheduler,
    /// message dispatcher) clone this to post events to the main loop.
    pub event_tx: UnboundedSender<Event>,
    /// Receiver end of the event channel. Owned by the interpreter
    /// between `new()` and `execute()`, then moved into the event
    /// loop. `execute()` will panic if called twice.
    pub event_rx: Option<UnboundedReceiver<Event>>,
    /// Registered closures keyed by id. Scheduled tasks post the id
    /// via `Event::FireClosure`; the event loop looks up the closure
    /// and invokes it in the correct agent context.
    pub closures: HashMap<u64, ScheduledClosure>,
    /// Next free closure id.
    pub next_closure_id: u64,
    /// Number of active Http.serve listeners. The event loop keeps running
    /// while this is > 0, even if no agents are live.
    pub active_http_servers: Arc<AtomicU64>,
    /// Source for diagnostics (optional).
    pub source: Option<NamedSource<String>>,
    /// Memory namespace derived from the source file path via `derive_program_name`.
    /// Format: `<stem>_<sha256[:12]>` for real files; `__repl__` / `__inline__`
    /// for REPL and inline evaluations. Defaults to `"__inline__"`.
    pub program_name: String,
    /// Last typed error thrown via `throw_typed_error`. Used by `try/catch` to
    /// match catch clauses by type name. Cleared at the start of each `try` block.
    pub last_typed_error: Option<(String, HashMap<String, Value>)>,
}

impl Interpreter {
    pub fn new() -> Self {
        let (event_tx, event_rx) = unbounded_channel();
        let mut interp = Interpreter {
            globals: HashMap::new(),
            agents: HashMap::new(),
            live_agents: Arc::new(Mutex::new(HashMap::new())),
            current_agent: None,
            namespaces: HashMap::new(),
            enum_types: HashMap::new(),
            struct_types: HashMap::new(),
            llm: Arc::new(crate::runtime::llm::LlmClient::new()),
            event_tx,
            event_rx: Some(event_rx),
            closures: HashMap::new(),
            next_closure_id: 0,
            active_http_servers: Arc::new(AtomicU64::new(0)),
            source: None,
            program_name: "__inline__".to_string(),
            last_typed_error: None,
        };
        crate::runtime::install_prelude(&mut interp);
        interp
    }

    /// Register a closure for later firing via `Event::FireClosure`.
    /// Returns the id to embed in scheduled events.
    pub fn register_closure(
        &mut self,
        agent_name: String,
        params: Vec<LambdaParam>,
        body: LambdaBody,
    ) -> u64 {
        let id = self.next_closure_id;
        self.next_closure_id += 1;
        self.closures.insert(
            id,
            ScheduledClosure {
                agent_name,
                params,
                body,
            },
        );
        id
    }

    /// Fire a registered closure in the named agent's context.
    /// Called by the event loop when `Event::FireClosure` arrives.
    pub async fn fire_closure(&mut self, agent_name: &str, closure_id: u64) -> Result<()> {
        let closure = self.closures.get(&closure_id).cloned();
        let inst = self.live_agents.lock().unwrap().get(agent_name).cloned();
        let (Some(c), Some(agent_inst)) = (closure, inst) else {
            return Ok(()); // agent stopped or closure removed
        };
        let prev = self.current_agent.take();
        self.current_agent = Some(agent_inst);
        let result = self.call_closure(&c.params, &c.body, vec![]).await;
        self.current_agent = prev;
        result.map(|_| ())
    }

    /// Deliver an incoming event to the matching `on <event>` handler
    /// on the target agent. Silently no-ops if the agent has stopped
    /// or has no handler for this event — both are valid states.
    pub async fn dispatch_event(
        &mut self,
        agent_name: &str,
        event_name: &str,
        data: Value,
    ) -> Result<()> {
        let inst = self.live_agents.lock().unwrap().get(agent_name).cloned();
        let Some(agent_inst) = inst else {
            return Ok(());
        };
        let handler = agent_inst
            .lock()
            .unwrap()
            .def
            .handlers
            .iter()
            .find(|h| h.event == event_name)
            .cloned();
        let Some(handler) = handler else {
            return Ok(());
        };

        let prev = self.current_agent.take();
        self.current_agent = Some(agent_inst);
        let mut env = Environment::new();
        if let Some(p) = &handler.param {
            bind_value(&p.name, data, &mut env)?;
        }
        let result = self.exec_block(&handler.body, &mut env).await;
        self.current_agent = prev;
        result.map(|_| ())
    }

    /// The model to use for `Ai.*` operations when no explicit
    /// `using:` argument is given. Falls back to the current agent's
    /// `@model` attribute, then to `"default"` (which triggers the
    /// `KEEL_OLLAMA_MODEL` catch-all in the Ollama client).
    pub fn current_model(&self) -> String {
        self.agent_string_attr("model")
            .unwrap_or_else(|| "default".to_string())
    }

    /// The current agent's `@role "..."` string, if any. Used by the
    /// LLM client to prepend an agent-identity preamble to every
    /// system prompt — so `Ai.draft(...)` inside an agent with
    /// `@role "Professional email triage"` gets that directive on
    /// every call. Returns `None` when called outside any agent.
    pub fn current_role(&self) -> Option<String> {
        self.agent_string_attr("role")
    }

    /// The current agent's `@rules [...]` list, if any. Every string
    /// item in the list is injected as a bullet under "Rules:" in the
    /// system prompt of every `Ai.*` call. Returns an empty vec when
    /// called outside an agent or when `@rules` is absent.
    pub fn current_rules(&self) -> Vec<String> {
        let Some(agent) = self.current_agent.as_ref() else {
            return vec![];
        };
        let def = agent.lock().unwrap().def.clone();
        for attr in &def.attributes {
            if attr.name == "rules"
                && let AttributeBody::Expr(Expr::ListLit(items)) = &attr.body
            {
                return items
                    .iter()
                    .filter_map(|e| match e {
                        Expr::StringLit(parts) => {
                            let s: String = parts
                                .iter()
                                .filter_map(|p| match p {
                                    StringPart::Literal(s) => Some(s.clone()),
                                    _ => None,
                                })
                                .collect();
                            if s.is_empty() { None } else { Some(s) }
                        }
                        _ => None,
                    })
                    .collect();
            }
        }
        vec![]
    }

    /// The name of the currently executing agent.
    /// Returns `None` when called outside an agent context.
    pub fn current_agent_name(&self) -> Option<String> {
        self.current_agent
            .as_ref()
            .map(|a| a.lock().unwrap().def.name.clone())
    }

    /// Returns `(program_name, agent_name)` or an error if called outside
    /// an agent context. Use this for any operation that must be agent-scoped.
    pub fn require_agent_context(&self, caller: &str) -> miette::Result<(String, String)> {
        match self.current_agent_name() {
            Some(agent) => Ok((self.program_name.clone(), agent)),
            None => Err(miette::miette!(
                "{caller} requires an agent context — call it from inside an agent body"
            )),
        }
    }

    /// The value of the current agent's `@memory` attribute as a string
    /// (`"persistent"`, `"session"`, or `"none"`), or `None` when the
    /// attribute is absent (callers default to `"session"`).
    pub fn current_memory_attr(&self) -> Option<String> {
        let agent = self.current_agent.as_ref()?;
        let def = agent.lock().unwrap().def.clone();
        for attr in &def.attributes {
            if attr.name == "memory" {
                match &attr.body {
                    AttributeBody::Expr(Expr::Ident(mode)) => return Some(mode.clone()),
                    // `none` is a reserved keyword, so `@memory none` parses as Expr::None_
                    AttributeBody::Expr(Expr::None_) => return Some("none".to_string()),
                    _ => {}
                }
            }
        }
        None
    }

    fn agent_string_attr(&self, name: &str) -> Option<String> {
        let agent = self.current_agent.as_ref()?;
        let def = agent.lock().unwrap().def.clone();
        for attr in &def.attributes {
            if attr.name == name
                && let AttributeBody::Expr(Expr::StringLit(parts)) = &attr.body
            {
                let s: String = parts
                    .iter()
                    .filter_map(|p| match p {
                        StringPart::Literal(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        None
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

pub async fn run_with_source(
    program: Program,
    source: Option<NamedSource<String>>,
    source_path: Option<&std::path::Path>,
) -> Result<()> {
    let mut interp = Interpreter::new();
    if let Some(path) = source_path {
        let raw = path.to_str().unwrap_or("__inline__");
        interp.program_name = crate::runtime::derive_program_name(raw);
    }
    interp.source = source;
    interp.execute(program).await
}

impl Interpreter {
    pub async fn execute(&mut self, program: Program) -> Result<()> {
        // Two-pass: register all declarations, then execute top-level statements.
        for (decl, _span) in &program.declarations {
            self.register_decl(decl)?;
        }
        for (decl, _span) in &program.declarations {
            if let Decl::Stmt((stmt, _)) = decl {
                let mut env = Environment::new();
                self.exec_stmt(stmt, &mut env).await?;
            }
        }

        // Event loop: serve scheduled ticks, message dispatch, and
        // Ctrl+C. Terminates when no agents are live, when a shutdown
        // event is posted, or on Ctrl+C.
        //
        // `KEEL_ONESHOT=1` (integration tests) exits after a short
        // idle window with no events — this lets `@on_start`-only
        // agents finish cleanly without blocking on `rx.recv()`.
        let oneshot = std::env::var("KEEL_ONESHOT").is_ok();
        let idle_budget = std::time::Duration::from_millis(250);
        let mut rx = self
            .event_rx
            .take()
            .expect("Interpreter::execute called twice");

        loop {
            let no_agents = self.live_agents.lock().unwrap().is_empty();
            let no_servers = self.active_http_servers.load(Ordering::Relaxed) == 0;
            if no_agents && no_servers {
                break;
            }

            let ev = if oneshot {
                match tokio::time::timeout(idle_budget, rx.recv()).await {
                    Ok(Some(ev)) => ev,
                    _ => break, // idle timeout or channel closed
                }
            } else {
                tokio::select! {
                    biased;
                    _ = tokio::signal::ctrl_c() => break,
                    maybe_ev = rx.recv() => match maybe_ev {
                        Some(ev) => ev,
                        None => break,
                    },
                }
            };

            match ev {
                Event::FireClosure {
                    agent_name,
                    closure_id,
                } => {
                    self.fire_closure(&agent_name, closure_id).await?;
                }
                Event::Dispatch {
                    agent_name,
                    event,
                    data,
                } => {
                    self.dispatch_event(&agent_name, &event, data).await?;
                }
                Event::FireClosureWithArgs {
                    closure_id,
                    request_json,
                    response_tx,
                } => {
                    let closure = self.closures.get(&closure_id).cloned();
                    if let Some(c) = closure {
                        // Deserialize request JSON to Value
                        let request_val =
                            match serde_json::from_str::<serde_json::Value>(&request_json) {
                                Ok(jval) => crate::runtime::json_to_value(&jval),
                                Err(_) => Value::String(request_json.clone()),
                            };
                        let result = self
                            .call_closure(
                                &c.params,
                                &c.body,
                                vec![CallArgValue {
                                    name: None,
                                    value: request_val,
                                }],
                            )
                            .await;
                        // Serialize result back to JSON string
                        let resp_val = result.unwrap_or(Value::None);
                        let json_val = crate::runtime::value_to_json(&resp_val);
                        let resp_json = match serde_json::to_string(&json_val) {
                            Ok(s) => s,
                            Err(_) => r#"{"status":500,"body":"serialization failed"}"#.into(),
                        };
                        let _ = response_tx.send(resp_json);
                    } else {
                        let _ =
                            response_tx.send(r#"{"status":500,"body":"handler not found"}"#.into());
                    }
                }
                Event::Shutdown => break,
            }
        }
        Ok(())
    }

    fn register_decl(&mut self, decl: &Decl) -> Result<()> {
        match decl {
            Decl::Type(t) => {
                // Bind the type name as a Namespace-like value so that
                // `Mood.neutral` resolves and `as: Mood` finds a
                // defined identifier. For simple enums, also cache
                // the variant list for Ai.classify.
                self.globals
                    .insert(t.name.clone(), Value::Namespace(t.name.clone()));
                match &t.def {
                    TypeDef::SimpleEnum(variants) => {
                        self.enum_types.insert(t.name.clone(), variants.clone());
                    }
                    TypeDef::Struct(fields) => {
                        let schema = fields
                            .iter()
                            .map(|f| (f.name.clone(), type_expr_to_string(&f.ty)))
                            .collect();
                        self.struct_types.insert(t.name.clone(), schema);
                    }
                    _ => {}
                }
                Ok(())
            }
            Decl::Interface(_) | Decl::Extern(_) | Decl::Use(_) => Ok(()),
            Decl::Task(t) => {
                self.globals
                    .insert(t.name.clone(), Value::Task(t.name.clone(), t.clone()));
                Ok(())
            }
            Decl::Agent(a) => {
                let def = AgentDef {
                    name: a.name.clone(),
                    attributes: a
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            AgentItem::Attribute(attr) => Some(attr.clone()),
                            _ => None,
                        })
                        .collect(),
                    state_fields: a
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            AgentItem::State(fields) => Some(fields.clone()),
                            _ => None,
                        })
                        .flatten()
                        .collect(),
                    tasks: a
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            AgentItem::Task(t) => Some(t.clone()),
                            _ => None,
                        })
                        .collect(),
                    handlers: a
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            AgentItem::On(h) => Some(h.clone()),
                            _ => None,
                        })
                        .collect(),
                };
                self.globals
                    .insert(a.name.clone(), Value::AgentRef(a.name.clone()));
                self.agents.insert(a.name.clone(), def);
                Ok(())
            }
            Decl::Stmt(_) => Ok(()), // executed in pass 2
        }
    }
}

fn type_expr_to_string(te: &TypeExpr) -> String {
    match te {
        TypeExpr::Named(n) => n.clone(),
        TypeExpr::Nullable(inner) => format!("{}?", type_expr_to_string(inner)),
        TypeExpr::List(inner) => format!("[{}]", type_expr_to_string(inner)),
        TypeExpr::Map(k, v) => format!(
            "map[{}, {}]",
            type_expr_to_string(k),
            type_expr_to_string(v)
        ),
        TypeExpr::Set(inner) => format!("set[{}]", type_expr_to_string(inner)),
        TypeExpr::Tuple(items) => {
            let parts: Vec<_> = items.iter().map(type_expr_to_string).collect();
            format!("({})", parts.join(", "))
        }
        TypeExpr::Func(params, ret) => {
            let ps: Vec<_> = params.iter().map(type_expr_to_string).collect();
            format!("({}) -> {}", ps.join(", "), type_expr_to_string(ret))
        }
        TypeExpr::Generic(name, args) => {
            let as_: Vec<_> = args.iter().map(type_expr_to_string).collect();
            format!("{}[{}]", name, as_.join(", "))
        }
        TypeExpr::Struct(fields) => {
            let fs: Vec<_> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, type_expr_to_string(&f.ty)))
                .collect();
            format!("{{{}}}", fs.join(", "))
        }
        TypeExpr::Dynamic => "dynamic".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Statement evaluation helpers
// ---------------------------------------------------------------------------

fn bind_destructure(pat: &DestructPat, value: Value, env: &mut Environment) -> Result<()> {
    match pat {
        DestructPat::Struct(fields) => {
            let map = match value {
                Value::Map(m) => m,
                other => {
                    return Err(runtime_error(format!(
                        "cannot destructure {} as a struct",
                        other.type_name()
                    )));
                }
            };
            for (source, local) in fields {
                let v = map.get(source).cloned().unwrap_or(Value::None);
                env.define(local.clone(), v);
            }
        }
        DestructPat::Tuple(names) => {
            let items = match value {
                Value::List(items) => items,
                other => {
                    return Err(runtime_error(format!(
                        "cannot destructure {} as a tuple",
                        other.type_name()
                    )));
                }
            };
            for (i, name) in names.iter().enumerate() {
                let v = items.get(i).cloned().unwrap_or(Value::None);
                env.define(name.clone(), v);
            }
        }
    }
    Ok(())
}

fn bind_value(binding: &Binding, value: Value, env: &mut Environment) -> Result<()> {
    match binding {
        Binding::Ident(name) => {
            env.define(name.clone(), value);
            Ok(())
        }
        Binding::Destruct(pat) => bind_destructure(pat, value, env),
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
                Stmt::Let { binding, value, .. } => {
                    let v = self.eval_expr(value, env).await?;
                    bind_value(binding, v, env)?;
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
                Stmt::For {
                    binding,
                    iter,
                    filter,
                    body,
                } => {
                    let iter_v = self.eval_expr(iter, env).await?;
                    // Build an owned iterator without materializing a range.
                    enum ForIter {
                        List(std::vec::IntoIter<Value>),
                        Range(std::ops::RangeInclusive<i64>),
                    }
                    impl Iterator for ForIter {
                        type Item = Value;
                        fn next(&mut self) -> Option<Value> {
                            match self {
                                ForIter::List(it) => it.next(),
                                ForIter::Range(it) => it.next().map(Value::Integer),
                            }
                        }
                    }
                    let iter: ForIter = match iter_v {
                        Value::List(items) => ForIter::List(items.into_iter()),
                        Value::Range(lo, hi) => ForIter::Range(lo..=hi),
                        other => {
                            return Err(runtime_error(format!(
                                "`for` expects a list, got {}",
                                other.type_name()
                            )));
                        }
                    };
                    for item in iter {
                        env.push_scope();
                        bind_value(binding, item, env)?;
                        if let Some(pred) = filter {
                            let matched = self.eval_expr(pred, env).await?.is_truthy();
                            if !matched {
                                env.pop_scope();
                                continue;
                            }
                        }
                        if let StmtOutcome::Return(v) = self.exec_block(body, env).await? {
                            env.pop_scope();
                            return Ok(StmtOutcome::Return(v));
                        }
                        env.pop_scope();
                    }
                    Ok(StmtOutcome::Normal)
                }
                Stmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
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
                            if let Some(guard) = &arm.guard
                                && !self.eval_expr(guard, env).await?.is_truthy()
                            {
                                env.pop_scope();
                                continue;
                            }
                            let out = self.exec_block(&arm.body, env).await?;
                            env.pop_scope();
                            return Ok(out);
                        }
                    }
                    Ok(StmtOutcome::Normal)
                }
                Stmt::TryCatch { body, catches } => {
                    self.last_typed_error = None;
                    match self.exec_block(body, env).await {
                        Ok(outcome) => Ok(outcome),
                        Err(err) => {
                            let typed = self.last_typed_error.take();
                            for clause in catches {
                                let clause_type = match &clause.ty {
                                    crate::ast::TypeExpr::Named(n) => n.as_str(),
                                    _ => continue,
                                };
                                let matches = match &typed {
                                    Some((type_name, _)) => {
                                        clause_type == "Error" || clause_type == type_name
                                    }
                                    None => clause_type == "Error",
                                };
                                if matches {
                                    let fields = match &typed {
                                        Some((_, f)) => f.clone(),
                                        None => {
                                            let mut m = HashMap::new();
                                            m.insert(
                                                "message".to_string(),
                                                Value::String(err.to_string()),
                                            );
                                            m
                                        }
                                    };
                                    let error_val = Value::Map(fields);
                                    env.push_scope();
                                    env.define(clause.name.clone(), error_val);
                                    let outcome = self.exec_block(&clause.body, env).await;
                                    env.pop_scope();
                                    return outcome;
                                }
                            }
                            Err(err)
                        }
                    }
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
                if let Value::EnumVariant(_, variant, _) = value
                    && variant == name
                {
                    return Some(vec![]);
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
                if &lit == value { Some(vec![]) } else { None }
            }
            Pattern::Variant { name, bindings } => {
                if let Value::EnumVariant(_ty, variant, fields) = value
                    && variant == name
                {
                    // Bind each named destructure from the variant's
                    // rich fields (if any). Wildcards (`_`) and
                    // missing fields bind to `none`.
                    let mut out = Vec::with_capacity(bindings.len());
                    for b in bindings {
                        if b == "_" {
                            continue;
                        }
                        let v = fields
                            .as_ref()
                            .and_then(|m| m.get(b).cloned())
                            .unwrap_or(Value::None);
                        out.push((b.clone(), v));
                    }
                    return Some(out);
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
                        // Common cause: invoking `self.{field}` from a
                        // closure that runs outside any agent context —
                        // e.g. an `Http.serve(...)` handler. Those run
                        // on the event loop with no `current_agent`,
                        // so `self.` cannot resolve. Route through
                        // `Agent.send(MyAgent, data, event: "...")` to
                        // hand off to a running agent instead.
                        Err(runtime_error(format!(
                            "`self.{field}` used outside an agent context — \
                             Http.serve handlers and other top-level closures \
                             cannot access agent state. Use \
                             `Agent.send(MyAgent, data, event: \"http_request\")` \
                             to route into a running agent."
                        )))
                    }
                }

                Expr::SelfRef => {
                    if let Some(agent) = &self.current_agent {
                        let name = agent.lock().unwrap().def.name.clone();
                        Ok(Value::AgentRef(name))
                    } else {
                        Err(runtime_error(
                            "`self` used outside of an agent context".to_string(),
                        ))
                    }
                }

                Expr::FieldAccess(obj, field) => {
                    // Enum variant access: `Urgency.medium`. If `obj` is
                    // a bare identifier naming a registered type, produce
                    // an EnumVariant directly (don't evaluate `obj`, which
                    // might not be bound as a Value).
                    if let Expr::Ident(name) = obj.as_ref()
                        && !self.agents.contains_key(name)
                        && self
                            .globals
                            .get(name)
                            .is_none_or(|v| matches!(v, Value::Namespace(_)))
                        && is_pascal_case(name)
                    {
                        return Ok(Value::EnumVariant(name.clone(), field.clone(), None));
                    }
                    let obj_v = self.eval_expr(obj, env).await?;
                    match &obj_v {
                        Value::Namespace(ns_name) => {
                            Ok(Value::EnumVariant(ns_name.clone(), field.clone(), None))
                        }
                        Value::Map(m) => {
                            if let Some(v) = m.get(field) {
                                return Ok(v.clone());
                            }
                            // Fall through to property-style method call.
                            let out = self
                                .call_method_on_value(obj_v.clone(), field, vec![], env)
                                .await;
                            out.map_err(|_| runtime_error(format!("Map has no field `{field}`")))
                        }
                        _ => {
                            // Zero-arg method fallback for properties
                            // like `.count`, `.length`, `.is_empty`.
                            self.call_method_on_value(obj_v.clone(), field, vec![], env)
                                .await
                                .map_err(|_| {
                                    runtime_error(format!(
                                        "Cannot access `.{field}` on {}",
                                        obj_v.type_name()
                                    ))
                                })
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
                                "Cannot negate {}",
                                other.type_name()
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

                Expr::Range(start, end) => {
                    let s = self.eval_expr(start, env).await?;
                    let e = self.eval_expr(end, env).await?;
                    match (s, e) {
                        (Value::Integer(lo), Value::Integer(hi)) => Ok(Value::Range(lo, hi)),
                        (l, r) => Err(runtime_error(format!(
                            "range `..` expects two integers, got {} and {}",
                            l.type_name(),
                            r.type_name()
                        ))),
                    }
                }

                Expr::Pipeline(left, right) => {
                    // `x |> f` ≡ `f(x)` (single positional argument)
                    let l = self.eval_expr(left, env).await?;
                    let args = vec![CallArgValue {
                        name: None,
                        value: l,
                    }];
                    self.call_value(right, args, env).await
                }

                Expr::Call { callee, args } => {
                    let arg_values = self.eval_args(args, env).await?;
                    self.call_value(callee, arg_values, env).await
                }

                Expr::MethodCall {
                    object,
                    method,
                    args,
                } => {
                    let arg_values = self.eval_args(args, env).await?;
                    // If object is a namespace, dispatch to its method.
                    let obj_val = self.eval_expr(object, env).await?;
                    if let Value::Namespace(ns) = &obj_val {
                        let ns_name = ns.clone();
                        return self
                            .call_namespace_method(&ns_name, method, arg_values)
                            .await;
                    }
                    // AgentRef.task(...) — cross-agent task call
                    if let Value::AgentRef(name) = &obj_val {
                        return self.call_agent_task(name, method, arg_values).await;
                    }
                    // Otherwise: method on a value (e.g., list.map).
                    self.call_method_on_value(obj_val, method, arg_values, env)
                        .await
                }

                Expr::Cast { expr: inner, ty: _ } => {
                    // v0.1: casts are runtime-checked elsewhere; here we
                    // just evaluate the inner expression.
                    self.eval_expr(inner, env).await
                }

                Expr::IfExpr {
                    cond,
                    then_body,
                    else_body,
                } => {
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
                            if let Some(g) = &arm.guard
                                && !self.eval_expr(g, env).await?.is_truthy()
                            {
                                env.pop_scope();
                                continue;
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
                    let n = v
                        .as_int()
                        .ok_or_else(|| runtime_error("duration value must be int"))?;
                    Ok(Value::Duration(Value::duration_seconds(n, *unit)))
                }

                Expr::EnumVariant {
                    ty,
                    variant,
                    fields,
                } => {
                    if fields.is_empty() {
                        Ok(Value::EnumVariant(ty.clone(), variant.clone(), None))
                    } else {
                        let mut evaluated = HashMap::new();
                        for (k, v) in fields {
                            evaluated.insert(k.clone(), self.eval_expr(v, env).await?);
                        }
                        Ok(Value::EnumVariant(
                            ty.clone(),
                            variant.clone(),
                            Some(evaluated),
                        ))
                    }
                }
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

    async fn eval_args(
        &mut self,
        args: &[CallArg],
        env: &mut Environment,
    ) -> Result<Vec<CallArgValue>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let v = self.eval_expr(&a.value, env).await?;
            out.push(CallArgValue {
                name: a.name.clone(),
                value: v,
            });
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

    async fn call_task(
        &mut self,
        _name: &str,
        decl: &TaskDecl,
        args: Vec<CallArgValue>,
    ) -> Result<Value> {
        let mut env = Environment::new();
        // Bind params by position (named args not wired for user tasks yet).
        for (i, p) in decl.params.iter().enumerate() {
            let v = args.get(i).map(|a| a.value.clone()).unwrap_or(Value::None);
            bind_value(&p.name, v, &mut env)?;
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
        // Check @tools capability gating if we're in an agent context
        if let Some(agent_mutex) = &self.current_agent {
            let agent = agent_mutex.lock().unwrap();
            if !self.is_namespace_allowed(&agent.def, ns_name) {
                return Err(runtime_error(format!(
                    "CapabilityError: namespace `{ns_name}` is not allowed by the @tools attribute"
                )));
            }
        }

        let f = {
            let ns = self
                .namespaces
                .get(ns_name)
                .ok_or_else(|| runtime_error(format!("Unknown namespace: `{ns_name}`")))?;
            ns.methods.get(method).cloned().ok_or_else(|| {
                runtime_error(format!("Namespace `{ns_name}` has no method `{method}`"))
            })?
        };
        f(self, args).await
    }

    /// Check if a namespace is allowed for the given agent.
    /// If the agent has no @tools attribute, all namespaces are allowed.
    /// If @tools is specified, only those namespaces are allowed.
    fn is_namespace_allowed(&self, agent: &AgentDef, ns_name: &str) -> bool {
        for attr in &agent.attributes {
            if attr.name == "tools"
                && let AttributeBody::Expr(Expr::ListLit(items)) = &attr.body
            {
                let allowed_namespaces: Vec<String> = items
                    .iter()
                    .filter_map(|e| {
                        if let Expr::Ident(name) = e {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                return allowed_namespaces.contains(&ns_name.to_string());
            }
        }
        // No @tools attribute means all namespaces are allowed
        true
    }

    /// Extract @limits from an agent's attributes.
    /// Returns a map with timeout (seconds as f64), max_tokens (i64), max_cost (f64).
    pub fn get_agent_limits(
        &self,
        agent: &AgentDef,
    ) -> Option<(Option<f64>, Option<i64>, Option<f64>)> {
        for attr in &agent.attributes {
            if attr.name == "limits"
                && let AttributeBody::Expr(Expr::StructLit(fields)) = &attr.body
            {
                let mut timeout = None;
                let mut max_tokens = None;
                let mut max_cost = None;

                for (key, expr) in fields {
                    match key.as_str() {
                        "timeout" => {
                            if let Expr::Duration { value, unit } = expr
                                && let Expr::Integer(n) = value.as_ref()
                            {
                                let secs = Value::duration_seconds(*n, *unit);
                                timeout = Some(secs);
                            }
                        }
                        "max_tokens" => {
                            if let Expr::Integer(n) = expr {
                                max_tokens = Some(*n);
                            }
                        }
                        "max_cost" => {
                            if let Expr::Float(f) = expr {
                                max_cost = Some(*f);
                            } else if let Expr::Integer(n) = expr {
                                max_cost = Some(*n as f64);
                            }
                        }
                        _ => {}
                    }
                }

                if timeout.is_some() || max_tokens.is_some() || max_cost.is_some() {
                    return Some((timeout, max_tokens, max_cost));
                }
            }
        }
        None
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
            (Value::String(s), "length" | "len" | "count") => {
                Ok(Value::Integer(s.chars().count() as i64))
            }
            (Value::String(s), "is_empty") => Ok(Value::Bool(s.is_empty())),
            (Value::String(s), "to_str") => Ok(Value::String(s.clone())),
            (Value::String(s), "upper") => Ok(Value::String(s.to_uppercase())),
            (Value::String(s), "lower") => Ok(Value::String(s.to_lowercase())),
            (Value::String(s), "trim" | "strip") => Ok(Value::String(s.trim().to_string())),
            (Value::String(s), "contains") => {
                let needle = args
                    .first()
                    .map(|a| a.value.as_string())
                    .unwrap_or_default();
                Ok(Value::Bool(s.contains(&needle)))
            }
            (Value::String(s), "starts_with") => {
                let prefix = args
                    .first()
                    .map(|a| a.value.as_string())
                    .unwrap_or_default();
                Ok(Value::Bool(s.starts_with(prefix.as_str())))
            }
            (Value::String(s), "ends_with") => {
                let suffix = args
                    .first()
                    .map(|a| a.value.as_string())
                    .unwrap_or_default();
                Ok(Value::Bool(s.ends_with(suffix.as_str())))
            }
            (Value::String(s), "replace") => {
                let from = args
                    .first()
                    .map(|a| a.value.as_string())
                    .unwrap_or_default();
                let to = args.get(1).map(|a| a.value.as_string()).unwrap_or_default();
                Ok(Value::String(s.replace(from.as_str(), &to)))
            }
            (Value::String(s), "split") => {
                let sep = args
                    .first()
                    .map(|a| a.value.as_string())
                    .unwrap_or_else(|| " ".to_string());
                let parts: Vec<Value> = s
                    .split(sep.as_str())
                    .map(|p| Value::String(p.to_string()))
                    .collect();
                Ok(Value::List(parts))
            }
            (Value::Range(lo, hi), "count" | "len") => {
                Ok(Value::Integer(if lo <= hi { hi - lo + 1 } else { 0 }))
            }
            (Value::Range(lo, hi), "is_empty") => Ok(Value::Bool(lo > hi)),
            (Value::Range(lo, hi), "contains") => {
                let target = args.first().and_then(|a| a.value.as_int());
                Ok(Value::Bool(target.is_some_and(|n| n >= *lo && n <= *hi)))
            }
            (Value::Range(lo, hi), "first") => Ok(if lo <= hi {
                Value::Integer(*lo)
            } else {
                Value::None
            }),
            (Value::Range(lo, hi), "last") => Ok(if lo <= hi {
                Value::Integer(*hi)
            } else {
                Value::None
            }),
            (Value::Range(lo, hi), "map") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("map expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("map argument must be a function")),
                };
                let count = if lo <= hi { (hi - lo + 1) as usize } else { 0 };
                let mut out = Vec::with_capacity(count);
                for n in *lo..=*hi {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: Value::Integer(n),
                            }],
                        )
                        .await?;
                    out.push(res);
                }
                Ok(Value::List(out))
            }
            (Value::Range(lo, hi), "filter") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("filter expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("filter argument must be a function")),
                };
                let mut out = Vec::new();
                for n in *lo..=*hi {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: Value::Integer(n),
                            }],
                        )
                        .await?;
                    if res.is_truthy() {
                        out.push(Value::Integer(n));
                    }
                }
                Ok(Value::List(out))
            }
            (Value::Range(lo, hi), "push") => {
                let mut result: Vec<Value> = (*lo..=*hi).map(Value::Integer).collect();
                if let Some(arg) = args.first() {
                    result.push(arg.value.clone());
                }
                Ok(Value::List(result))
            }
            (Value::List(items), "count" | "len") => Ok(Value::Integer(items.len() as i64)),
            (Value::List(items), "is_empty") => Ok(Value::Bool(items.is_empty())),
            (Value::List(items), "contains") => {
                let target = args.first().map(|a| a.value.clone()).unwrap_or(Value::None);
                Ok(Value::Bool(items.iter().any(|v| v == &target)))
            }
            (Value::List(items), "first") => Ok(items.first().cloned().unwrap_or(Value::None)),
            (Value::List(items), "last") => Ok(items.last().cloned().unwrap_or(Value::None)),
            (Value::List(items), "map") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("map expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("map argument must be a function")),
                };
                let mut out = Vec::with_capacity(items.len());
                for item in items.clone() {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: item,
                            }],
                        )
                        .await?;
                    out.push(res);
                }
                Ok(Value::List(out))
            }
            (Value::List(items), "filter") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("filter expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("filter argument must be a function")),
                };
                let mut out = Vec::new();
                for item in items.clone() {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: item.clone(),
                            }],
                        )
                        .await?;
                    if res.is_truthy() {
                        out.push(item);
                    }
                }
                Ok(Value::List(out))
            }
            (Value::List(items), "push") => {
                let mut result = items.clone();
                if let Some(arg) = args.first() {
                    result.push(arg.value.clone());
                }
                Ok(Value::List(result))
            }
            (Value::List(items), "any") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("any expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("any: argument must be a function")),
                };
                for item in items.clone() {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: item,
                            }],
                        )
                        .await?;
                    if res.is_truthy() {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }
            (Value::List(items), "all") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("all expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("all: argument must be a function")),
                };
                for item in items.clone() {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: item,
                            }],
                        )
                        .await?;
                    if !res.is_truthy() {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }
            (Value::List(items), "find") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("find expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("find: argument must be a function")),
                };
                for item in items.clone() {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: item.clone(),
                            }],
                        )
                        .await?;
                    if res.is_truthy() {
                        return Ok(item);
                    }
                }
                Ok(Value::None)
            }
            (Value::List(items), "reduce") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("reduce expects a function as first argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("reduce: first argument must be a function")),
                };
                let mut acc = args
                    .get(1)
                    .map(|a| a.value.clone())
                    .unwrap_or(Value::None);
                for item in items.clone() {
                    acc = self
                        .call_closure(
                            &params,
                            &body,
                            vec![
                                CallArgValue {
                                    name: None,
                                    value: acc,
                                },
                                CallArgValue {
                                    name: None,
                                    value: item,
                                },
                            ],
                        )
                        .await?;
                }
                Ok(acc)
            }
            (Value::List(items), "sum") => {
                let mut int_sum: i64 = 0;
                let mut float_sum: f64 = 0.0;
                let mut is_float = false;
                for item in items {
                    match item {
                        Value::Integer(n) => {
                            int_sum += n;
                            float_sum += *n as f64;
                        }
                        Value::Float(f) => {
                            float_sum += f;
                            is_float = true;
                        }
                        _ => return Err(runtime_error("sum: list must contain only numbers")),
                    }
                }
                if is_float {
                    Ok(Value::Float(float_sum))
                } else {
                    Ok(Value::Integer(int_sum))
                }
            }
            (Value::List(items), "min") => {
                if items.is_empty() {
                    return Ok(Value::None);
                }
                let mut result = items[0].clone();
                for item in &items[1..] {
                    let less = match (&result, item) {
                        (Value::Integer(a), Value::Integer(b)) => b < a,
                        (Value::Float(a), Value::Float(b)) => b < a,
                        (Value::Integer(a), Value::Float(b)) => b < &(*a as f64),
                        (Value::Float(a), Value::Integer(b)) => &(*b as f64) < a,
                        (Value::String(a), Value::String(b)) => b < a,
                        _ => false,
                    };
                    if less {
                        result = item.clone();
                    }
                }
                Ok(result)
            }
            (Value::List(items), "max") => {
                if items.is_empty() {
                    return Ok(Value::None);
                }
                let mut result = items[0].clone();
                for item in &items[1..] {
                    let greater = match (&result, item) {
                        (Value::Integer(a), Value::Integer(b)) => b > a,
                        (Value::Float(a), Value::Float(b)) => b > a,
                        (Value::Integer(a), Value::Float(b)) => b > &(*a as f64),
                        (Value::Float(a), Value::Integer(b)) => &(*b as f64) > a,
                        (Value::String(a), Value::String(b)) => b > a,
                        _ => false,
                    };
                    if greater {
                        result = item.clone();
                    }
                }
                Ok(result)
            }
            (Value::List(items), "join") => {
                let sep = args
                    .first()
                    .map(|a| a.value.as_string())
                    .unwrap_or_default();
                let parts: Vec<String> = items.iter().map(|v| v.as_string()).collect();
                Ok(Value::String(parts.join(&sep)))
            }
            (Value::List(items), "sort") => {
                let mut sorted = items.clone();
                sorted.sort_by(|a, b| match (a, b) {
                    (Value::Integer(x), Value::Integer(y)) => x.cmp(y),
                    (Value::Float(x), Value::Float(y)) => {
                        x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    (Value::String(x), Value::String(y)) => x.cmp(y),
                    _ => std::cmp::Ordering::Equal,
                });
                Ok(Value::List(sorted))
            }
            (Value::List(items), "reverse") => {
                let mut reversed = items.clone();
                reversed.reverse();
                Ok(Value::List(reversed))
            }
            (Value::List(items), "flatten") => {
                let mut flat = Vec::new();
                for item in items {
                    match item {
                        Value::List(inner) => flat.extend(inner.clone()),
                        other => flat.push(other.clone()),
                    }
                }
                Ok(Value::List(flat))
            }
            (Value::List(items), "take") => {
                let n = args
                    .first()
                    .and_then(|a| a.value.as_int())
                    .unwrap_or(0)
                    .max(0) as usize;
                Ok(Value::List(items.iter().take(n).cloned().collect()))
            }
            (Value::List(items), "skip") => {
                let n = args
                    .first()
                    .and_then(|a| a.value.as_int())
                    .unwrap_or(0)
                    .max(0) as usize;
                Ok(Value::List(items.iter().skip(n).cloned().collect()))
            }
            (Value::Map(m), "keys") => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                Ok(Value::List(
                    keys.into_iter().map(|k| Value::String(k.clone())).collect(),
                ))
            }
            (Value::Map(m), "values") => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                Ok(Value::List(
                    keys.into_iter().map(|k| m[k].clone()).collect(),
                ))
            }
            (Value::Map(m), "get") => {
                let key = args
                    .first()
                    .map(|a| a.value.as_string())
                    .unwrap_or_default();
                Ok(m.get(&key).cloned().unwrap_or(Value::None))
            }
            (Value::Map(m), "count" | "len" | "size") => Ok(Value::Integer(m.len() as i64)),
            (Value::Map(m), "is_empty") => Ok(Value::Bool(m.is_empty())),
            (Value::Map(m), "contains" | "has") => {
                let key = args
                    .first()
                    .map(|a| a.value.as_string())
                    .unwrap_or_default();
                Ok(Value::Bool(m.contains_key(&key)))
            }
            (Value::Integer(n), "to_str") => Ok(Value::String(n.to_string())),
            (Value::Float(f), "to_str") => Ok(Value::String(f.to_string())),
            (Value::Bool(b), "to_str") => Ok(Value::String(b.to_string())),
            (Value::EnumVariant(_, v, _), "to_str") => Ok(Value::String(v.clone())),
            // datetime methods — dispatched on strings that parse as RFC 3339
            (Value::String(s), "parts") => {
                use chrono::{Datelike, Timelike};
                match chrono::DateTime::parse_from_rfc3339(s) {
                    Ok(dt) => {
                        let mut m = std::collections::HashMap::new();
                        m.insert("year".into(), Value::Integer(dt.year() as i64));
                        m.insert("month".into(), Value::Integer(dt.month() as i64));
                        m.insert("day".into(), Value::Integer(dt.day() as i64));
                        m.insert("hour".into(), Value::Integer(dt.hour() as i64));
                        m.insert("minute".into(), Value::Integer(dt.minute() as i64));
                        m.insert("second".into(), Value::Integer(dt.second() as i64));
                        m.insert(
                            "millisecond".into(),
                            Value::Integer((dt.nanosecond() / 1_000_000) as i64),
                        );
                        m.insert("tz".into(), Value::String(dt.offset().to_string()));
                        Ok(Value::Map(m))
                    }
                    Err(_) => Ok(Value::None),
                }
            }
            (Value::String(s), "format") => {
                let pattern = args
                    .iter()
                    .find(|a| a.name.as_deref() == Some("as"))
                    .or_else(|| args.first())
                    .map(|a| a.value.as_string())
                    .unwrap_or_default();
                match chrono::DateTime::parse_from_rfc3339(s) {
                    Ok(dt) => Ok(Value::String(dt.format(&pattern).to_string())),
                    Err(_) => Ok(Value::None),
                }
            }
            _ => Err(runtime_error(format!(
                "Method `{method}` not available on {}",
                obj.type_name()
            ))),
        }
    }

    async fn call_agent_task(
        &mut self,
        agent_name: &str,
        task_name: &str,
        args: Vec<CallArgValue>,
    ) -> Result<Value> {
        let def = self
            .agents
            .get(agent_name)
            .cloned()
            .ok_or_else(|| runtime_error(format!("Unknown agent: `{agent_name}`")))?;
        let task = def
            .tasks
            .iter()
            .find(|t| t.name == task_name)
            .cloned()
            .ok_or_else(|| {
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
        let def = self
            .agents
            .get(agent_name)
            .cloned()
            .ok_or_else(|| runtime_error(format!("Unknown agent: `{agent_name}`")))?;
        let mut state = HashMap::new();
        for f in &def.state_fields {
            let mut tmp_env = Environment::new();
            state.insert(
                f.name.clone(),
                self.eval_expr(&f.default, &mut tmp_env).await?,
            );
        }
        let inst = Arc::new(Mutex::new(AgentInstance {
            def: def.clone(),
            state,
        }));
        self.live_agents
            .lock()
            .unwrap()
            .insert(agent_name.to_string(), inst.clone());

        // Run @on_start block, if any.
        let on_start = def
            .attributes
            .iter()
            .find(|a| a.name == "on_start")
            .cloned();
        if let Some(attr) = on_start
            && let AttributeBody::Block(body) = attr.body
        {
            let prev = self.current_agent.take();
            self.current_agent = Some(inst.clone());
            let mut env = Environment::new();
            self.exec_block(&body, &mut env).await?;
            self.current_agent = prev;
        }

        Ok(())
    }

    pub async fn stop_agent(&mut self, agent_name: &str) -> Result<()> {
        // Run @on_stop block before removing from live_agents.
        // Clone the def out before awaiting to avoid holding the lock across an await.
        let def = self
            .live_agents
            .lock()
            .unwrap()
            .get(agent_name)
            .map(|inst| inst.lock().unwrap().def.clone());
        if let Some(def) = def {
            let on_stop = def.attributes.iter().find(|a| a.name == "on_stop").cloned();
            if let Some(attr) = on_stop
                && let AttributeBody::Block(body) = attr.body
            {
                let inst = self.live_agents.lock().unwrap().get(agent_name).cloned();
                let prev = self.current_agent.take();
                self.current_agent = inst;
                let mut env = Environment::new();
                self.exec_block(&body, &mut env).await?;
                self.current_agent = prev;
            }
        }
        self.live_agents.lock().unwrap().remove(agent_name);
        Ok(())
    }

    pub async fn run_agent_task(
        &mut self,
        agent_name: &str,
        task_name: &str,
        args: Vec<CallArgValue>,
    ) -> Result<Value> {
        let inst = self.live_agents.lock().unwrap().get(agent_name).cloned();
        let inst = match inst {
            Some(i) => i,
            None => {
                return Err(runtime_error(format!(
                    "Agent `{agent_name}` is not running"
                )));
            }
        };
        let def = inst.lock().unwrap().def.clone();
        let task = def
            .tasks
            .iter()
            .find(|t| t.name == task_name)
            .cloned()
            .ok_or_else(|| {
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
            if *b == 0 {
                return Err(runtime_error("Division by zero"));
            }
            Ok(Value::Integer(a / b))
        }
        (Mod, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a % b)),
        (Add, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Sub, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (Mul, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (Div, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
        // float op int (and int op float) — promote int to float
        (Add, Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a + *b as f64)),
        (Sub, Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a - *b as f64)),
        (Mul, Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a * *b as f64)),
        (Div, Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a / *b as f64)),
        (Add, Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
        (Sub, Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
        (Mul, Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
        (Div, Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 / b)),
        (Lt, Value::Float(a), Value::Integer(b)) => Ok(Value::Bool(*a < *b as f64)),
        (Gt, Value::Float(a), Value::Integer(b)) => Ok(Value::Bool(*a > *b as f64)),
        (Lte, Value::Float(a), Value::Integer(b)) => Ok(Value::Bool(*a <= *b as f64)),
        (Gte, Value::Float(a), Value::Integer(b)) => Ok(Value::Bool(*a >= *b as f64)),
        (Lt, Value::Integer(a), Value::Float(b)) => Ok(Value::Bool((*a as f64) < *b)),
        (Gt, Value::Integer(a), Value::Float(b)) => Ok(Value::Bool((*a as f64) > *b)),
        (Lte, Value::Integer(a), Value::Float(b)) => Ok(Value::Bool((*a as f64) <= *b)),
        (Gte, Value::Integer(a), Value::Float(b)) => Ok(Value::Bool((*a as f64) >= *b)),
        (Add, Value::String(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
        (Add, Value::List(a), Value::List(b)) => {
            let mut result = a.clone();
            result.extend(b.clone());
            Ok(Value::List(result))
        }
        // Concatenating with a range materializes it — user explicitly asked for a list.
        (Add, Value::Range(lo, hi), Value::List(b)) => {
            let mut result: Vec<Value> = (*lo..=*hi).map(Value::Integer).collect();
            result.extend(b.clone());
            Ok(Value::List(result))
        }
        (Add, Value::List(a), Value::Range(lo, hi)) => {
            let mut result = a.clone();
            result.extend((*lo..=*hi).map(Value::Integer));
            Ok(Value::List(result))
        }
        (Add, Value::Range(lo1, hi1), Value::Range(lo2, hi2)) => {
            let mut result: Vec<Value> = (*lo1..=*hi1).map(Value::Integer).collect();
            result.extend((*lo2..=*hi2).map(Value::Integer));
            Ok(Value::List(result))
        }
        (Eq, a, b) => Ok(Value::Bool(a == b)),
        (Neq, a, b) => Ok(Value::Bool(a != b)),
        (Lt, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a < b)),
        (Gt, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a > b)),
        (Lte, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a <= b)),
        (Gte, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a >= b)),
        (Lt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
        (Gt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
        (Lte, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
        (Gte, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),
        (And, a, b) => Ok(Value::Bool(a.is_truthy() && b.is_truthy())),
        (Or, a, b) => Ok(Value::Bool(a.is_truthy() || b.is_truthy())),
        // datetime (ISO string) ± duration → ISO string  (millisecond precision)
        (Add, Value::String(s), Value::Duration(secs)) => {
            use chrono::SecondsFormat;
            let dt = parse_dt(s).ok_or_else(|| {
                runtime_error(format!("cannot add duration to non-datetime string {s:?}"))
            })?;
            let ms = (*secs * 1000.0) as i64;
            let shifted = dt + chrono::Duration::milliseconds(ms);
            Ok(Value::String(
                shifted.to_rfc3339_opts(SecondsFormat::Millis, true),
            ))
        }
        (Sub, Value::String(s), Value::Duration(secs)) => {
            use chrono::SecondsFormat;
            let dt = parse_dt(s).ok_or_else(|| {
                runtime_error(format!(
                    "cannot subtract duration from non-datetime string {s:?}"
                ))
            })?;
            let ms = (*secs * 1000.0) as i64;
            let shifted = dt - chrono::Duration::milliseconds(ms);
            Ok(Value::String(
                shifted.to_rfc3339_opts(SecondsFormat::Millis, true),
            ))
        }
        // datetime - datetime → duration (seconds)
        (Sub, Value::String(a), Value::String(b)) => {
            let da = parse_dt(a).ok_or_else(|| {
                runtime_error(format!("cannot subtract: {a:?} is not a datetime string"))
            })?;
            let db = parse_dt(b).ok_or_else(|| {
                runtime_error(format!("cannot subtract: {b:?} is not a datetime string"))
            })?;
            let secs = (da - db).num_milliseconds() as f64 / 1000.0;
            Ok(Value::Duration(secs))
        }
        // datetime string comparison
        (Lt, Value::String(a), Value::String(b)) => match (parse_dt(a), parse_dt(b)) {
            (Some(da), Some(db)) => Ok(Value::Bool(da < db)),
            _ => Err(runtime_error(format!(
                "cannot compare strings {a:?} and {b:?} with `<`"
            ))),
        },
        (Gt, Value::String(a), Value::String(b)) => match (parse_dt(a), parse_dt(b)) {
            (Some(da), Some(db)) => Ok(Value::Bool(da > db)),
            _ => Err(runtime_error(format!(
                "cannot compare strings {a:?} and {b:?} with `>`"
            ))),
        },
        (Lte, Value::String(a), Value::String(b)) => match (parse_dt(a), parse_dt(b)) {
            (Some(da), Some(db)) => Ok(Value::Bool(da <= db)),
            _ => Err(runtime_error(format!(
                "cannot compare strings {a:?} and {b:?} with `<=`"
            ))),
        },
        (Gte, Value::String(a), Value::String(b)) => match (parse_dt(a), parse_dt(b)) {
            (Some(da), Some(db)) => Ok(Value::Bool(da >= db)),
            _ => Err(runtime_error(format!(
                "cannot compare strings {a:?} and {b:?} with `>=`"
            ))),
        },
        _ => Err(runtime_error(format!(
            "Cannot apply `{:?}` to {} and {}",
            op,
            l.type_name(),
            r.type_name()
        ))),
    }
}

fn is_pascal_case(s: &str) -> bool {
    s.chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
}

fn parse_dt(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d",
    ] {
        if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                ndt,
                chrono::Utc,
            ));
        }
        if let Ok(nd) = chrono::NaiveDate::parse_from_str(s, fmt) {
            let ndt = nd.and_hms_opt(0, 0, 0)?;
            return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                ndt,
                chrono::Utc,
            ));
        }
    }
    None
}
