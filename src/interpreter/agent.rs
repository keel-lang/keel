use std::collections::HashMap;
use std::sync::Arc;

use miette::Result;
use parking_lot::Mutex;

use crate::ast::AttributeBody;

use super::environment::Environment;
use super::runtime_error;
use super::state::{AgentInstance, CallArgValue, Interpreter};
use super::stmt::ExprFlow;
use super::value::Value;

impl Interpreter {
    pub(crate) async fn call_current_agent_task(
        &mut self,
        task_name: &str,
        args: Vec<CallArgValue>,
    ) -> Result<Value> {
        let current = self.current_agent.as_ref().cloned().ok_or_else(|| {
            runtime_error(format!(
                "`self.{task_name}(...)` used outside an agent context"
            ))
        })?;
        let task = {
            let inst = current.lock();
            inst.def
                .tasks
                .iter()
                .find(|task| task.name == task_name)
                .cloned()
                .ok_or_else(|| {
                    runtime_error(format!(
                        "Agent `{}` has no task `{task_name}`",
                        inst.def.name
                    ))
                })?
        };
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
            let default = match self.eval_expr(&f.default, &mut tmp_env).await? {
                ExprFlow::Value(v) => v,
                ExprFlow::Return(v) => v,
            };
            state.insert(f.name.clone(), default);
        }
        let inst = Arc::new(Mutex::new(AgentInstance {
            def: def.clone(),
            state,
            allowed_tools: None,
        }));
        self.live_agents
            .lock()
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
            self.evaluate_tools_for_turn().await?;
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
            .get(agent_name)
            .map(|inst| inst.lock().def.clone());
        if let Some(def) = def {
            let on_stop = def.attributes.iter().find(|a| a.name == "on_stop").cloned();
            if let Some(attr) = on_stop
                && let AttributeBody::Block(body) = attr.body
            {
                let inst = self.live_agents.lock().get(agent_name).cloned();
                let prev = self.current_agent.take();
                self.current_agent = inst;
                let mut env = Environment::new();
                self.exec_block(&body, &mut env).await?;
                self.current_agent = prev;
            }
        }
        self.live_agents.lock().remove(agent_name);
        Ok(())
    }
}
