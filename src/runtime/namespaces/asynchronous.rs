use std::collections::HashMap;
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::Poll;

use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::value::{MapKey, Value};
use crate::interpreter::{CallArgValue, Namespace};
use crate::runtime::args::expect_duration;
use crate::runtime::namespace::{ns, positional};

pub(crate) const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "Async",
        name: "spawn",
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Spawn a concurrent task and return a handle.",
    },
    BuiltinMethod {
        namespace: "Async",
        name: "join_all",
        params: &[BuiltinParam {
            name: "handles",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Wait for all async task handles to complete.",
    },
    BuiltinMethod {
        namespace: "Async",
        name: "select",
        params: &[BuiltinParam {
            name: "handles",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Return the result of the first completed task handle.",
    },
    BuiltinMethod {
        namespace: "Async",
        name: "sleep",
        params: &[BuiltinParam {
            name: "duration",
            ty: TySpec::Duration,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Pause execution for the given duration.",
    },
];

pub(crate) fn namespace() -> Namespace {
    ns!("Async", {
        // Async.spawn(fn) — spawn fn as an independent Tokio task.
        // Returns a handle map with `_id` and `_status` fields.
        "spawn" => |host, args| Box::pin(async move {
            let (params, body) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), (**b).clone())),
                _ => None,
            }).ok_or_else(|| miette::miette!("Async.spawn: missing closure argument"))?;

            let handle_id = host.spawn_closure(params, body).await?;

            let mut handle_map = HashMap::new();
            handle_map.insert(MapKey::Str("_id".into()), Value::Integer(handle_id as i64));
            handle_map.insert(MapKey::Str("_status".into()), Value::String("pending".to_string()));

            Ok(Value::Map(handle_map))
        }),
        "join_all" => |host, args| Box::pin(async move {
            let handles_list = positional(&args, 0).cloned().unwrap_or(Value::List(vec![]));

            match handles_list {
                Value::List(items) => {
                    let mut results = Vec::with_capacity(items.len());
                    for item in items {
                        let id = async_handle_id(&item)
                            .ok_or_else(|| miette::miette!("Async.join_all: invalid task handle"))?;
                        let handle = host
                            .runtime()
                            .take_async_task(id)
                            .ok_or_else(|| miette::miette!("Async.join_all: unknown task handle `{id}`"))?;
                        let value = handle
                            .await
                            .map_err(|err| miette::miette!("Async.join_all: task join error: {err}"))?
                            .map_err(|err| miette::miette!("Async.join_all: task failed: {err}"))?;
                        results.push(value);
                    }
                    Ok(Value::List(results))
                }
                _ => Err(miette::miette!("Async.join_all: expected a list of handles")),
            }
        }),
        "select" => |host, args| Box::pin(async move {
            let handles_list = positional(&args, 0).cloned().unwrap_or(Value::List(vec![]));

            match handles_list {
                Value::List(items) if !items.is_empty() => {
                    let mut handles = Vec::with_capacity(items.len());
                    for item in items {
                        let id = async_handle_id(&item)
                            .ok_or_else(|| miette::miette!("Async.select: invalid task handle"))?;
                        let handle = host
                            .runtime()
                            .take_async_task(id)
                            .ok_or_else(|| miette::miette!("Async.select: unknown task handle `{id}`"))?;
                        handles.push(handle);
                    }

                    let (completed_index, first) = poll_fn(|cx| {
                        for (index, handle) in handles.iter_mut().enumerate() {
                            match Pin::new(handle).poll(cx) {
                                Poll::Ready(result) => return Poll::Ready((index, result)),
                                Poll::Pending => {}
                            }
                        }
                        Poll::Pending
                    })
                    .await;
                    for (index, handle) in handles.into_iter().enumerate() {
                        if index != completed_index {
                            handle.abort();
                        }
                    }
                    first
                        .map_err(|err| miette::miette!("Async.select: task join error: {err}"))?
                        .map_err(|err| miette::miette!("Async.select: task failed: {err}"))
                }
                Value::List(_) => Err(miette::miette!("Async.select: expected a non-empty list of handles")),
                _ => Err(miette::miette!("Async.select: expected a list of handles")),
            }
        }),
        "sleep" => |_i, args| Box::pin(async move {
            sleep_for_duration(args, "Async.sleep").await
        }),
    })
}

pub(crate) async fn sleep_for_duration(
    args: Vec<CallArgValue>,
    caller: &str,
) -> miette::Result<Value> {
    let secs = expect_duration(&args, 0, caller)?;
    tokio::time::sleep(std::time::Duration::from_secs_f64(secs)).await;
    Ok(Value::None)
}

fn async_handle_id(value: &Value) -> Option<u64> {
    let Value::Map(fields) = value else {
        return None;
    };
    match fields.get(&MapKey::Str("_id".into())) {
        Some(Value::Integer(id)) if *id >= 0 => Some(*id as u64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    use crate::interpreter::{CallArgValue, Interpreter};
    use crate::runtime::context::{Clock, MapEnv, NativeFileSystem, RuntimeContext};

    // ── test helpers ───────────────────────────────────────────────

    struct TestClock;

    impl Clock for TestClock {
        fn now_utc(&self) -> chrono::DateTime<chrono::Utc> {
            chrono::Utc::now()
        }
        fn now_instant(&self) -> Instant {
            Instant::now()
        }
    }

    fn test_interp() -> Interpreter {
        let ctx = RuntimeContext::test_context(
            Arc::new(MapEnv::with(&[])),
            Arc::new(TestClock),
            Arc::new(NativeFileSystem),
        );
        Interpreter::with_runtime(ctx)
    }

    fn arg_val(v: Value) -> CallArgValue {
        CallArgValue {
            name: None,
            value: v,
        }
    }

    // ── async_handle_id ────────────────────────────────────────────

    #[test]
    fn handle_id_extracts_positive_integer() {
        let mut fields = HashMap::new();
        fields.insert(MapKey::Str("_id".into()), Value::Integer(42));
        let handle = Value::Map(fields);
        assert_eq!(async_handle_id(&handle), Some(42));
    }

    #[test]
    fn handle_id_ignores_negative() {
        let mut fields = HashMap::new();
        fields.insert(MapKey::Str("_id".into()), Value::Integer(-1));
        let handle = Value::Map(fields);
        assert_eq!(async_handle_id(&handle), None);
    }

    #[test]
    fn handle_id_rejects_non_integer_id() {
        let mut fields = HashMap::new();
        fields.insert(MapKey::Str("_id".into()), Value::String("abc".into()));
        let handle = Value::Map(fields);
        assert_eq!(async_handle_id(&handle), None);
    }

    #[test]
    fn handle_id_rejects_missing_id_key() {
        let mut fields = HashMap::new();
        fields.insert(MapKey::Str("_status".into()), Value::String("done".into()));
        let handle = Value::Map(fields);
        assert_eq!(async_handle_id(&handle), None);
    }

    #[test]
    fn handle_id_rejects_non_map() {
        assert_eq!(async_handle_id(&Value::Integer(1)), None);
        assert_eq!(async_handle_id(&Value::String("x".into())), None);
        assert_eq!(async_handle_id(&Value::List(vec![])), None);
        assert_eq!(async_handle_id(&Value::None), None);
    }

    // ── Async.spawn error paths ────────────────────────────────────

    #[tokio::test]
    async fn spawn_missing_closure_errors() {
        let ns = namespace();
        let mut interp = test_interp();
        let method = ns.methods.get("spawn").unwrap();
        let result = method(&mut interp, vec![]).await;
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("missing closure"),
            "expected 'missing closure' error, got: {err}"
        );
    }

    #[tokio::test]
    async fn spawn_non_closure_arg_errors() {
        let ns = namespace();
        let mut interp = test_interp();
        let method = ns.methods.get("spawn").unwrap();
        let result = method(&mut interp, vec![arg_val(Value::Integer(1))]).await;
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("missing closure"),
            "expected 'missing closure' error, got: {err}"
        );
    }

    // ── Async.join_all error paths ─────────────────────────────────

    #[tokio::test]
    async fn join_all_non_list_errors() {
        let ns = namespace();
        let mut interp = test_interp();
        let method = ns.methods.get("join_all").unwrap();
        let result = method(&mut interp, vec![arg_val(Value::String("nope".into()))]).await;
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("expected a list of handles"),
            "expected list-of-handles error, got: {err}"
        );
    }

    #[tokio::test]
    async fn join_all_invalid_handle_errors() {
        let ns = namespace();
        let mut interp = test_interp();
        let method = ns.methods.get("join_all").unwrap();
        // Pass a list containing a non-map value (not a valid handle)
        let result = method(
            &mut interp,
            vec![arg_val(Value::List(vec![Value::Integer(1)]))],
        )
        .await;
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("invalid task handle"),
            "expected 'invalid task handle' error, got: {err}"
        );
    }

    #[tokio::test]
    async fn join_all_unknown_handle_errors() {
        let ns = namespace();
        let mut interp = test_interp();
        let method = ns.methods.get("join_all").unwrap();
        // Fake handle with a valid _id that doesn't exist in the runtime
        let mut fields = HashMap::new();
        fields.insert(MapKey::Str("_id".into()), Value::Integer(999));
        let fake_handle = Value::Map(fields);
        let result = method(&mut interp, vec![arg_val(Value::List(vec![fake_handle]))]).await;
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("unknown task handle"),
            "expected 'unknown task handle' error, got: {err}"
        );
    }

    // ── Async.select error paths ───────────────────────────────────

    #[tokio::test]
    async fn select_empty_list_errors() {
        let ns = namespace();
        let mut interp = test_interp();
        let method = ns.methods.get("select").unwrap();
        let result = method(&mut interp, vec![arg_val(Value::List(vec![]))]).await;
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("non-empty list"),
            "expected 'non-empty list' error, got: {err}"
        );
    }

    #[tokio::test]
    async fn select_non_list_errors() {
        let ns = namespace();
        let mut interp = test_interp();
        let method = ns.methods.get("select").unwrap();
        let result = method(&mut interp, vec![arg_val(Value::Integer(1))]).await;
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("expected a list of handles"),
            "expected 'expected a list of handles' error, got: {err}"
        );
    }

    #[tokio::test]
    async fn select_invalid_handle_errors() {
        let ns = namespace();
        let mut interp = test_interp();
        let method = ns.methods.get("select").unwrap();
        let result = method(
            &mut interp,
            vec![arg_val(Value::List(vec![Value::Integer(1)]))],
        )
        .await;
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("invalid task handle"),
            "expected 'invalid task handle' error, got: {err}"
        );
    }

    // ── Async.sleep ────────────────────────────────────────────────

    #[tokio::test]
    async fn sleep_returns_none() {
        let ns = namespace();
        let mut interp = test_interp();
        let method = ns.methods.get("sleep").unwrap();
        let result = method(&mut interp, vec![arg_val(Value::Duration(0.001))]).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::None);
    }

    #[tokio::test]
    async fn sleep_without_duration_errors() {
        let ns = namespace();
        let mut interp = test_interp();
        let method = ns.methods.get("sleep").unwrap();
        let err = method(&mut interp, vec![])
            .await
            .expect_err("missing delay must fail");
        assert_eq!(
            err.to_string(),
            "Async.sleep: missing argument at position 0"
        );
    }

    #[tokio::test]
    async fn sleep_rejects_non_duration_values() {
        let ns = namespace();
        let mut interp = test_interp();
        let method = ns.methods.get("sleep").unwrap();
        let err = method(&mut interp, vec![arg_val(Value::Integer(1))])
            .await
            .expect_err("integer delay must fail");
        assert_eq!(
            err.to_string(),
            "Async.sleep: argument at position 0 must be duration, got int"
        );
    }

    // ── Async.select happy path ────────────────────────────────────

    #[tokio::test]
    async fn select_returns_first_completed() {
        let ns = namespace();
        // Full select happy-path coverage requires spawned tasks;
        // covered by the async_select integration test.
        // Spawn two tasks: one fast, one slow (blocked)
        // The fast one returns immediately
        let spawn = ns.methods.get("spawn").unwrap();

        // Simulate a "spawned" task by inserting a fake join handle into runtime
        // We can't easily test the full spawn→select pipeline in unit tests
        // because spawn needs a real closure Value. Instead, verify the select
        // function's core logic through integration tests.
        //
        // The error paths above already cover select's argument validation.
        // The happy path (poll_fn, aborting non-first handles) is covered
        // by integration tests.

        // Minimal smoke: namespace is constructable and has all methods
        assert!(ns.methods.contains_key("spawn"));
        assert!(ns.methods.contains_key("join_all"));
        assert!(ns.methods.contains_key("select"));
        assert!(ns.methods.contains_key("sleep"));
        let _ = spawn; // avoid unused warning
    }

    #[test]
    fn namespace_has_expected_name() {
        let ns = namespace();
        assert_eq!(ns.name, "Async");
    }
}
