// Rust guideline compliant 2026-02-21
//! Host trait decoupling runtime namespaces from the concrete interpreter.
//!
//! Namespace closures receive `&mut dyn Host` so individual namespaces can be
//! tested and alternate execution backends can be introduced without touching
//! namespace code.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use miette::Result;
use parking_lot::Mutex;
use tokio::sync::mpsc::UnboundedSender;

use crate::ast::{LambdaBody, LambdaParam, TaskDecl};
use crate::runtime::context::RuntimeContext;

use super::state::{AgentInstance, BuiltinFn, CallArgValue, Event, Interpreter, Namespace};
use super::value::Value;

/// Boxed async future returned by `Host` trait methods.
pub type HostFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Shared map of live agent instances used by `Host::live_agents`.
pub type LiveAgents = Arc<Mutex<HashMap<String, Arc<Mutex<AgentInstance>>>>>;

/// Interpreter capabilities exposed to runtime namespace closures.
///
/// All namespace closures receive `&mut dyn Host` instead of `&mut Interpreter`,
/// keeping namespaces testable without constructing a full interpreter and
/// making the capability boundary explicit.
///
/// `Interpreter` provides the only production implementation. Tests can supply
/// a minimal `MockHost` that implements only the methods their namespace needs.
pub trait Host: Send {
    // ── runtime backends ──────────────────────────────────────────────────

    /// Runtime backends (env, clock, LLM, file system, …).
    fn runtime(&self) -> &Arc<RuntimeContext>;

    // ── closure / task dispatch ───────────────────────────────────────────

    /// Execute a user-defined closure with the given arguments.
    fn call_closure<'a>(
        &'a mut self,
        params: &'a [LambdaParam],
        body: &'a LambdaBody,
        args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value>;

    /// Execute a user-defined task declaration.
    fn call_task<'a>(
        &'a mut self,
        name: &'a str,
        decl: &'a TaskDecl,
        args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value>;

    /// Look up an `impl` method on a value's type.
    fn find_impl_task(&self, value: &Value, method: &str) -> Option<TaskDecl>;

    /// Dispatch a namespace method call by name.
    fn call_namespace_method<'a>(
        &'a mut self,
        ns: &'a str,
        method: &'a str,
        args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value>;

    // ── agent lifecycle ───────────────────────────────────────────────────

    /// Start an agent (runs `@on_start` and registers the live instance).
    fn start_agent<'a>(&'a mut self, name: &'a str) -> HostFuture<'a, ()>;

    /// Stop an agent (runs `@on_stop` and removes the live instance).
    fn stop_agent<'a>(&'a mut self, name: &'a str) -> HostFuture<'a, ()>;

    /// Post a runtime event to the interpreter event channel.
    fn enqueue_event(&self, event: Event) -> Result<()>;

    /// Shared table of all live agent instances.
    fn live_agents(&self) -> &LiveAgents;

    // ── agent context ─────────────────────────────────────────────────────

    /// Name of the currently executing agent, or `None` outside an agent.
    fn current_agent_name(&self) -> Option<String>;

    /// Returns `(program_name, agent_name)` or an error if called outside an agent.
    fn require_agent_context(&self, caller: &str) -> Result<(String, String)>;

    /// The `@memory` attribute value of the current agent, or `None` if absent.
    fn current_memory_attr(&self) -> Option<String>;

    /// Model tag for the current agent (`@model`), or `"default"`.
    fn current_model(&self) -> String;

    /// The `@role` string of the current agent, or `None` outside an agent.
    fn current_role(&self) -> Option<String>;

    /// The `@rules` list of the current agent, or an empty vec.
    fn current_rules(&self) -> Vec<String>;

    // ── type registries ───────────────────────────────────────────────────

    /// Simple-enum registry: type name → variant list.
    fn enum_types(&self) -> &HashMap<String, Vec<String>>;

    /// Struct registry: type name → `(field_name, type_str)` pairs.
    fn struct_types(&self) -> &HashMap<String, Vec<(String, String)>>;

    // ── event infrastructure ──────────────────────────────────────────────

    /// Clone the event sender for use in background Tokio tasks.
    fn clone_event_tx(&self) -> UnboundedSender<Event>;

    /// Counter tracking active `Http.serve` listeners.
    fn active_http_servers(&self) -> &Arc<AtomicU64>;

    /// Register a closure for later firing via `Event::FireClosure`.
    ///
    /// Returns the closure id to embed in scheduled events.
    fn register_closure(
        &mut self,
        agent_name: String,
        params: Vec<LambdaParam>,
        body: LambdaBody,
    ) -> u64;

    /// Spawn a closure as an independent Tokio task with a snapshotted interpreter state.
    ///
    /// Returns the async handle id that callers can pass to `Async.join_all` or
    /// `Async.select`. The concrete implementation constructs a child interpreter
    /// sharing the parent's event infrastructure and symbol tables.
    fn spawn_closure<'a>(
        &'a mut self,
        params: Vec<LambdaParam>,
        body: LambdaBody,
    ) -> HostFuture<'a, u64>;

    // ── prelude installation ──────────────────────────────────────────────

    /// Insert a global name–value binding (used during prelude installation).
    fn insert_global(&mut self, name: String, value: Value);

    /// Register a prelude namespace.
    fn register_namespace(&mut self, ns: Namespace);

    /// Register a top-level built-in function (e.g. `run`, `stop`, `uuid`).
    fn register_top_fn(&mut self, name: &str, f: BuiltinFn);
}

// ---------------------------------------------------------------------------
// impl Host for Interpreter
// ---------------------------------------------------------------------------

impl Host for Interpreter {
    fn runtime(&self) -> &Arc<RuntimeContext> {
        &self.runtime
    }

    fn call_closure<'a>(
        &'a mut self,
        params: &'a [LambdaParam],
        body: &'a LambdaBody,
        args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value> {
        Box::pin(Interpreter::call_closure(self, params, body, args))
    }

    fn call_task<'a>(
        &'a mut self,
        name: &'a str,
        decl: &'a TaskDecl,
        args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value> {
        Box::pin(Interpreter::call_task(self, name, decl, args))
    }

    fn find_impl_task(&self, value: &Value, method: &str) -> Option<TaskDecl> {
        Interpreter::find_impl_task(self, value, method)
    }

    fn call_namespace_method<'a>(
        &'a mut self,
        ns: &'a str,
        method: &'a str,
        args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value> {
        Box::pin(Interpreter::call_namespace_method(self, ns, method, args))
    }

    fn start_agent<'a>(&'a mut self, name: &'a str) -> HostFuture<'a, ()> {
        Box::pin(Interpreter::start_agent(self, name))
    }

    fn stop_agent<'a>(&'a mut self, name: &'a str) -> HostFuture<'a, ()> {
        Box::pin(Interpreter::stop_agent(self, name))
    }

    fn enqueue_event(&self, event: Event) -> Result<()> {
        Interpreter::enqueue_event(self, event)
    }

    fn live_agents(&self) -> &LiveAgents {
        &self.live_agents
    }

    fn current_agent_name(&self) -> Option<String> {
        Interpreter::current_agent_name(self)
    }

    fn require_agent_context(&self, caller: &str) -> Result<(String, String)> {
        Interpreter::require_agent_context(self, caller)
    }

    fn current_memory_attr(&self) -> Option<String> {
        Interpreter::current_memory_attr(self)
    }

    fn current_model(&self) -> String {
        Interpreter::current_model(self)
    }

    fn current_role(&self) -> Option<String> {
        Interpreter::current_role(self)
    }

    fn current_rules(&self) -> Vec<String> {
        Interpreter::current_rules(self)
    }

    fn enum_types(&self) -> &HashMap<String, Vec<String>> {
        &self.enum_types
    }

    fn struct_types(&self) -> &HashMap<String, Vec<(String, String)>> {
        &self.struct_types
    }

    fn clone_event_tx(&self) -> UnboundedSender<Event> {
        self.event_tx.clone()
    }

    fn active_http_servers(&self) -> &Arc<AtomicU64> {
        &self.active_http_servers
    }

    fn register_closure(
        &mut self,
        agent_name: String,
        params: Vec<LambdaParam>,
        body: LambdaBody,
    ) -> u64 {
        Interpreter::register_closure(self, agent_name, params, body)
    }

    fn spawn_closure<'a>(
        &'a mut self,
        params: Vec<LambdaParam>,
        body: LambdaBody,
    ) -> HostFuture<'a, u64> {
        Box::pin(async move {
            let handle_id = self.runtime.next_async_handle_id();
            let runtime = self.runtime.clone();
            let current_agent = self.current_agent.clone();
            let program_name = self.program_name.clone();
            // Snapshot program symbol tables so the spawned task can resolve
            // user-defined tasks, enum/struct types, and registered closures.
            let globals = self.globals.clone();
            let agents = self.agents.clone();
            let enum_types = self.enum_types.clone();
            let struct_types = self.struct_types.clone();
            let struct_aliases = self.struct_aliases.clone();
            // Share event infrastructure so the spawned task can use
            // Schedule.*, Agent.send, and Http.serve.
            let closures = self.closures.clone();
            let next_closure_id = self.next_closure_id.clone();
            let event_tx = self.event_tx.clone();
            let active_http_servers = self.active_http_servers.clone();
            let live_agents = self.live_agents.clone();

            let handle = tokio::spawn(async move {
                let mut local_interp = Interpreter::with_runtime(runtime);
                local_interp.globals = globals;
                local_interp.agents = agents;
                local_interp.enum_types = enum_types;
                local_interp.struct_types = struct_types;
                local_interp.struct_aliases = struct_aliases;
                local_interp.closures = closures;
                local_interp.next_closure_id = next_closure_id;
                local_interp.event_tx = event_tx;
                local_interp.event_rx = None;
                local_interp.active_http_servers = active_http_servers;
                local_interp.live_agents = live_agents;
                local_interp.current_agent = current_agent;
                local_interp.program_name = program_name;
                local_interp
                    .call_closure(&params, &body, vec![])
                    .await
                    .map_err(|err| err.to_string())
            });
            self.runtime.insert_async_task(handle_id, handle);
            Ok(handle_id)
        })
    }

    fn insert_global(&mut self, name: String, value: Value) {
        self.globals.insert(name, value);
    }

    fn register_namespace(&mut self, ns: Namespace) {
        Interpreter::register_namespace(self, ns);
    }

    fn register_top_fn(&mut self, name: &str, f: BuiltinFn) {
        Interpreter::register_top_fn(self, name, f);
    }
}

// ---------------------------------------------------------------------------
// MockHost — minimal test double for stateless namespaces
// ---------------------------------------------------------------------------

/// Minimal `Host` implementation for testing namespaces that access no
/// interpreter state (Math, Crypto, Random, Uuid.v4/v5, …).
///
/// Every `Host` method panics with "MockHost does not support …". Use this
/// type with namespaces whose closures never call back into the interpreter;
/// for namespaces that do (Agent, Control, Memory, …) define a purpose-built
/// struct that implements `Host` with the specific methods those closures use.
#[cfg(any(test, feature = "test-util"))]
pub struct MockHost {
    /// Backing runtime backends (env, clock, LLM, …).  Supply via
    /// `RuntimeContext::test_context(...)` to test namespaces that read
    /// `runtime().clock`, `runtime().env`, etc.
    pub runtime: Arc<RuntimeContext>,
}

#[cfg(any(test, feature = "test-util"))]
impl MockHost {
    /// Create a `MockHost` backed by the given `RuntimeContext`.
    pub fn new(runtime: Arc<RuntimeContext>) -> Self {
        Self { runtime }
    }
}

#[cfg(any(test, feature = "test-util"))]
impl Host for MockHost {
    fn runtime(&self) -> &Arc<RuntimeContext> {
        &self.runtime
    }

    fn call_closure<'a>(
        &'a mut self,
        _params: &'a [LambdaParam],
        _body: &'a LambdaBody,
        _args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value> {
        unimplemented!("MockHost does not support call_closure")
    }

    fn call_task<'a>(
        &'a mut self,
        _name: &'a str,
        _decl: &'a TaskDecl,
        _args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value> {
        unimplemented!("MockHost does not support call_task")
    }

    fn find_impl_task(&self, _value: &Value, _method: &str) -> Option<TaskDecl> {
        unimplemented!("MockHost does not support find_impl_task")
    }

    fn call_namespace_method<'a>(
        &'a mut self,
        _ns: &'a str,
        _method: &'a str,
        _args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value> {
        unimplemented!("MockHost does not support call_namespace_method")
    }

    fn start_agent<'a>(&'a mut self, _name: &'a str) -> HostFuture<'a, ()> {
        unimplemented!("MockHost does not support start_agent")
    }

    fn stop_agent<'a>(&'a mut self, _name: &'a str) -> HostFuture<'a, ()> {
        unimplemented!("MockHost does not support stop_agent")
    }

    fn enqueue_event(&self, _event: Event) -> miette::Result<()> {
        unimplemented!("MockHost does not support enqueue_event")
    }

    fn live_agents(&self) -> &LiveAgents {
        unimplemented!("MockHost does not support live_agents")
    }

    fn current_agent_name(&self) -> Option<String> {
        unimplemented!("MockHost does not support current_agent_name")
    }

    fn require_agent_context(&self, _caller: &str) -> miette::Result<(String, String)> {
        unimplemented!("MockHost does not support require_agent_context")
    }

    fn current_memory_attr(&self) -> Option<String> {
        unimplemented!("MockHost does not support current_memory_attr")
    }

    fn current_model(&self) -> String {
        unimplemented!("MockHost does not support current_model")
    }

    fn current_role(&self) -> Option<String> {
        unimplemented!("MockHost does not support current_role")
    }

    fn current_rules(&self) -> Vec<String> {
        unimplemented!("MockHost does not support current_rules")
    }

    fn enum_types(&self) -> &HashMap<String, Vec<String>> {
        unimplemented!("MockHost does not support enum_types")
    }

    fn struct_types(&self) -> &HashMap<String, Vec<(String, String)>> {
        unimplemented!("MockHost does not support struct_types")
    }

    fn clone_event_tx(&self) -> UnboundedSender<Event> {
        unimplemented!("MockHost does not support clone_event_tx")
    }

    fn active_http_servers(&self) -> &Arc<AtomicU64> {
        unimplemented!("MockHost does not support active_http_servers")
    }

    fn register_closure(
        &mut self,
        _agent_name: String,
        _params: Vec<LambdaParam>,
        _body: LambdaBody,
    ) -> u64 {
        unimplemented!("MockHost does not support register_closure")
    }

    fn spawn_closure<'a>(
        &'a mut self,
        _params: Vec<LambdaParam>,
        _body: LambdaBody,
    ) -> HostFuture<'a, u64> {
        unimplemented!("MockHost does not support spawn_closure")
    }

    fn insert_global(&mut self, _name: String, _value: Value) {
        unimplemented!("MockHost does not support insert_global")
    }

    fn register_namespace(&mut self, _ns: Namespace) {
        unimplemented!("MockHost does not support register_namespace")
    }

    fn register_top_fn(&mut self, _name: &str, _f: BuiltinFn) {
        unimplemented!("MockHost does not support register_top_fn")
    }
}
