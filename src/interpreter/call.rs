use miette::Result;

use crate::ast::{AttributeBody, LambdaBody, LambdaParam, TaskDecl};

use super::bind_value;
use super::environment::Environment;
use super::promote::promote_value;
use super::runtime_error;
use super::state::{AllowedTools, CallArgValue, Interpreter};
use super::stmt::{ExprFlow, StmtOutcome};
use super::value::Value;

impl Interpreter {
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

    pub(crate) async fn call_task(
        &mut self,
        _name: &str,
        decl: &TaskDecl,
        args: Vec<CallArgValue>,
    ) -> Result<Value> {
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
        // __global is always allowed — it holds agent lifecycle builtins (run, stop).
        if ns_name != "__global"
            && let Some(agent_mutex) = &self.current_agent
        {
            let allowed = agent_mutex
                .lock()
                .allowed_tools
                .as_ref()
                .map(|a| a.allows(ns_name, method));
            if allowed == Some(false) {
                return Err(runtime_error(format!(
                    "CapabilityError: `{ns_name}.{method}` is not allowed by @tools"
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
