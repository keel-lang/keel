use miette::Result;

use crate::ast::{AttributeBody, LambdaBody, LambdaParam, TaskDecl};

use super::bind_value;
use super::debug_hook::{FrameInfo, SourceLocation};
use super::environment::Environment;
use super::promote::promote_value;
use super::state::{AllowedTools, CallArgValue, Interpreter, RecordedMockCall};
use super::stmt::{ExprFlow, StmtOutcome};
use super::value::Value;
use super::{RuntimeErrorKind, runtime_error};
use crate::runtime::namespace::make_typed_report;

impl Interpreter {
    /// Invoke a closure. Inherits `current_module_id` from the caller rather
    /// than tracking a home module of its own — see `Value::Closure`'s doc
    /// comment for why (a closure invoked right where it's defined, the
    /// overwhelmingly common case, is already correct by inheritance; only a
    /// closure passed across a module boundary into a std callback can be
    /// misattributed, a documented D0 limitation).
    pub async fn call_closure(
        &mut self,
        params: &[LambdaParam],
        body: &LambdaBody,
        args: Vec<CallArgValue>,
    ) -> Result<Value> {
        self.call_depth += 1;
        if self.debug_active {
            self.debug_hook.on_call_enter(FrameInfo {
                name: "<closure>".to_string(),
                location: SourceLocation {
                    module_id: self.current_module_id,
                    span: 0..0,
                },
            });
        }
        let result = self.call_closure_inner(params, body, args).await;
        if self.debug_active {
            self.debug_hook.on_call_exit();
        }
        self.call_depth -= 1;
        result
    }

    async fn call_closure_inner(
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
            LambdaBody::Expr(e) => match self.eval_expr(e, &mut env).await? {
                ExprFlow::Value(v) | ExprFlow::Return(v) => Ok(v),
            },
            LambdaBody::Block(block) => match self.exec_block(block, &mut env).await? {
                StmtOutcome::Value(v) | StmtOutcome::Return(v) => Ok(v),
                StmtOutcome::Normal => Ok(Value::None),
                StmtOutcome::Break | StmtOutcome::Continue => {
                    Err(super::runtime_error("`break`/`continue` outside a loop"))
                }
            },
        }
    }

    /// Invoke a top-level task or impl method. `name` is looked up in
    /// `task_module` to switch `current_module_id` for the call's duration;
    /// an impl method or agent-internal task (absent from that table) simply
    /// inherits whatever module is already current.
    pub(crate) async fn call_task(
        &mut self,
        name: &str,
        decl: &TaskDecl,
        args: Vec<CallArgValue>,
    ) -> Result<Value> {
        let prev_module = self.current_module_id;
        if let Some(&module_id) = self.task_module.get(name) {
            self.current_module_id = module_id;
        }
        self.call_depth += 1;
        if self.debug_active {
            self.debug_hook.on_call_enter(FrameInfo {
                name: name.to_string(),
                location: SourceLocation {
                    module_id: self.current_module_id,
                    span: decl.name_span.clone(),
                },
            });
        }
        let result = self.call_task_inner(decl, args).await;
        if self.debug_active {
            self.debug_hook.on_call_exit();
        }
        self.call_depth -= 1;
        self.current_module_id = prev_module;
        result
    }

    async fn call_task_inner(&mut self, decl: &TaskDecl, args: Vec<CallArgValue>) -> Result<Value> {
        let mut env = Environment::new();
        // Positional args (no label) fill params in order; named args
        // (label matches param name) bind by name and are skipped in the
        // positional sequence. Mixed calls like `foo(1, b: 20, 3)` work
        // the same way as Python keyword arguments.
        // Spread args have already been flattened into positional slots by eval_args.
        let positional: Vec<&Value> = args
            .iter()
            .filter(|a| a.name.is_none())
            .map(|a| &a.value)
            .collect();
        let named: std::collections::HashMap<&str, &Value> = args
            .iter()
            .filter_map(|a| a.name.as_deref().map(|n| (n, &a.value)))
            .collect();

        let variadic_idx = decl
            .params
            .iter()
            .position(|p| p.variadic)
            .unwrap_or(decl.params.len());

        let is_variadic = variadic_idx < decl.params.len();
        if !is_variadic && positional.len() > decl.params.len() {
            return Err(miette::miette!(
                "task takes {} argument(s), got {} — spread args (`...`) require a variadic callee",
                decl.params.len(),
                positional.len()
            ));
        }

        let mut pos_idx = 0;
        for (i, p) in decl.params.iter().enumerate() {
            // Only simple `Binding::Ident` params can be matched by name;
            // destructuring params (`{a, b}`) fall back to positional only.
            let param_name = match &p.name {
                crate::ast::Binding::Ident(s) => Some(s.as_str()),
                _ => None,
            };
            if p.variadic {
                // Collect and promote remaining positional args into a list.
                let rest: Vec<Value> = positional[pos_idx..]
                    .iter()
                    .map(|v| {
                        promote_value(
                            (*v).clone(),
                            &p.ty.kind,
                            &self.struct_types,
                            &self.struct_aliases,
                        )
                    })
                    .collect();
                bind_value(&p.name, Value::List(rest), &mut env)?;
                break;
            }
            let v = if let Some(name) = param_name
                && i < variadic_idx
                && let Some(v) = named.get(name)
            {
                (*v).clone()
            } else if let Some(v) = positional.get(pos_idx) {
                pos_idx += 1;
                (*v).clone()
            } else {
                Value::None
            };
            let v = promote_value(v, &p.ty.kind, &self.struct_types, &self.struct_aliases);
            bind_value(&p.name, v, &mut env)?;
        }
        match self.exec_block(&decl.body, &mut env).await? {
            StmtOutcome::Value(v) | StmtOutcome::Return(v) => {
                let v = match &decl.return_type {
                    Some(node) => {
                        promote_value(v, &node.kind, &self.struct_types, &self.struct_aliases)
                    }
                    None => v,
                };
                Ok(v)
            }
            StmtOutcome::Normal => Ok(Value::None),
            StmtOutcome::Break | StmtOutcome::Continue => {
                Err(super::runtime_error("`break`/`continue` outside a loop"))
            }
        }
    }

    pub(crate) async fn call_namespace_method(
        &mut self,
        ns_name: &str,
        method: &str,
        args: Vec<CallArgValue>,
    ) -> Result<Value> {
        // Check @tools capability gating if we're in an agent context.
        // Capabilities guard effects: only modules that touch the world
        // outside the process are gated, deny-by-default. An agent without
        // `@tools` may not call any effectful module; `@tools all` is the
        // explicit unrestricted form. Pure-compute modules (json, math, …)
        // and __global (agent lifecycle builtins) are never gated.
        //
        // A user-authored provider's `complete()` (active_providers non-empty)
        // is trusted program infrastructure — like a built-in backend that makes
        // HTTP calls without the agent granting `@tools [http]` — so effectful
        // calls (env, http, …) run ungated regardless of the consuming agent's
        // `@tools`. The bypass covers everything reached while a provider is on
        // the stack: the `complete()` body, any task it calls, and closures it
        // spawns (which inherit `active_providers`) — all provider-authored,
        // trusted code, never the consuming agent's own statements.
        if keel_catalog::module_requires_capability(ns_name)
            && self.active_providers.is_empty()
            && let Some(agent_mutex) = &self.current_agent
        {
            let (allowed, agent_name) = {
                let agent = agent_mutex.lock();
                (
                    agent
                        .allowed_tools
                        .as_ref()
                        .is_some_and(|a| a.allows(ns_name, method)),
                    agent.def.name.clone(),
                )
            };
            if !allowed {
                return Err(make_typed_report(
                    RuntimeErrorKind::Capability,
                    format!(
                        "`{ns_name}.{method}` is not allowed by @tools on {agent_name} — \
                         add `{ns_name}` to @tools, or use `@tools all`"
                    ),
                ));
            }
        }

        let mocked = {
            let mut test_mocks = self.test_mocks.lock();
            test_mocks
                .get_mut(&(ns_name.to_string(), method.to_string()))
                .map(|state| {
                    state.calls.push(RecordedMockCall { args: args.clone() });
                    if state.returns.len() > 1 {
                        state.returns.remove(0)
                    } else {
                        state.returns.first().cloned().unwrap_or(Value::None)
                    }
                })
        };
        if let Some(mocked) = mocked {
            if ns_name == "ai" && method == "classify" {
                return self.mocked_classify_result(&args, mocked);
            }
            return Ok(mocked);
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

    fn mocked_classify_result(&self, args: &[CallArgValue], mocked: Value) -> Result<Value> {
        let expected_enum = args.iter().find_map(|arg| {
            if arg.name.as_deref() == Some("as")
                && let Value::Namespace(name) = &arg.value
            {
                return Some(name.as_str());
            }
            None
        });

        match (&mocked, expected_enum) {
            (Value::EnumVariant(actual_enum, _, _), Some(expected)) if actual_enum == expected => {
                Ok(mocked)
            }
            (Value::EnumVariant(actual_enum, _, _), Some(expected)) => Err(make_typed_report(
                RuntimeErrorKind::Ai,
                format!(
                    "mocked Ai.classify returned `{actual_enum}` but call expected `{expected}`"
                ),
            )),
            (Value::EnumVariant(_, _, _), None) => Ok(mocked),
            _ => Ok(mocked),
        }
    }

    /// Evaluate `@tools` guards for the current agent turn.
    /// Reads the `@tools [...]` entries, runs any `when` conditions, and stores
    /// the resulting `AllowedTools` on the agent instance.
    /// Must be called after `self.current_agent` is set.
    pub(crate) async fn evaluate_tools_for_turn(&mut self) -> Result<()> {
        let entries = {
            let agent = match &self.current_agent {
                Some(a) => a.lock(),
                None => return Ok(()),
            };
            let mut found = None;
            for attr in &agent.def.attributes {
                if attr.name == "tools"
                    && let AttributeBody::Tools(e) = &attr.body
                {
                    found = Some(e.clone());
                    break;
                }
            }
            found
        };

        let Some(entries) = entries else {
            // No @tools — clear any stale restriction from a previous turn.
            if let Some(a) = &self.current_agent {
                a.lock().allowed_tools = None;
            }
            return Ok(());
        };

        let mut allowed = Vec::new();
        let mut env = Environment::new();
        for entry in &entries {
            let included = match &entry.condition {
                None => true,
                Some(cond) => {
                    matches!(
                        self.eval_expr(cond, &mut env).await?,
                        ExprFlow::Value(Value::Bool(true))
                    )
                }
            };
            if included {
                allowed.push((entry.namespace.clone(), entry.method.clone()));
            }
        }

        if let Some(a) = &self.current_agent {
            a.lock().allowed_tools = Some(AllowedTools(allowed));
        }
        Ok(())
    }
}
