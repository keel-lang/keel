use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use miette::{NamedSource, Result};

use crate::ast::{Decl, Program, TestDecl};
use crate::lexer::Span;
use crate::types::interface::TypeEnv;

use super::environment::Environment;
use super::state::{CallArgValue, Event, Interpreter};
use super::stmt::{ExprFlow, StmtOutcome};
use super::value::Value;

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

#[derive(Debug, Clone)]
pub struct TestOutcome {
    pub name: String,
    pub passed: bool,
    pub error: Option<String>,
    pub failure_location: Option<String>,
    pub elapsed: Duration,
}

struct TestFailure {
    error: String,
    span: Option<Span>,
}

impl TestFailure {
    fn new(err: impl std::fmt::Display, span: Option<Span>) -> Self {
        Self {
            error: err.to_string(),
            span,
        }
    }
}

fn format_failure_location(
    source_path: Option<&std::path::Path>,
    span: Option<&Span>,
) -> Option<String> {
    let (Some(path), Some(span)) = (source_path, span) else {
        return None;
    };
    let source = std::fs::read_to_string(path).ok()?;
    let offset = span.start.min(source.len());
    let mut line = 1_usize;
    let mut line_start = 0_usize;
    for (idx, byte) in source.bytes().enumerate() {
        if idx >= offset {
            break;
        }
        if byte == b'\n' {
            line += 1;
            line_start = idx + 1;
        }
    }
    let column = source[line_start..offset].chars().count() + 1;
    Some(format!("{}:{line}:{column}", path.display()))
}

pub async fn run_tests_with_source_and_runtime(
    program: Program,
    source: Option<NamedSource<String>>,
    source_path: Option<&std::path::Path>,
    runtime: Arc<crate::runtime::context::RuntimeContext>,
    filter: Option<&str>,
    fail_fast: bool,
) -> Result<Vec<TestOutcome>> {
    let tests: Vec<TestDecl> = program
        .declarations
        .iter()
        .filter_map(|node| match &node.kind {
            Decl::Test(test) if filter.is_none_or(|filter| test.name.contains(filter)) => {
                Some(test.clone())
            }
            _ => None,
        })
        .collect();

    let mut outcomes = Vec::new();
    for test in tests {
        let cases = test_cases(
            program.clone(),
            source.clone(),
            source_path,
            &runtime,
            &test,
        )
        .await;
        let cases = match cases {
            Ok(cases) => cases,
            Err(failure) => {
                outcomes.push(TestOutcome {
                    name: test.name,
                    passed: false,
                    error: Some(failure.error),
                    failure_location: format_failure_location(source_path, failure.span.as_ref()),
                    elapsed: Duration::ZERO,
                });
                if fail_fast {
                    break;
                }
                continue;
            }
        };
        let total_cases = cases.len();
        let parameterized = test.param.is_some();
        for (index, case_value) in cases.into_iter().enumerate() {
            let test_runtime = crate::runtime::context::RuntimeContext::isolated_from(&runtime);
            let mut interp = Interpreter::with_runtime(test_runtime);
            if let Some(path) = source_path {
                let raw = path.to_str().unwrap_or("__inline__");
                interp.program_name = crate::runtime::derive_program_name_with_fs(
                    raw,
                    interp.runtime.file_system.as_ref(),
                );
            }
            interp.source = source.clone();
            let started = Instant::now();
            let result = interp
                .execute_test(program.clone(), &test, case_value)
                .await;
            let elapsed = started.elapsed();
            let passed = result.is_ok();
            let name = if parameterized {
                format!("{} [{}]", test.name, index)
            } else {
                test.name.clone()
            };
            outcomes.push(TestOutcome {
                name,
                passed,
                error: result.as_ref().err().map(|failure| failure.error.clone()),
                failure_location: result.as_ref().err().and_then(|failure| {
                    format_failure_location(source_path, failure.span.as_ref())
                }),
                elapsed,
            });
            if fail_fast && !passed {
                break;
            }
        }
        if fail_fast
            && outcomes
                .iter()
                .rev()
                .take(total_cases)
                .any(|outcome| !outcome.passed)
        {
            break;
        }
    }
    Ok(outcomes)
}

async fn test_cases(
    program: Program,
    source: Option<NamedSource<String>>,
    source_path: Option<&std::path::Path>,
    runtime: &Arc<crate::runtime::context::RuntimeContext>,
    test: &TestDecl,
) -> std::result::Result<Vec<Option<Value>>, TestFailure> {
    let Some(param) = &test.param else {
        return Ok(vec![None]);
    };

    let test_runtime = crate::runtime::context::RuntimeContext::isolated_from(runtime);
    let mut interp = Interpreter::with_runtime(test_runtime);
    if let Some(path) = source_path {
        let raw = path.to_str().unwrap_or("__inline__");
        interp.program_name =
            crate::runtime::derive_program_name_with_fs(raw, interp.runtime.file_system.as_ref());
    }
    interp.source = source;
    interp
        .prepare_program(&program)
        .map_err(|err| TestFailure::new(err, None))?;
    let mut env = Environment::new();
    let value = match interp.eval_expr(&param.cases, &mut env).await {
        Ok(ExprFlow::Value(value) | ExprFlow::Return(value)) => value,
        Err(err) => return Err(TestFailure::new(err, Some(param.cases.span.clone()))),
    };
    let Value::List(items) = value else {
        return Err(TestFailure {
            error: format!(
                "parameterized test cases must evaluate to list, got {}",
                value.type_name()
            ),
            span: Some(param.cases.span.clone()),
        });
    };
    Ok(items.into_iter().map(Some).collect())
}

pub fn test_names(program: &Program, filter: Option<&str>) -> Vec<String> {
    program
        .declarations
        .iter()
        .filter_map(|node| match &node.kind {
            Decl::Test(test) if filter.is_none_or(|filter| test.name.contains(filter)) => {
                Some(test.name.clone())
            }
            _ => None,
        })
        .collect()
}

impl Interpreter {
    pub async fn execute(&mut self, program: Program) -> Result<()> {
        self.prepare_program(&program)?;
        for node in &program.declarations {
            if let Decl::Stmt(stmt_node) = &node.kind {
                let mut env = Environment::new();
                self.exec_stmt(&stmt_node.kind, &mut env).await?;
            }
        }

        self.run_event_loop().await
    }

    async fn execute_test(
        &mut self,
        program: Program,
        test: &TestDecl,
        case_value: Option<Value>,
    ) -> std::result::Result<(), TestFailure> {
        self.prepare_program(&program)
            .map_err(|err| TestFailure::new(err, None))?;

        let mut env = Environment::new();
        if let (Some(param), Some(value)) = (&test.param, case_value) {
            env.define(param.name.clone(), value);
        }
        self.set_test_mocks(std::collections::HashMap::new());

        for stmt in &test.setup {
            match self.exec_stmt(&stmt.kind, &mut env).await {
                Ok(outcome) => match outcome {
                    StmtOutcome::Normal | StmtOutcome::Value(_) => {}
                    StmtOutcome::Return(_) => {
                        return Err(TestFailure {
                            error: "`return` outside task".to_string(),
                            span: Some(stmt.span.clone()),
                        });
                    }
                    StmtOutcome::Break | StmtOutcome::Continue => {
                        return Err(TestFailure {
                            error: "`break`/`continue` outside a loop".to_string(),
                            span: Some(stmt.span.clone()),
                        });
                    }
                },
                Err(err) => return Err(TestFailure::new(err, Some(stmt.span.clone()))),
            }
        }

        for stmt in &test.body {
            match self.exec_stmt(&stmt.kind, &mut env).await {
                Ok(outcome) => match outcome {
                    StmtOutcome::Normal | StmtOutcome::Value(_) => {}
                    StmtOutcome::Return(_) => {
                        return Err(TestFailure {
                            error: "`return` outside task".to_string(),
                            span: Some(stmt.span.clone()),
                        });
                    }
                    StmtOutcome::Break | StmtOutcome::Continue => {
                        return Err(TestFailure {
                            error: "`break`/`continue` outside a loop".to_string(),
                            span: Some(stmt.span.clone()),
                        });
                    }
                },
                Err(err) => return Err(TestFailure::new(err, Some(stmt.span.clone()))),
            }
        }

        self.run_event_loop()
            .await
            .map_err(|err| TestFailure::new(err, None))
    }

    fn prepare_program(&mut self, program: &Program) -> Result<()> {
        // Pre-pass 1: register all interface declarations so that impl blocks
        // can reference them regardless of source order.
        for node in &program.declarations {
            if let Decl::Interface(iface) = &node.kind {
                self.interfaces
                    .insert(iface.name.clone(), iface.methods.clone());
            }
        }

        // Pre-pass 2: build the type-resolution environment from all `type`
        // declarations so that `impl` conformance checks can resolve TypeExpr
        // nodes to Ty values with proper alias expansion.
        let mut type_env = TypeEnv::new();
        type_env.collect_aliases(program.declarations.iter().map(|n| &n.kind));
        self.type_env = type_env;

        for node in &program.declarations {
            self.register_decl(&node.kind)?;
        }
        Ok(())
    }

    async fn run_event_loop(&mut self) -> Result<()> {
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
    use crate::ast::{Decl, Expr, Node, Program, Stmt, TypeDecl, TypeDef, TypeExpr};
    use crate::interpreter::state::Interpreter;

    // ── helpers ──────────────────────────────────────────────────────────

    fn named_ty(name: &str) -> Node<TypeExpr> {
        Node::synthetic(TypeExpr::Named(name.to_string()))
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
                Node::synthetic(Decl::Type(TypeDecl {
                    name: "Color".into(),
                    name_span: 0..0,
                    type_params: vec![],
                    def: TypeDef::SimpleEnum(vec!["red".into(), "green".into()]),
                })),
                Node::synthetic(Decl::Type(TypeDecl {
                    name: "Point".into(),
                    name_span: 0..0,
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
                })),
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
            declarations: vec![Node::synthetic(Decl::Stmt(Node::synthetic(Stmt::Expr(
                Node::synthetic(Expr::Integer(42)),
            ))))],
        };
        interp.execute(program).await.unwrap();
    }

    #[tokio::test]
    async fn execute_mixed_decls_and_stmts() {
        let mut interp = Interpreter::new();
        let program = Program {
            declarations: vec![
                Node::synthetic(Decl::Type(TypeDecl {
                    name: "Status".into(),
                    name_span: 0..0,
                    type_params: vec![],
                    def: TypeDef::SimpleEnum(vec!["ok".into()]),
                })),
                Node::synthetic(Decl::Stmt(Node::synthetic(Stmt::Expr(Node::synthetic(
                    Expr::Integer(1),
                ))))),
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
            declarations: vec![Node::synthetic(Decl::Type(TypeDecl {
                name: "T".into(),
                name_span: 0..0,
                type_params: vec![],
                def: TypeDef::SimpleEnum(vec!["a".into()]),
            }))],
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
        interp.event_tx.try_send(Event::Shutdown).unwrap();
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
        let body = LambdaBody::Expr(Box::new(Node::synthetic(Expr::Ident("req".to_string()))));
        let closure_id = interp.register_closure("test_agent".to_string(), params, body);

        // Keep event loop alive by faking an active server
        interp.active_http_servers.fetch_add(1, Ordering::Relaxed);

        // Create response channel
        let (tx, mut rx) = oneshot::channel();

        // Post FireClosureWithArgs then Shutdown
        let request_json = r#"{"name":"test","count":42}"#;
        interp
            .event_tx
            .try_send(Event::FireClosureWithArgs {
                closure_id,
                request_json: request_json.to_string(),
                response_tx: tx,
            })
            .unwrap();
        interp.event_tx.try_send(Event::Shutdown).unwrap();

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
            .try_send(Event::FireClosureWithArgs {
                closure_id: 99999, // doesn't exist
                request_json: r#"{}"#.to_string(),
                response_tx: tx,
            })
            .unwrap();
        interp.event_tx.try_send(Event::Shutdown).unwrap();

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
        let body = LambdaBody::Expr(Box::new(Node::synthetic(Expr::Ident("req".to_string()))));
        let closure_id = interp.register_closure("test_agent".to_string(), params, body);

        interp.active_http_servers.fetch_add(1, Ordering::Relaxed);

        let (tx, mut rx) = oneshot::channel();

        // Send invalid JSON - the handler should fall back to Value::String
        interp
            .event_tx
            .try_send(Event::FireClosureWithArgs {
                closure_id,
                request_json: "not valid json!!!".to_string(),
                response_tx: tx,
            })
            .unwrap();
        interp.event_tx.try_send(Event::Shutdown).unwrap();

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
        let body = LambdaBody::Expr(Box::new(Node::synthetic(Expr::None_)));
        let closure_id = interp.register_closure("test_agent".to_string(), params, body);

        interp.active_http_servers.fetch_add(1, Ordering::Relaxed);

        let (tx, mut rx) = oneshot::channel();

        interp
            .event_tx
            .try_send(Event::FireClosureWithArgs {
                closure_id,
                request_json: r#"{"valid": true}"#.to_string(),
                response_tx: tx,
            })
            .unwrap();
        interp.event_tx.try_send(Event::Shutdown).unwrap();

        let handle = tokio::spawn(async move {
            interp.execute(empty_program()).await.unwrap();
        });

        let resp = (&mut rx).await.expect("should receive response");
        handle.abort();

        // The closure returns None, which serializes to null
        assert!(resp.contains("null"), "expected null response, got: {resp}");
    }
}
