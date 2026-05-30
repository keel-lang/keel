// Rust guideline compliant 2026-02-21
use crate::interpreter::CallArgValue;
use crate::interpreter::value::Value;
use crate::runtime::namespace::{find_arg, positional};

pub(crate) fn expect_str<'a>(
    args: &'a [CallArgValue],
    idx: usize,
    caller: &str,
) -> miette::Result<&'a str> {
    let value = positional(args, idx)
        .ok_or_else(|| miette::miette!("{caller}: missing argument at position {idx}"))?;
    expect_str_value(value, &format!("argument at position {idx}"), caller)
}

pub(crate) fn expect_int(args: &[CallArgValue], idx: usize, caller: &str) -> miette::Result<i64> {
    let value = positional(args, idx)
        .ok_or_else(|| miette::miette!("{caller}: missing argument at position {idx}"))?;
    match value {
        Value::Integer(n) => Ok(*n),
        other => Err(type_error(
            caller,
            &format!("argument at position {idx}"),
            "int",
            other,
        )),
    }
}

pub(crate) fn expect_list<'a>(
    args: &'a [CallArgValue],
    idx: usize,
    caller: &str,
) -> miette::Result<&'a [Value]> {
    let value = positional(args, idx)
        .ok_or_else(|| miette::miette!("{caller}: missing argument at position {idx}"))?;
    match value {
        Value::List(items) => Ok(items),
        other => Err(type_error(
            caller,
            &format!("argument at position {idx}"),
            "list",
            other,
        )),
    }
}

pub(crate) fn expect_duration(
    args: &[CallArgValue],
    idx: usize,
    caller: &str,
) -> miette::Result<f64> {
    let value = positional(args, idx)
        .ok_or_else(|| miette::miette!("{caller}: missing argument at position {idx}"))?;
    match value {
        Value::Duration(seconds) => Ok(*seconds),
        other => Err(type_error(
            caller,
            &format!("argument at position {idx}"),
            "duration",
            other,
        )),
    }
}

pub(crate) fn expect_bool_named(
    args: &[CallArgValue],
    name: &str,
    caller: &str,
) -> miette::Result<Option<bool>> {
    match find_arg(args, name) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(other) => Err(type_error(caller, &format!("`{name}:`"), "bool", other)),
        None => Ok(None),
    }
}

pub(crate) fn expect_str_named<'a>(
    args: &'a [CallArgValue],
    name: &str,
    caller: &str,
) -> miette::Result<Option<&'a str>> {
    find_arg(args, name)
        .map(|value| expect_str_value(value, &format!("`{name}:`"), caller))
        .transpose()
}

pub(crate) fn expect_duration_named(
    args: &[CallArgValue],
    name: &str,
    caller: &str,
) -> miette::Result<Option<f64>> {
    match find_arg(args, name) {
        Some(Value::Duration(value)) => Ok(Some(*value)),
        Some(other) => Err(type_error(caller, &format!("`{name}:`"), "duration", other)),
        None => Ok(None),
    }
}

pub(crate) fn expect_str_value<'a>(
    value: &'a Value,
    argument: &str,
    caller: &str,
) -> miette::Result<&'a str> {
    match value {
        Value::String(value) => Ok(value),
        other => Err(type_error(caller, argument, "str", other)),
    }
}

fn type_error(caller: &str, argument: &str, expected: &str, actual: &Value) -> miette::Report {
    miette::miette!(
        "{caller}: {argument} must be {expected}, got {}",
        actual.type_name()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positional_arg(value: Value) -> CallArgValue {
        CallArgValue { name: None, value }
    }

    fn named_arg(name: &str, value: Value) -> CallArgValue {
        CallArgValue {
            name: Some(name.to_string()),
            value,
        }
    }

    #[test]
    fn expect_str_rejects_non_string_values() {
        let args = [positional_arg(Value::Integer(42))];
        let err = expect_str(&args, 0, "File.read").expect_err("integer path must fail");
        assert_eq!(
            err.to_string(),
            "File.read: argument at position 0 must be str, got int"
        );
    }

    #[test]
    fn expect_int_returns_integer_values() {
        let args = [positional_arg(Value::Integer(42))];
        assert_eq!(expect_int(&args, 0, "Example").unwrap(), 42);
    }

    #[test]
    fn expect_list_returns_list_values() {
        let args = [positional_arg(Value::List(vec![Value::Bool(true)]))];
        assert_eq!(
            expect_list(&args, 0, "Example").unwrap(),
            &[Value::Bool(true)]
        );
    }

    #[test]
    fn expect_duration_rejects_non_duration_values() {
        let args = [positional_arg(Value::Integer(42))];
        let err = expect_duration(&args, 0, "Schedule.sleep").expect_err("integer delay must fail");
        assert_eq!(
            err.to_string(),
            "Schedule.sleep: argument at position 0 must be duration, got int"
        );
    }

    #[test]
    fn expect_bool_named_rejects_non_boolean_values() {
        let args = [named_arg("dir", Value::String("yes".into()))];
        let err =
            expect_bool_named(&args, "dir", "File.mktemp").expect_err("string flag must fail");
        assert_eq!(err.to_string(), "File.mktemp: `dir:` must be bool, got str");
    }
}
