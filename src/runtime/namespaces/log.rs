use crate::interpreter::value::Value;
use crate::interpreter::{Interpreter, Namespace};
use crate::runtime::args::expect_str;
use crate::runtime::context;
use crate::runtime::namespace::{ns, positional};

fn log_if_enabled(interp: &Interpreter, level: &str, msg: &str) {
    let call_rank = context::log_level_rank(level).unwrap_or(1);
    if call_rank >= interp.runtime.current_log_threshold() {
        eprintln!("[{level}] {msg}");
    }
}

pub(crate) fn namespace() -> Namespace {
    ns!("Log", {
        "info" => |interp, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            log_if_enabled(interp, "info", &msg);
            Ok(Value::None)
        }),
        "warn" => |interp, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            log_if_enabled(interp, "warn", &msg);
            Ok(Value::None)
        }),
        "error" => |interp, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            log_if_enabled(interp, "error", &msg);
            Ok(Value::None)
        }),
        "debug" => |interp, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            log_if_enabled(interp, "debug", &msg);
            Ok(Value::None)
        }),
        "set_level" => |interp, args| Box::pin(async move {
            let level = expect_str(&args, 0, "Log.set_level")?;
            if !interp.runtime.set_log_threshold(level) {
                return Err(miette::miette!(
                    "Log.set_level: `{level}` is not a valid level (expected debug|info|warn|error)"
                ));
            }
            Ok(Value::None)
        }),
        "level" => |interp, _args| Box::pin(async move {
            Ok(Value::String(
                context::log_level_name(interp.runtime.current_log_threshold()).to_string(),
            ))
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::interpreter::{CallArgValue, Interpreter};
    use crate::runtime::context::{MapEnv, NativeClock, NativeFileSystem, RuntimeContext};

    fn interp_default_log() -> Interpreter {
        let ctx = RuntimeContext::test_context(
            Arc::new(MapEnv::with(&[])),
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
    fn namespace_has_all_methods() {
        let ns = namespace();
        assert_eq!(ns.name, "Log");
        assert!(ns.methods.contains_key("info"));
        assert!(ns.methods.contains_key("warn"));
        assert!(ns.methods.contains_key("error"));
        assert!(ns.methods.contains_key("debug"));
        assert!(ns.methods.contains_key("set_level"));
        assert!(ns.methods.contains_key("level"));
    }

    #[test]
    fn log_level_rank_returns_known_levels() {
        assert_eq!(context::log_level_rank("debug"), Some(0));
        assert_eq!(context::log_level_rank("info"), Some(1));
        assert_eq!(context::log_level_rank("warn"), Some(2));
        assert_eq!(context::log_level_rank("WARNING"), Some(2));
        assert_eq!(context::log_level_rank("error"), Some(3));
        assert_eq!(context::log_level_rank("unknown"), None);
    }

    #[test]
    fn log_level_name_maps_back() {
        assert_eq!(context::log_level_name(0), "debug");
        assert_eq!(context::log_level_name(1), "info");
        assert_eq!(context::log_level_name(2), "warn");
        assert_eq!(context::log_level_name(3), "error");
    }

    #[tokio::test]
    async fn info_logs_without_error() {
        let ns = namespace();
        let mut interp = interp_default_log();
        let method = ns.methods.get("info").unwrap();
        let result = method(&mut interp, vec![arg(Value::String("test".into()))]).await;
        assert_eq!(result.unwrap(), Value::None);
    }

    #[tokio::test]
    async fn level_returns_current_threshold() {
        let ns = namespace();
        let mut interp = interp_default_log();
        let method = ns.methods.get("level").unwrap();
        let result = method(&mut interp, vec![]).await;
        assert_eq!(result.unwrap(), Value::String("info".into()));
    }

    #[tokio::test]
    async fn set_level_changes_threshold() {
        let ns = namespace();
        let mut interp = interp_default_log();
        let method = ns.methods.get("set_level").unwrap();
        let result = method(&mut interp, vec![arg(Value::String("debug".into()))]).await;
        assert!(result.is_ok());
        let level_method = ns.methods.get("level").unwrap();
        let level_result = level_method(&mut interp, vec![]).await;
        assert_eq!(level_result.unwrap(), Value::String("debug".into()));
    }

    #[tokio::test]
    async fn set_level_rejects_invalid() {
        let ns = namespace();
        let mut interp = interp_default_log();
        let method = ns.methods.get("set_level").unwrap();
        let result = method(&mut interp, vec![arg(Value::String("invalid".into()))]).await;
        assert!(result.is_err());
    }
}
