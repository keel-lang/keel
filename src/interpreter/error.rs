use std::collections::HashMap;
use std::fmt;

use super::value::Value;

/// Classification of a structured runtime error from a stdlib namespace.
///
/// Each variant maps to a stable Keel error type name used in `catch` clauses
/// and as the machine-readable diagnostic code. Add variants here as namespaces
/// are migrated to structured errors (see issue #20).
#[derive(Debug)]
// Variants that are defined but not yet wired to a namespace are intentional:
// they exist so user code can catch them by name once the namespace migrates.
#[allow(dead_code)]
pub(crate) enum RuntimeErrorKind {
    // Namespace-specific — users catch these individually
    File,
    Csv,
    Ai,
    AiSchema,
    Db,
    Cache,
    Math,
    Memory,
    Email,
    Http,
    Shell,
    Json,
    Env,
    // Cross-namespace
    Capability,
    Io,
    UserRaised,
    Timeout,
    Deadline,
    RuntimeBusy,
}

impl RuntimeErrorKind {
    /// The Keel error type name matched in `catch (e: <TypeName>)` clauses.
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            Self::File => "FileError",
            Self::Csv => "CsvError",
            Self::Ai => "AiError",
            Self::AiSchema => "AiSchemaError",
            Self::Db => "DbError",
            Self::Cache => "CacheError",
            Self::Math => "MathError",
            Self::Memory => "MemoryError",
            Self::Email => "EmailError",
            Self::Http => "HttpError",
            Self::Shell => "ShellError",
            Self::Json => "JsonError",
            Self::Env => "EnvError",
            Self::Capability => "CapabilityError",
            Self::Io => "IoError",
            Self::UserRaised => "UserRaised",
            Self::Timeout => "TimeoutError",
            Self::Deadline => "DeadlineError",
            Self::RuntimeBusy => "RuntimeBusy",
        }
    }
}

/// A typed Keel runtime error carried through interpreter call stacks.
#[derive(Debug)]
pub(crate) struct RuntimeError {
    pub(crate) kind: RuntimeErrorKind,
    pub(crate) fields: HashMap<String, Value>,
}

impl RuntimeError {
    pub(crate) fn new(kind: RuntimeErrorKind, fields: HashMap<String, Value>) -> Self {
        Self { kind, fields }
    }

    pub(crate) fn type_name(&self) -> &'static str {
        self.kind.type_name()
    }

    pub(crate) fn as_value(&self) -> Value {
        Value::Struct(self.type_name().to_string(), self.fields.clone())
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = self
            .fields
            .get("message")
            .map(|v| v.to_display_string())
            .unwrap_or_default();
        write!(f, "{}: {}", self.type_name(), msg)
    }
}

impl std::error::Error for RuntimeError {}

impl miette::Diagnostic for RuntimeError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new(format!(
            "keel::runtime::{}",
            self.kind.type_name()
        )))
    }
}

pub(crate) fn runtime_error(msg: impl Into<String>) -> miette::Report {
    miette::miette!("{}", msg.into())
}
