use crate::interpreter::CallArgValue;
use crate::interpreter::value::Value;

pub(crate) fn throw_typed_error(
    interp: &mut crate::interpreter::Interpreter,
    type_name: &str,
    message: &str,
    extra: Option<(&str, String)>,
) -> miette::Result<Value> {
    let mut fields = std::collections::HashMap::new();
    fields.insert("message".to_string(), Value::String(message.to_string()));
    if let Some((key, val)) = extra {
        fields.insert(key.to_string(), Value::String(val));
    }
    interp.last_typed_error = Some((type_name.to_string(), fields));
    Err(miette::miette!("{type_name}: {message}"))
}

pub(crate) fn find_arg<'a>(args: &'a [CallArgValue], name: &str) -> Option<&'a Value> {
    args.iter()
        .find(|a| a.name.as_deref() == Some(name))
        .map(|a| &a.value)
}

pub(crate) fn positional(args: &[CallArgValue], idx: usize) -> Option<&Value> {
    args.iter()
        .filter(|a| a.name.is_none())
        .nth(idx)
        .map(|a| &a.value)
}

macro_rules! ns {
    ($name:expr, { $($method:expr => $impl:expr),* $(,)? }) => {{
        let mut m: std::collections::HashMap<String, crate::interpreter::BuiltinFn> =
            std::collections::HashMap::new();
        $(
            m.insert($method.to_string(), std::sync::Arc::new($impl));
        )*
        crate::interpreter::Namespace { name: $name.to_string(), methods: m }
    }};
}

pub(crate) use ns;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::CallArgValue;
    use crate::interpreter::value::Value;

    // ── find_arg ──────────────────────────────────────────────────────────

    #[test]
    fn find_arg_returns_value_when_name_matches() {
        let args = [CallArgValue {
            name: Some("ttl".into()),
            value: Value::Duration(60.0),
        }];
        let found = find_arg(&args, "ttl");
        assert!(found.is_some());
        assert_eq!(found.unwrap(), &Value::Duration(60.0));
    }

    #[test]
    fn find_arg_returns_none_when_name_differs() {
        let args = [CallArgValue {
            name: Some("ttl".into()),
            value: Value::Duration(60.0),
        }];
        assert!(find_arg(&args, "missing").is_none());
    }

    #[test]
    fn find_arg_skips_positional_args() {
        let args = [CallArgValue {
            name: None,
            value: Value::String("hi".into()),
        }];
        assert!(find_arg(&args, "hi").is_none());
    }

    #[test]
    fn find_arg_returns_none_on_empty_args() {
        assert!(find_arg(&[], "anything").is_none());
    }

    // ── positional ────────────────────────────────────────────────────────

    #[test]
    fn positional_returns_first_unnamed_arg() {
        let args = [CallArgValue {
            name: None,
            value: Value::Integer(1),
        }];
        let found = positional(&args, 0);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), &Value::Integer(1));
    }

    #[test]
    fn positional_returns_second_unnamed_arg() {
        let args = [
            CallArgValue {
                name: None,
                value: Value::Integer(1),
            },
            CallArgValue {
                name: None,
                value: Value::Bool(true),
            },
        ];
        let found = positional(&args, 1);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), &Value::Bool(true));
    }

    #[test]
    fn positional_skips_named_args() {
        let args = [
            CallArgValue {
                name: Some("x".into()),
                value: Value::Integer(99),
            },
            CallArgValue {
                name: None,
                value: Value::Integer(1),
            },
        ];
        // idx 0 should skip the named arg and return the positional one.
        let found = positional(&args, 0);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), &Value::Integer(1));
    }

    #[test]
    fn positional_returns_none_when_out_of_bounds() {
        let args = [CallArgValue {
            name: None,
            value: Value::Integer(1),
        }];
        assert!(positional(&args, 1).is_none());
        assert!(positional(&[], 0).is_none());
    }

    // ── throw_typed_error ─────────────────────────────────────────────────

    #[test]
    fn throw_typed_error_without_extra() {
        let mut interp = crate::interpreter::Interpreter::new();
        let result = throw_typed_error(&mut interp, "AiError", "something failed", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("AiError"), "expected AiError in: {msg}");
        assert!(
            msg.contains("something failed"),
            "expected message in: {msg}"
        );
        // last_typed_error is populated.
        let te = interp
            .last_typed_error
            .as_ref()
            .expect("last_typed_error should be set");
        assert_eq!(te.0, "AiError");
        assert_eq!(
            te.1.get("message").map(|v| v.to_display_string()),
            Some("something failed".into())
        );
    }

    #[test]
    fn throw_typed_error_with_extra_field() {
        let mut interp = crate::interpreter::Interpreter::new();
        let result = throw_typed_error(
            &mut interp,
            "AiSchemaError",
            "schema mismatch",
            Some(("got", "{\"x\":1}".into())),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("AiSchemaError"),
            "expected AiSchemaError in: {msg}"
        );
        let te = interp
            .last_typed_error
            .as_ref()
            .expect("last_typed_error should be set");
        assert_eq!(te.0, "AiSchemaError");
        assert_eq!(
            te.1.get("got").map(|v| v.to_display_string()),
            Some("{\"x\":1}".into())
        );
    }
}
