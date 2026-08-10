//! `CompiledHost` — the `Host` implementation backing `keel-codegen`-compiled
//! binaries, mirroring `Interpreter`'s role in the tree-walking path (see
//! `designs/llvm-compilation.md` §2.7). Compiled code has no closures, tasks,
//! or agents of its own to dispatch back into (that's still the
//! interpreter's job for anything this milestone doesn't compile), so most
//! methods are unreachable for now and `todo!()`. Namespace-method dispatch
//! (`call_namespace_method`, `register_namespace`) is real: it reuses the
//! exact same `Namespace` closures the interpreter installs, via
//! `keel_runtime::runtime::namespaces::install` — the "23 namespaces for
//! free" seam §2.7 describes.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use keel_runtime::interpreter::host::{Host, HostFuture, LiveAgents};
use keel_runtime::interpreter::value::Value;
use keel_runtime::interpreter::{BuiltinFn, CallArgValue, Event, Namespace};
use keel_runtime::runtime::context::RuntimeContext;
use keel_syntax::ast::{LambdaBody, LambdaParam, TaskDecl};
use parking_lot::Mutex;
use tokio::sync::mpsc;

/// `Host` implementation for compiled binaries. Holds the same
/// `RuntimeContext` (env, clock, LLM, file system, …) the interpreter uses,
/// plus every stdlib namespace's method closures (installed at construction
/// time, same as `Interpreter::new`). Everything else a compiled program
/// needs is either resolved at compile time (direct `CallTarget::Fn` calls
/// never reach `Host`) or not yet supported (agents, closures, tasks-by-name).
pub struct CompiledHost {
    runtime: Arc<RuntimeContext>,
    live_agents: LiveAgents,
    namespaces: HashMap<String, Namespace>,
}

impl CompiledHost {
    pub fn new(runtime: Arc<RuntimeContext>) -> Self {
        let mut host = Self {
            runtime,
            live_agents: Arc::new(Mutex::new(HashMap::new())),
            namespaces: HashMap::new(),
        };
        keel_runtime::runtime::namespaces::install(&mut host);
        host
    }
}

impl Host for CompiledHost {
    fn runtime(&self) -> &Arc<RuntimeContext> {
        &self.runtime
    }

    fn call_closure<'a>(
        &'a mut self,
        _params: &'a [LambdaParam],
        _body: &'a LambdaBody,
        _args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value> {
        todo!("compiled programs have no closures to dispatch back into (M1 scope)")
    }

    fn call_task<'a>(
        &'a mut self,
        _name: &'a str,
        _decl: &'a TaskDecl,
        _args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value> {
        todo!("compiled tasks are called directly via CallTarget::Fn, never through Host")
    }

    fn find_impl_task(&self, _value: &Value, _method: &str) -> Option<Arc<TaskDecl>> {
        // Correct today, not just a stub: only a `Value::Struct` receiver can
        // ever have an impl method, and no compiled call site can construct
        // one yet — there is no value-method lowering in `keel-kir` at all
        // (see `call_method_on_value` below), so this is never reached with
        // a struct that might actually have one. Revisit once struct
        // receivers lower.
        None
    }

    fn call_namespace_method<'a>(
        &'a mut self,
        ns: &'a str,
        method: &'a str,
        args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value> {
        // `@tools` capability gating (`Interpreter::call_namespace_method`,
        // `call.rs`) is deliberately skipped here: it only ever applies
        // inside an agent's turn, and compiled programs can't compile an
        // agent yet (M3), so there is no context to gate. Revisit once
        // agents land — enforcement must ride this same path per §2.7.
        let Some(namespace) = self.namespaces.get(ns) else {
            return Box::pin(async move {
                Err(miette::miette!(
                    "`{ns}` is not a registered stdlib namespace"
                ))
            });
        };
        let Some(closure) = namespace.methods.get(method).cloned() else {
            return Box::pin(
                async move { Err(miette::miette!("`{ns}` has no method `{method}`")) },
            );
        };
        Box::pin(async move { closure(self, args).await })
    }

    fn call_method_on_value<'a>(
        &'a mut self,
        obj: Value,
        method: &'a str,
        args: Vec<CallArgValue>,
    ) -> HostFuture<'a, Value> {
        Box::pin(keel_runtime::interpreter::call_method_on_value(
            self, obj, method, args,
        ))
    }

    fn start_agent<'a>(&'a mut self, _name: &'a str) -> HostFuture<'a, ()> {
        todo!("agents are not compiled yet")
    }

    fn stop_agent<'a>(&'a mut self, _name: &'a str) -> HostFuture<'a, ()> {
        todo!("agents are not compiled yet")
    }

    fn enqueue_event(&self, _event: Event) -> miette::Result<()> {
        todo!("event dispatch is not compiled yet")
    }

    fn live_agents(&self) -> &LiveAgents {
        &self.live_agents
    }

    fn current_agent_name(&self) -> Option<String> {
        None
    }

    fn require_agent_context(&self, caller: &str) -> miette::Result<(String, String)> {
        todo!("{caller}: agent context is not compiled yet")
    }

    fn current_memory_attr(&self) -> Option<String> {
        None
    }

    fn current_model(&self) -> String {
        "default".to_string()
    }

    fn current_provider(&self) -> Option<String> {
        None
    }

    fn installed_provider(&self) -> Option<String> {
        None
    }

    fn set_installed_provider(&mut self, _type_name: String) {
        todo!("ai.install is not compiled yet")
    }

    fn provider_is_active(&self, _type_name: &str) -> bool {
        false
    }

    fn push_active_provider(&mut self, _type_name: String) {
        todo!("user-authored providers are not compiled yet")
    }

    fn pop_active_provider(&mut self, _type_name: &str) {
        todo!("user-authored providers are not compiled yet")
    }

    fn current_max_tokens(&self) -> Option<u32> {
        None
    }

    fn current_role(&self) -> Option<String> {
        None
    }

    fn current_rules(&self) -> Vec<String> {
        Vec::new()
    }

    fn enum_types(&self) -> &HashMap<String, Vec<String>> {
        todo!("enum type registry is not compiled yet")
    }

    fn struct_types(&self) -> &HashMap<String, Vec<(String, String)>> {
        todo!("struct type registry is not compiled yet")
    }

    fn background_event_tx(&self) -> mpsc::Sender<Event> {
        todo!("event infrastructure is not compiled yet")
    }

    fn active_http_servers(&self) -> &Arc<AtomicU64> {
        todo!("Http.serve is not compiled yet")
    }

    fn register_closure(&mut self, _agent_name: String, _f: Value) -> u64 {
        todo!("closures are not compiled yet")
    }

    fn spawn_closure<'a>(&'a mut self, _f: Value) -> HostFuture<'a, u64> {
        todo!("Async.spawn is not compiled yet")
    }

    fn insert_global(&mut self, _name: String, _value: Value) {
        todo!("prelude installation is not compiled yet")
    }

    fn register_namespace(&mut self, ns: Namespace) {
        self.namespaces.insert(ns.name.clone(), ns);
    }

    fn register_top_fn(&mut self, _name: &str, _f: BuiltinFn) {
        todo!("prelude installation is not compiled yet")
    }
}
