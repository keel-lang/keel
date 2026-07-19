//! Proves the `Host`-seam reuse in isolation from codegen/LLVM: a
//! hand-built `CompiledHost` dispatches `io.show`/`log.info` through the
//! exact same namespace closures the interpreter uses, with no LLVM
//! involved. `keel-codegen`'s own tests (issue #135) prove that the
//! generated `CallTarget::Ns` call sites actually reach this same path.

use keel_rt::CompiledHost;
use keel_runtime::interpreter::CallArgValue;
use keel_runtime::interpreter::host::Host;
use keel_runtime::interpreter::value::Value;
use keel_runtime::runtime::context::RuntimeContext;

fn arg(v: Value) -> CallArgValue {
    CallArgValue {
        name: None,
        value: v,
    }
}

#[tokio::test]
async fn io_show_reaches_the_same_closure_the_interpreter_uses() {
    let mut host = CompiledHost::new(RuntimeContext::native());
    let result = host
        .call_namespace_method("io", "show", vec![arg(Value::String("hello".to_string()))])
        .await;
    assert_eq!(result.unwrap(), Value::None);
}

#[tokio::test]
async fn log_info_reaches_the_same_closure_the_interpreter_uses() {
    let mut host = CompiledHost::new(RuntimeContext::native());
    let result = host
        .call_namespace_method(
            "log",
            "info",
            vec![arg(Value::String("started".to_string()))],
        )
        .await;
    assert_eq!(result.unwrap(), Value::None);
}

#[tokio::test]
async fn unknown_namespace_is_a_catchable_error_not_a_panic() {
    let mut host = CompiledHost::new(RuntimeContext::native());
    let result = host.call_namespace_method("nope", "show", vec![]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn unknown_method_is_a_catchable_error_not_a_panic() {
    let mut host = CompiledHost::new(RuntimeContext::native());
    let result = host.call_namespace_method("io", "nope", vec![]).await;
    assert!(result.is_err());
}
