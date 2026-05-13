use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::namespace::{ns, positional};

pub(crate) fn namespace() -> Namespace {
    ns!("Env", {
        "get" => |interp, args| Box::pin(async move {
            let name = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            match interp.runtime.env.var(&name) {
                Some(v) => Ok(Value::String(v)),
                None => Ok(Value::None),
            }
        }),
        "require" => |interp, args| Box::pin(async move {
            let name = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            match interp.runtime.env.var(&name) {
                Some(v) => Ok(Value::String(v)),
                None => Err(miette::miette!("Env.require: `{name}` is not set")),
            }
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::interpreter::{CallArgValue, Interpreter};
    use crate::runtime::context::{MapEnv, NativeClock, NativeFileSystem, RuntimeContext};

    fn interp_with_env(values: &[(&str, &str)]) -> Interpreter {
        let ctx = RuntimeContext::test_context(
            Arc::new(MapEnv::with(values)),
            Arc::new(NativeClock),
            Arc::new(NativeFileSystem),
        );
        Interpreter::with_runtime(ctx)
    }

    fn arg(v: Value) -> CallArgValue {
        CallArgValue {
            name: None,
            value: v,
        }
    }

    #[test]
    fn namespace_has_get_and_require() {
        let ns = namespace();
        assert_eq!(ns.name, "Env");
        assert!(ns.methods.contains_key("get"));
        assert!(ns.methods.contains_key("require"));
    }

    #[tokio::test]
    async fn get_returns_value_when_set() {
        let ns = namespace();
        let mut interp = interp_with_env(&[("HOME", "/home/user")]);
        let method = ns.methods.get("get").unwrap();
        let result = method(&mut interp, vec![arg(Value::String("HOME".into()))]).await;
        assert_eq!(result.unwrap(), Value::String("/home/user".into()));
    }

    #[tokio::test]
    async fn get_returns_none_when_unset() {
        let ns = namespace();
        let mut interp = interp_with_env(&[]);
        let method = ns.methods.get("get").unwrap();
        let result = method(&mut interp, vec![arg(Value::String("MISSING".into()))]).await;
        assert_eq!(result.unwrap(), Value::None);
    }

    #[tokio::test]
    async fn require_returns_value_when_set() {
        let ns = namespace();
        let mut interp = interp_with_env(&[("USER", "alice")]);
        let method = ns.methods.get("require").unwrap();
        let result = method(&mut interp, vec![arg(Value::String("USER".into()))]).await;
        assert_eq!(result.unwrap(), Value::String("alice".into()));
    }

    #[tokio::test]
    async fn require_errors_when_unset() {
        let ns = namespace();
        let mut interp = interp_with_env(&[]);
        let method = ns.methods.get("require").unwrap();
        let result = method(&mut interp, vec![arg(Value::String("MISSING".into()))]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("is not set"));
    }
}
