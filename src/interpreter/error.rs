use std::collections::HashMap;
use std::fmt;

use super::value::Value;

/// A typed Keel runtime error carried through interpreter call stacks.
#[derive(Debug)]
pub(crate) struct RuntimeError {
    pub(crate) type_name: String,
    pub(crate) fields: HashMap<String, Value>,
}

impl RuntimeError {
    pub(crate) fn new(type_name: impl Into<String>, fields: HashMap<String, Value>) -> Self {
        Self {
            type_name: type_name.into(),
            fields,
        }
    }

    pub(crate) fn as_value(&self) -> Value {
        Value::Struct(self.type_name.clone(), self.fields.clone())
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = self
            .fields
            .get("message")
            .map(|v| v.to_display_string())
            .unwrap_or_default();
        write!(f, "{}: {}", self.type_name, msg)
    }
}

impl std::error::Error for RuntimeError {}

impl miette::Diagnostic for RuntimeError {}

pub(crate) fn runtime_error(msg: impl Into<String>) -> miette::Report {
    miette::miette!("{}", msg.into())
}
