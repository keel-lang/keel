use crate::builtins::{BuiltinMethod, BuiltinResult, TySpec};
use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::namespace::{ns, positional};

pub(crate) const SPEC: &[BuiltinMethod] = &[BuiltinMethod {
    namespace: "testing",
    name: "mock",
    params: &[],
    result: BuiltinResult::Fixed(TySpec::Unknown),
    doc: "Create a test-local mock handle for a namespace method.",
}];

pub(crate) fn namespace() -> Namespace {
    ns!("testing", {
        "mock" => |_host, args| Box::pin(async move {
            let target = positional(&args, 0)
                .ok_or_else(|| miette::miette!("testing.mock: missing target"))?;
            match target {
                Value::MockTarget { namespace, method } => Ok(Value::MockHandle {
                    namespace: namespace.clone(),
                    method: method.clone(),
                }),
                other => Err(miette::miette!(
                    "testing.mock: expected a namespace method target, got {}",
                    other.type_name()
                )),
            }
        }),
    })
}
