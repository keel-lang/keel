use miette::Result;

use crate::ast::{AttributeBody, Expr, LambdaBody, LambdaParam, TaskDecl};

use super::bind_value;
use super::environment::Environment;
use super::runtime_error;
use super::state::{AgentDef, AllowedTools, CallArgValue, Interpreter};
use super::stmt::StmtOutcome;
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
            LambdaBody::Expr(e) => {
                let v = self.eval_expr(e, &mut env).await?;
                if let Value::EarlyReturn(inner) = v {
                    Ok(*inner)
                } else {
                    Ok(v)
                }
            }
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
        let positional: Vec<&Value> = args
            .iter()
            .filter(|a| a.name.is_none())
            .map(|a| &a.value)
            .collect();
        let named: std::collections::HashMap<&str, &Value> = args
            .iter()
            .filter_map(|a| a.name.as_deref().map(|n| (n, &a.value)))
            .collect();
        let mut pos_idx = 0;
        for p in &decl.params {
            // Only simple `Binding::Ident` params can be matched by name;
            // destructuring params (`{a, b}`) fall back to positional only.
            let param_name = match &p.name {
                crate::ast::Binding::Ident(s) => Some(s.as_str()),
                _ => None,
            };
            let v = if let Some(name) = param_name
                && let Some(v) = named.get(name)
            {
                (*v).clone()
            } else if let Some(v) = positional.get(pos_idx) {
                pos_idx += 1;
                (*v).clone()
            } else {
                Value::None
            };
            bind_value(&p.name, v, &mut env)?;
        }
        match self.exec_block(&decl.body, &mut env).await? {
            StmtOutcome::Value(v) | StmtOutcome::Return(v) => Ok(v),
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
        if let Some(agent_mutex) = &self.current_agent {
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
                    matches!(self.eval_expr(cond, &mut env).await?, Value::Bool(true))
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

    /// Extract @limits from an agent's attributes.
    /// Returns a map with timeout (seconds as f64), max_tokens (i64), max_cost (f64).
    pub fn agent_limits(
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
}
