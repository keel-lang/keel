use std::sync::Arc;
use std::sync::atomic::Ordering;

use miette::{NamedSource, Result};

use crate::ast::{Decl, Program};

use super::environment::Environment;
use super::state::{CallArgValue, Event, Interpreter};
use super::value::Value;

pub async fn run_with_source(
    program: Program,
    source: Option<NamedSource<String>>,
    source_path: Option<&std::path::Path>,
) -> Result<()> {
    run_with_source_and_runtime(
        program,
        source,
        source_path,
        crate::runtime::context::RuntimeContext::native(),
    )
    .await
}

pub async fn run_with_source_and_runtime(
    program: Program,
    source: Option<NamedSource<String>>,
    source_path: Option<&std::path::Path>,
    runtime: Arc<crate::runtime::context::RuntimeContext>,
) -> Result<()> {
    let mut interp = Interpreter::with_runtime(runtime);
    if let Some(path) = source_path {
        let raw = path.to_str().unwrap_or("__inline__");
        interp.program_name =
            crate::runtime::derive_program_name_with_fs(raw, interp.runtime.file_system.as_ref());
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
        let oneshot = self.runtime.env.var("KEEL_ONESHOT").is_some();
        let idle_budget = std::time::Duration::from_millis(250);
        let mut rx = self
            .event_rx
            .take()
            .expect("Interpreter::execute called twice");

        loop {
            let no_agents = self.live_agents.lock().is_empty();
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
                    self.call_scheduled_closure(&agent_name, closure_id).await?;
                }
                Event::Dispatch {
                    agent_name,
                    event,
                    data,
                } => {
                    self.call_event_handler(&agent_name, &event, data).await?;
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
                        let resp_val = result.unwrap_or_else(|err| {
                            eprintln!("[keel] HTTP handler error: {err}");
                            Value::None
                        });
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
}
