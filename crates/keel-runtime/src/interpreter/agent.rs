use std::collections::HashMap;
use std::sync::Arc;

use miette::Result;
use parking_lot::Mutex;

use crate::ast::AttributeBody;

use super::debug_hook::{FrameInfo, SourceLocation};
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
            .store
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
            let module_id = self.agent_module.get(agent_name).copied();
            let turn = self.begin_agent_turn(Some(inst.clone()), module_id);
            if self.debug_active {
                self.debug_hook.on_call_enter(FrameInfo {
                    name: format!("{agent_name}.on_start"),
                    location: SourceLocation {
                        module_id: self.current_module_id,
                        span: 0..0,
                    },
                });
            }
            self.evaluate_tools_for_turn().await?;
            let mut env = Environment::new();
            self.exec_block(&body, &mut env).await?;
            if self.debug_active {
                self.debug_hook.on_call_exit();
            }
            self.end_agent_turn(turn);
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
                let module_id = self.agent_module.get(agent_name).copied();
                let turn = self.begin_agent_turn(inst, module_id);
                if self.debug_active {
                    self.debug_hook.on_call_enter(FrameInfo {
                        name: format!("{agent_name}.on_stop"),
                        location: SourceLocation {
                            module_id: self.current_module_id,
                            span: 0..0,
                        },
                    });
                }
                let mut env = Environment::new();
                self.exec_block(&body, &mut env).await?;
                if self.debug_active {
                    self.debug_hook.on_call_exit();
                }
                self.end_agent_turn(turn);
            }
        }
        self.live_agents.lock().remove(agent_name);
        Ok(())
    }
}
