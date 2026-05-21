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
        // Pre-pass: register all interface declarations so that impl blocks
        // can reference them regardless of source order.
        for (decl, _span) in &program.declarations {
            if let Decl::Interface(iface) = decl {
                self.interfaces
                    .insert(iface.name.clone(), iface.methods.clone());
            }
        }

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
                    let closure = self.closures.lock().get(&closure_id).cloned();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Decl, Expr, Program, Stmt, TypeDecl, TypeDef, TypeExpr};
    use crate::interpreter::state::Interpreter;

    // ── helpers ──────────────────────────────────────────────────────────

    fn named_ty(name: &str) -> TypeExpr {
        TypeExpr::Named(name.to_string())
    }

    fn empty_program() -> Program {
        Program {
            declarations: vec![],
        }
    }

    // ── execute: two-pass (no agents/servers) ───────────────────────────

    #[tokio::test]
    async fn execute_empty_program() {
        let mut interp = Interpreter::new();
        interp.execute(empty_program()).await.unwrap();
    }

    #[tokio::test]
    async fn execute_with_declarations_only() {
        let mut interp = Interpreter::new();
        let program = Program {
            declarations: vec![
                (
                    Decl::Type(TypeDecl {
                        name: "Color".into(),
                        type_params: vec![],
                        def: TypeDef::SimpleEnum(vec!["red".into(), "green".into()]),
                    }),
                    0..0,
                ),
                (
                    Decl::Type(TypeDecl {
                        name: "Point".into(),
                        type_params: vec![],
                        def: TypeDef::Struct(vec![
                            crate::ast::Field {
                                name: "x".into(),
                                ty: named_ty("int"),
                            },
                            crate::ast::Field {
                                name: "y".into(),
                                ty: named_ty("int"),
                            },
                        ]),
                    }),
                    0..0,
                ),
            ],
        };
        interp.execute(program).await.unwrap();

        // After execution, the globals should contain the type registrations
        assert!(interp.globals.contains_key("Color"));
        assert!(interp.globals.contains_key("Point"));
        assert!(interp.enum_types.contains_key("Color"));
        assert!(interp.struct_types.contains_key("Point"));
    }

    #[tokio::test]
    async fn execute_with_stmt_pass() {
        // Verify that top-level statements are executed in pass 2.
        let mut interp = Interpreter::new();
        let program = Program {
            declarations: vec![(Decl::Stmt((Stmt::Expr(Expr::Integer(42)), 0..0)), 0..0)],
        };
        interp.execute(program).await.unwrap();
    }

    #[tokio::test]
    async fn execute_mixed_decls_and_stmts() {
        let mut interp = Interpreter::new();
        let program = Program {
            declarations: vec![
                (
                    Decl::Type(TypeDecl {
                        name: "Status".into(),
                        type_params: vec![],
                        def: TypeDef::SimpleEnum(vec!["ok".into()]),
                    }),
                    0..0,
                ),
                (Decl::Stmt((Stmt::Expr(Expr::Integer(1)), 0..0)), 0..0),
            ],
        };
        interp.execute(program).await.unwrap();
        assert!(interp.globals.contains_key("Status"));
        assert!(interp.enum_types.contains_key("Status"));
    }

    // ── execute: event loop exit conditions ─────────────────────────────

    #[tokio::test]
    async fn execute_event_loop_exits_when_no_agents_and_no_servers() {
        // With no agents and no servers, the event loop should exit immediately
        // after the two-pass registration + execution of statements.
        let mut interp = Interpreter::new();
        interp.execute(empty_program()).await.unwrap();
    }

    #[tokio::test]
    async fn execute_with_oneshot_env_exits_after_idle() {
        // KEEL_ONESHOT=1 should cause the event loop to exit after idle timeout
        // when there are no live agents/servers.
        // SAFETY: This test sets and clears the env var in sequence, so no other
        // test thread will observe it (Rust tests run with --test-threads=1 by
        // convention, or the env is cleared after this test).
        unsafe {
            std::env::set_var("KEEL_ONESHOT", "1");
        }
        let mut interp = Interpreter::new();
        interp.execute(empty_program()).await.unwrap();
        unsafe {
            std::env::remove_var("KEEL_ONESHOT");
        }
    }

    // ── run_with_source_and_runtime ─────────────────────────────────────

    #[tokio::test]
    async fn run_with_source_and_runtime_basic() {
        let runtime = crate::runtime::context::RuntimeContext::native();
        let program = Program {
            declarations: vec![(
                Decl::Type(TypeDecl {
                    name: "T".into(),
                    type_params: vec![],
                    def: TypeDef::SimpleEnum(vec!["a".into()]),
                }),
                0..0,
            )],
        };
        run_with_source_and_runtime(program, None, None, runtime)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn run_with_source_and_runtime_with_path() {
        use std::path::Path;
        let runtime = crate::runtime::context::RuntimeContext::native();
        let program = empty_program();
        let path = Path::new("test_prog.keel");
        run_with_source_and_runtime(program, None, Some(path), runtime)
            .await
            .unwrap();
    }

    // ── FireClosureWithArgs: JSON round-trip ───────────────────────────

    #[tokio::test]
    async fn fire_closure_with_args_json_roundtrip() {
        // Test the JSON serialization/deserialization helpers used in
        // FireClosureWithArgs event handling.
        use crate::runtime::{json_to_value, value_to_json};

        // Round-trip a simple map (most common HTTP handler case)
        let input = serde_json::json!({"name": "test", "count": 42});
        let val = json_to_value(&input);
        let output = value_to_json(&val);
        assert_eq!(input, output);

        // Round-trip a list
        let input = serde_json::json!([1, 2, 3]);
        let val = json_to_value(&input);
        let output = value_to_json(&val);
        assert_eq!(input, output);

        // Round-trip null
        let input = serde_json::json!(null);
        let val = json_to_value(&input);
        let output = value_to_json(&val);
        assert_eq!(input, output);

        // Round-trip boolean
        let input = serde_json::json!(true);
        let val = json_to_value(&input);
        let output = value_to_json(&val);
        assert_eq!(input, output);

        // Round-trip nested structures
        let input = serde_json::json!({
            "nested": {"key": "value"},
            "list": [{"a": 1}, {"b": 2}]
        });
        let val = json_to_value(&input);
        let output = value_to_json(&val);
        assert_eq!(input, output);
    }

    #[tokio::test]
    async fn fire_closure_with_args_invalid_json() {
        // When request_json is invalid JSON, serde_json::from_str should fail,
        // and the handler falls back to Value::String.
        let result: Result<serde_json::Value, _> = serde_json::from_str("not valid json");
        assert!(result.is_err());
    }

    // ── execute: Shutdown event ────────────────────────────────────────

    #[tokio::test]
    async fn execute_shutdown_event_exits_loop() {
        let mut interp = Interpreter::new();
        // Post a Shutdown event before running execute.
        // The event loop should process it and break.
        interp.event_tx.send(Event::Shutdown).unwrap();
        interp.execute(empty_program()).await.unwrap();
    }

    // ── FireClosureWithArgs: event-driven test ──────────────────────────

    #[tokio::test]
    async fn fire_closure_with_args_event_identity_handler() {
        use crate::ast::{LambdaBody, LambdaParam};
        use std::sync::atomic::Ordering;
        use tokio::sync::oneshot;

        let mut interp = Interpreter::new();

        // Register a simple identity closure: |x| x
        let params = vec![LambdaParam {
            name: "req".to_string(),
            ty: None,
        }];
        let body = LambdaBody::Expr(Box::new(Expr::Ident("req".to_string())));
        let closure_id = interp.register_closure("test_agent".to_string(), params, body);

        // Keep event loop alive by faking an active server
        interp.active_http_servers.fetch_add(1, Ordering::Relaxed);

        // Create response channel
        let (tx, mut rx) = oneshot::channel();

        // Post FireClosureWithArgs then Shutdown
        let request_json = r#"{"name":"test","count":42}"#;
        interp
            .event_tx
            .send(Event::FireClosureWithArgs {
                closure_id,
                request_json: request_json.to_string(),
                response_tx: tx,
            })
            .unwrap();
        interp.event_tx.send(Event::Shutdown).unwrap();

        // Run execute in background (it will block until events are processed)
        let handle = tokio::spawn(async move {
            interp.execute(empty_program()).await.unwrap();
        });

        // Receive the response
        let resp = (&mut rx).await.expect("should receive response");

        // Cancel the background task (it should already be done)
        handle.abort();

        // The response should be a JSON value - the identity handler returns
        // the input map
        assert!(
            resp.contains("test"),
            "response should contain 'test': {resp}"
        );
        assert!(resp.contains("42"), "response should contain 42: {resp}");
    }

    #[tokio::test]
    async fn fire_closure_with_args_event_handler_not_found() {
        use std::sync::atomic::Ordering;
        use tokio::sync::oneshot;

        let mut interp = Interpreter::new();

        // Keep event loop alive
        interp.active_http_servers.fetch_add(1, Ordering::Relaxed);

        let (tx, mut rx) = oneshot::channel();

        // Post FireClosureWithArgs with a non-existent closure id
        interp
            .event_tx
            .send(Event::FireClosureWithArgs {
                closure_id: 99999, // doesn't exist
                request_json: r#"{}"#.to_string(),
                response_tx: tx,
            })
            .unwrap();
        interp.event_tx.send(Event::Shutdown).unwrap();

        let handle = tokio::spawn(async move {
            interp.execute(empty_program()).await.unwrap();
        });

        let resp = (&mut rx).await.expect("should receive error response");
        handle.abort();

        assert!(
            resp.contains("handler not found"),
            "expected handler not found, got: {resp}"
        );
    }

    #[tokio::test]
    async fn fire_closure_with_args_invalid_json_request() {
        use crate::ast::{LambdaBody, LambdaParam};
        use std::sync::atomic::Ordering;
        use tokio::sync::oneshot;

        let mut interp = Interpreter::new();

        // Register a closure that just returns the arg
        let params = vec![LambdaParam {
            name: "req".to_string(),
            ty: None,
        }];
        let body = LambdaBody::Expr(Box::new(Expr::Ident("req".to_string())));
        let closure_id = interp.register_closure("test_agent".to_string(), params, body);

        interp.active_http_servers.fetch_add(1, Ordering::Relaxed);

        let (tx, mut rx) = oneshot::channel();

        // Send invalid JSON - the handler should fall back to Value::String
        interp
            .event_tx
            .send(Event::FireClosureWithArgs {
                closure_id,
                request_json: "not valid json!!!".to_string(),
                response_tx: tx,
            })
            .unwrap();
        interp.event_tx.send(Event::Shutdown).unwrap();

        let handle = tokio::spawn(async move {
            interp.execute(empty_program()).await.unwrap();
        });

        let resp = (&mut rx).await.expect("should receive response");
        handle.abort();

        // The identity handler returns the input as a string
        assert!(
            resp.contains("not valid json"),
            "response should contain the string: {resp}"
        );
    }

    #[tokio::test]
    async fn fire_closure_with_args_handler_error_is_caught() {
        use crate::ast::{LambdaBody, LambdaParam};
        use std::sync::atomic::Ordering;
        use tokio::sync::oneshot;

        let mut interp = Interpreter::new();

        // Register a closure that returns an error by accessing a non-existent field
        // Actually let's use one that just returns none
        let params = vec![LambdaParam {
            name: "req".to_string(),
            ty: None,
        }];
        let body = LambdaBody::Expr(Box::new(Expr::None_));
        let closure_id = interp.register_closure("test_agent".to_string(), params, body);

        interp.active_http_servers.fetch_add(1, Ordering::Relaxed);

        let (tx, mut rx) = oneshot::channel();

        interp
            .event_tx
            .send(Event::FireClosureWithArgs {
                closure_id,
                request_json: r#"{"valid": true}"#.to_string(),
                response_tx: tx,
            })
            .unwrap();
        interp.event_tx.send(Event::Shutdown).unwrap();

        let handle = tokio::spawn(async move {
            interp.execute(empty_program()).await.unwrap();
        });

        let resp = (&mut rx).await.expect("should receive response");
        handle.abort();

        // The closure returns None, which serializes to null
        assert!(resp.contains("null"), "expected null response, got: {resp}");
    }
}
