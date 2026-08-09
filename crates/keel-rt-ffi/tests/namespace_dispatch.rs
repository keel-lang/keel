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

// ---------------------------------------------------------------------------
// `call_method_on_value` (part of #114) — same "reaches the exact same shared
// dispatch as the interpreter" property as the namespace-method tests above,
// for the `Host::call_method_on_value` extraction.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn str_upper_reaches_the_same_dispatch_the_interpreter_uses() {
    let mut host = CompiledHost::new(RuntimeContext::native());
    let result = host
        .call_method_on_value(Value::String("keel".to_string()), "upper", vec![])
        .await;
    assert_eq!(result.unwrap(), Value::String("KEEL".to_string()));
}

#[tokio::test]
async fn str_contains_with_an_argument_reaches_the_same_dispatch() {
    let mut host = CompiledHost::new(RuntimeContext::native());
    let result = host
        .call_method_on_value(
            Value::String("hello world".to_string()),
            "contains",
            vec![arg(Value::String("world".to_string()))],
        )
        .await;
    assert_eq!(result.unwrap(), Value::Bool(true));
}

#[tokio::test]
async fn unknown_value_method_is_a_catchable_error_not_a_panic() {
    let mut host = CompiledHost::new(RuntimeContext::native());
    let result = host
        .call_method_on_value(Value::String("keel".to_string()), "nope", vec![])
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn a_struct_receiver_finds_no_impl_method_yet() {
    // `CompiledHost::find_impl_task` always returns `None` today — no
    // compiled call site can construct a struct receiver with a real impl
    // method to dispatch to yet (only `Str` value-methods lower). This pins
    // that documented, currently-correct stub so it fails loudly instead of
    // silently once struct receivers do start lowering.
    let host = CompiledHost::new(RuntimeContext::native());
    assert!(
        host.find_impl_task(&Value::String("keel".to_string()), "upper")
            .is_none()
    );
}
