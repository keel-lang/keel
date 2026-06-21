use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use miette::{NamedSource, Result};
use parking_lot::Mutex;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use super::store::ProgramStore;
use crate::ast::{
    AttributeBody, AttributeDecl, Binding, Expr, LambdaBody, LambdaParam, Node, OnHandler, Param,
    StateField, StringPart, TaskDecl, TaskSig, TypeExpr,
};
use crate::types::interface::TypeEnv;

use super::bind_value;
use super::environment::Environment;
use super::error::RuntimeErrorKind;
use super::host::Host;
use super::runtime_error;
use super::value::Value;

pub type BuiltinFn = Arc<
    dyn for<'a> Fn(
            &'a mut dyn Host,
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

#[derive(Clone)]
pub(crate) struct RecordedMockCall {
    pub args: Vec<CallArgValue>,
}

#[derive(Clone)]
pub(crate) struct TestMockState {
    pub returns: Vec<Value>,
    pub calls: Vec<RecordedMockCall>,
}

fn mock_call_matches(actual: &[CallArgValue], expected: &[CallArgValue]) -> bool {
    let actual_positionals: Vec<&Value> = actual
        .iter()
        .filter(|arg| arg.name.is_none())
        .map(|arg| &arg.value)
        .collect();
    let expected_positionals: Vec<&Value> = expected
        .iter()
        .filter(|arg| arg.name.is_none())
        .map(|arg| &arg.value)
        .collect();

    if expected_positionals.len() > actual_positionals.len()
        || expected_positionals
            .iter()
            .zip(actual_positionals.iter())
            .any(|(expected, actual)| *expected != *actual)
    {
        return false;
    }

    expected
        .iter()
        .filter_map(|arg| arg.name.as_deref().map(|name| (name, &arg.value)))
        .all(|(expected_name, expected_value)| {
            actual.iter().any(|arg| {
                arg.name.as_deref() == Some(expected_name) && &arg.value == expected_value
            })
        })
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
    pub tasks: Vec<Arc<TaskDecl>>,
    pub handlers: Vec<OnHandler>,
}

/// Precomputed per-turn tool allowlist derived from `@tools` after evaluating
/// any `when` guards. `None` on `AgentInstance` means no `@tools` attribute
/// was present — std module calls are denied (capabilities are declared,
/// never implied; `@tools all` is the explicit unrestricted form).
pub(crate) struct AllowedTools(pub(crate) Vec<(String, Option<String>)>);

impl AllowedTools {
    /// Returns true if `ns.method` is covered by any entry.
    /// An entry with `method = None` grants the whole namespace;
    /// the `all` wildcard (from `@tools all`) grants everything.
    pub(crate) fn allows(&self, ns: &str, method: &str) -> bool {
        self.0
            .iter()
            .any(|(n, m)| n == "all" || (n == ns && m.as_deref().is_none_or(|m| m == method)))
    }
}

/// Live agent instance — state + a reference to its declaration.
pub struct AgentInstance {
    pub def: Arc<AgentDef>,
    pub state: HashMap<String, Value>,
    /// Evaluated once per handler dispatch from `@tools`. See `evaluate_tools_for_turn`.
    pub(crate) allowed_tools: Option<AllowedTools>,
}

// ---------------------------------------------------------------------------
// Interpreter state
// ---------------------------------------------------------------------------

/// Runtime event posted to the interpreter's mailbox from tokio tasks
/// (schedulers, message dispatchers, …) and consumed by `execute`.
#[allow(dead_code)]
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
#[allow(dead_code)]
#[derive(Clone)]
pub struct ScheduledClosure {
    pub agent_name: String,
    pub params: Vec<LambdaParam>,
    pub body: LambdaBody,
}

pub struct Interpreter {
    /// Top-level name → value (types, tasks, agent defs, namespaces).
    pub(crate) globals: HashMap<String, Value>,
    /// Interned declaration tables (impl methods, agent definitions).
    pub(crate) store: ProgramStore,
    /// Live agent instances started via `run()`, keyed by agent name.
    pub(crate) live_agents: Arc<Mutex<HashMap<String, Arc<Mutex<AgentInstance>>>>>,
    /// Currently-executing agent (for `self.` access inside tasks).
    pub(crate) current_agent: Option<Arc<Mutex<AgentInstance>>>,
    /// Prelude namespaces installed at startup.
    pub(crate) namespaces: HashMap<String, Namespace>,
    /// Test-local namespace method override sequences and call metadata.
    pub(crate) test_mocks: Arc<Mutex<HashMap<(String, String), TestMockState>>>,
    /// Simple-enum type name → variant names. Populated from `type X = a | b` declarations.
    pub(crate) enum_types: HashMap<String, Vec<String>>,
    /// Struct type name → field (name, type_string) pairs. Populated from `type T { f: ty }`.
    pub(crate) struct_types: HashMap<String, Vec<(String, String)>>,
    /// Struct alias name → canonical struct type name. Populated from `type Alias = T`.
    pub(crate) struct_aliases: HashMap<String, String>,
    /// Known interfaces: interface_name → required method signatures.
    /// Pre-seeded with built-ins (Stringable); extended by `interface` declarations.
    pub(crate) interfaces: HashMap<String, Vec<TaskSig>>,
    /// Type-resolution environment built from `type` declarations before any
    /// `impl` conformance checks run.  Used by [`crate::types::interface`].
    pub(crate) type_env: TypeEnv,
    /// Runtime backends for nondeterministic boundaries.
    pub(crate) runtime: Arc<crate::runtime::context::RuntimeContext>,
    /// Sender for runtime events. Spawned tokio tasks (scheduler,
    /// message dispatcher) clone this to post events to the main loop.
    /// Capacity is set from `RuntimeContext::event_queue_capacity`.
    pub(crate) event_tx: Sender<Event>,
    /// Receiver end of the event channel. Owned by the interpreter
    /// between `new()` and `execute()`, then moved into the event
    /// loop. `execute()` will panic if called twice.
    pub(crate) event_rx: Option<Receiver<Event>>,
    /// Registered closures keyed by id. Scheduled tasks post the id
    /// via `Event::FireClosure`; the event loop looks up the closure
    /// and invokes it in the correct agent context. Shared with any
    /// interpreter spawned via `Async.spawn` so scheduled closures
    /// registered inside a spawned task are visible to the main event loop.
    pub(crate) closures: Arc<Mutex<HashMap<u64, ScheduledClosure>>>,
    /// Next free closure id — shared with spawned interpreters to avoid
    /// id collisions when both register closures concurrently.
    pub(crate) next_closure_id: Arc<AtomicU64>,
    /// Number of active Http.serve listeners. The event loop keeps running
    /// while this is > 0, even if no agents are live.
    pub(crate) active_http_servers: Arc<AtomicU64>,
    /// Source for diagnostics (optional).
    pub(crate) source: Option<NamedSource<String>>,
    /// Memory namespace derived from the source file path via `derive_program_name`.
    /// Format: `<stem>_<sha256[:12]>` for real files; `__repl__` / `__inline__`
    /// for REPL and inline evaluations. Defaults to `"__inline__"`.
    pub(crate) program_name: String,
    /// Type name of the user-authored provider registered with `ai.install`,
    /// or `None` when no program-wide user provider is installed. This is the
    /// lowest-precedence backend, below per-call prefixes and `@provider`.
    pub(crate) installed_provider: Option<String>,
    /// User-provider type names whose `complete()` is currently executing.
    /// Guards against a provider re-entering `ai.*` from inside its own
    /// `complete()`, which would recurse without bound.
    pub(crate) active_providers: Vec<String>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self::with_runtime(crate::runtime::context::RuntimeContext::native())
    }

    pub fn with_runtime(runtime: Arc<crate::runtime::context::RuntimeContext>) -> Self {
        let (event_tx, event_rx) = channel(runtime.event_queue_capacity());
        let mut interp = Interpreter {
            globals: HashMap::with_capacity(64),
            store: ProgramStore::new(),
            live_agents: Arc::new(Mutex::new(HashMap::new())),
            current_agent: None,
            namespaces: HashMap::with_capacity(32),
            test_mocks: Arc::new(Mutex::new(HashMap::new())),
            enum_types: HashMap::with_capacity(16),
            struct_types: HashMap::with_capacity(16),
            struct_aliases: HashMap::with_capacity(16),
            interfaces: builtin_interfaces(),
            type_env: TypeEnv::new(),
            runtime,
            event_tx,
            event_rx: Some(event_rx),
            closures: Arc::new(Mutex::new(HashMap::new())),
            next_closure_id: Arc::new(AtomicU64::new(0)),
            active_http_servers: Arc::new(AtomicU64::new(0)),
            source: None,
            program_name: "__inline__".to_string(),
            installed_provider: None,
            active_providers: Vec::new(),
        };
        crate::runtime::install_prelude(&mut interp);
        interp
    }

    pub(crate) fn set_test_mocks(&mut self, mocks: HashMap<(String, String), TestMockState>) {
        self.test_mocks = Arc::new(Mutex::new(mocks));
    }

    pub(crate) fn register_test_mock_return(
        &mut self,
        namespace: &str,
        method: &str,
        value: Value,
    ) {
        self.test_mocks
            .lock()
            .entry((namespace.to_string(), method.to_string()))
            .or_insert_with(|| TestMockState {
                returns: Vec::new(),
                calls: Vec::new(),
            })
            .returns
            .push(value);
    }

    pub(crate) fn test_mock_called(&self, namespace: &str, method: &str) -> Option<bool> {
        self.test_mocks
            .lock()
            .get(&(namespace.to_string(), method.to_string()))
            .map(|state| !state.calls.is_empty())
    }

    pub(crate) fn test_mock_call_count(&self, namespace: &str, method: &str) -> Option<i64> {
        self.test_mocks
            .lock()
            .get(&(namespace.to_string(), method.to_string()))
            .map(|state| state.calls.len() as i64)
    }

    pub(crate) fn test_mock_called_with(
        &self,
        namespace: &str,
        method: &str,
        expected_args: &[CallArgValue],
    ) -> Option<bool> {
        self.test_mocks
            .lock()
            .get(&(namespace.to_string(), method.to_string()))
            .map(|state| {
                state
                    .calls
                    .iter()
                    .any(|call| mock_call_matches(&call.args, expected_args))
            })
    }

    /// Register a closure for later firing via `Event::FireClosure`.
    /// Returns the id to embed in scheduled events.
    pub fn register_closure(
        &mut self,
        agent_name: String,
        params: Vec<LambdaParam>,
        body: LambdaBody,
    ) -> u64 {
        let id = self.next_closure_id.fetch_add(1, Ordering::Relaxed);
        self.closures.lock().insert(
            id,
            ScheduledClosure {
                agent_name,
                params,
                body,
            },
        );
        id
    }

    pub(crate) fn enqueue_event(&self, event: Event) -> Result<()> {
        // try_send must be used (not .send().await) — the event loop is single-threaded
        // and enqueue_event is called from within handlers running on that same thread.
        // A blocking send would deadlock.
        self.event_tx.try_send(event).map_err(|err| match err {
            TrySendError::Full(_) => crate::runtime::namespace::make_typed_report(
                RuntimeErrorKind::RuntimeBusy,
                "event queue is full",
            ),
            TrySendError::Closed(_) => runtime_error("event loop is closed"),
        })
    }

    /// Fire a registered closure in the named agent's context.
    /// Called by the event loop when `Event::FireClosure` arrives.
    pub async fn call_scheduled_closure(
        &mut self,
        agent_name: &str,
        closure_id: u64,
    ) -> Result<()> {
        let closure = self.closures.lock().get(&closure_id).cloned();
        let inst = self.live_agents.lock().get(agent_name).cloned();
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
    pub async fn call_event_handler(
        &mut self,
        agent_name: &str,
        event_name: &str,
        data: Value,
    ) -> Result<()> {
        let inst = self.live_agents.lock().get(agent_name).cloned();
        let Some(agent_inst) = inst else {
            return Ok(());
        };
        let handler = agent_inst
            .lock()
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
        self.evaluate_tools_for_turn().await?;
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

    /// The current agent's `@provider` attribute, written as a bareword
    /// identifier — either a built-in backend name (`ollama`, `openai`,
    /// `anthropic`) or a user-authored provider type — or `None` when absent.
    /// The `ai` namespace uses it as the agent's default provider for model
    /// tags that carry no `provider:` prefix.
    pub fn current_provider(&self) -> Option<String> {
        let agent = self.current_agent.as_ref()?;
        let def = agent.lock().def.clone();
        for attr in &def.attributes {
            if attr.name == "provider"
                && let AttributeBody::Expr(node) = &attr.body
                && let Expr::Ident(name) = &node.kind
            {
                return Some(name.clone());
            }
        }
        None
    }

    /// The current agent's `@limits { max_tokens: N }`, or `None` when absent or
    /// not a positive integer literal. Threaded into the provider request as the
    /// generation cap. Only the inline-struct form is read; spread-update limits
    /// fall back to the default.
    pub fn current_max_tokens(&self) -> Option<u32> {
        let agent = self.current_agent.as_ref()?;
        let def = agent.lock().def.clone();
        for attr in &def.attributes {
            if attr.name == "limits"
                && let AttributeBody::Expr(node) = &attr.body
                && let Expr::StructLit(fields) = &node.kind
            {
                for (key, val) in fields {
                    if key.as_str() == Some("max_tokens")
                        && let Expr::Integer(n) = &val.kind
                        && *n > 0
                    {
                        return u32::try_from(*n).ok();
                    }
                }
            }
        }
        None
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
        let def = agent.lock().def.clone();
        for attr in &def.attributes {
            if attr.name == "rules"
                && let AttributeBody::Expr(node) = &attr.body
                && let Expr::ListLit(items) = &node.kind
            {
                return items
                    .iter()
                    .filter_map(|e| match &e.kind {
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
            .map(|a| a.lock().def.name.clone())
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
        let def = agent.lock().def.clone();
        for attr in &def.attributes {
            if attr.name == "memory" {
                match &attr.body {
                    AttributeBody::Expr(node) if matches!(&node.kind, Expr::Ident(_)) => {
                        let Expr::Ident(mode) = &node.kind else {
                            unreachable!()
                        };
                        return Some(mode.clone());
                    }
                    // `none` is a reserved keyword, so `@memory none` parses as Expr::None_
                    AttributeBody::Expr(node) if matches!(&node.kind, Expr::None_) => {
                        return Some("none".to_string());
                    }
                    _ => {}
                }
            }
        }
        None
    }

    fn agent_string_attr(&self, name: &str) -> Option<String> {
        let agent = self.current_agent.as_ref()?;
        let def = agent.lock().def.clone();
        for attr in &def.attributes {
            if attr.name == name
                && let AttributeBody::Expr(node) = &attr.body
                && let Expr::StringLit(parts) = &node.kind
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
    ///
    /// Registration makes the module dispatchable; it does NOT bind the
    /// module name in scope. Bindings are created per program from
    /// `use std/<name>` declarations (or [`bind_all_namespaces`] in the
    /// REPL, where the full stdlib is pre-imported for convenience).
    ///
    /// [`bind_all_namespaces`]: Self::bind_all_namespaces
    pub fn register_namespace(&mut self, ns: Namespace) {
        self.namespaces.insert(ns.name.clone(), ns);
    }

    /// Bind every registered std module in the root scope under its
    /// canonical name. REPL-only convenience — programs must import
    /// modules explicitly with `use std/<name>`.
    pub fn bind_all_namespaces(&mut self) {
        let names: Vec<String> = self.namespaces.keys().cloned().collect();
        for name in names {
            if name == "__global" {
                continue;
            }
            self.globals.insert(name.clone(), Value::Namespace(name));
        }
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

/// Returns the set of interfaces that are always available without an explicit
/// `interface` declaration.  Adding a new built-in here is the only change
/// needed to introduce another reserved interface name.
fn builtin_interfaces() -> HashMap<String, Vec<TaskSig>> {
    let mut map = HashMap::new();

    // Synthetic params have no source position; use the 0..0 sentinel span.
    let self_param = || Param {
        name: Binding::Ident("self".to_string()),
        name_span: 0..0,
        ty: Node::synthetic(TypeExpr::SelfType),
        default: None,
        variadic: false,
    };
    let dynamic_param = |name: &str| Param {
        name: Binding::Ident(name.to_string()),
        name_span: 0..0,
        ty: Node::synthetic(TypeExpr::Dynamic),
        default: None,
        variadic: false,
    };
    let named_param = |name: &str, ty: &str| Param {
        name: Binding::Ident(name.to_string()),
        name_span: 0..0,
        ty: Node::synthetic(TypeExpr::Named(ty.to_string())),
        default: None,
        variadic: false,
    };

    map.insert(
        "Stringable".to_string(),
        vec![TaskSig {
            name: "to_str".to_string(),
            name_span: 0..0,
            params: vec![self_param()],
            return_type: Some(Node::synthetic(TypeExpr::Named("str".to_string()))),
        }],
    );
    map.insert(
        "Serializable".to_string(),
        vec![TaskSig {
            name: "to_json".to_string(),
            name_span: 0..0,
            params: vec![self_param()],
            return_type: Some(Node::synthetic(TypeExpr::Named("str".to_string()))),
        }],
    );
    map.insert(
        "Comparable".to_string(),
        vec![TaskSig {
            name: "compare".to_string(),
            name_span: 0..0,
            params: vec![self_param(), dynamic_param("other")],
            return_type: Some(Node::synthetic(TypeExpr::Named("int".to_string()))),
        }],
    );
    map.insert(
        "Equatable".to_string(),
        vec![TaskSig {
            name: "equals".to_string(),
            name_span: 0..0,
            params: vec![self_param(), dynamic_param("other")],
            return_type: Some(Node::synthetic(TypeExpr::Named("bool".to_string()))),
        }],
    );
    map.insert(
        "Iterable".to_string(),
        vec![TaskSig {
            name: "items".to_string(),
            name_span: 0..0,
            params: vec![self_param()],
            // list[dynamic] — wildcard list return, matches any list[T] in conformance checks
            return_type: Some(Node::synthetic(TypeExpr::List(Box::new(TypeExpr::Dynamic)))),
        }],
    );
    map.insert(
        "LlmProvider".to_string(),
        vec![TaskSig {
            name: "complete".to_string(),
            name_span: 0..0,
            params: vec![self_param(), named_param("req", "CompletionRequest")],
            return_type: Some(Node::synthetic(TypeExpr::Named("str".to_string()))),
        }],
    );
    map
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}
