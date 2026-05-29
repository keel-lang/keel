//! Structured diagnostics emitted by the type checker.
//!
//! The checker keeps diagnostics as typed data until presentation boundaries
//! such as the CLI pipeline and LSP convert them into user-facing messages.

use crate::lexer::Span;
use crate::types::ty::{Ty, describe_ty};

/// A structured type-checking diagnostic.
#[derive(Debug, Clone)]
pub enum TypeDiagnostic {
    /// A value-level identifier was not found in lexical or global scope.
    UndefinedName { name: String, span: Span },
    /// An inferred type did not satisfy a declared or expected type.
    TypeMismatch {
        context: String,
        expected: Ty,
        actual: Ty,
        span: Span,
        help: Option<String>,
    },
    /// A call site passed too many positional arguments.
    WrongArity {
        task_name: String,
        expected: usize,
        actual: usize,
        expected_params: Vec<String>,
        span: Span,
    },
    /// A `when` over an enum did not cover every variant.
    NonExhaustiveWhen {
        enum_name: String,
        missing: Vec<String>,
        span: Span,
    },
    /// An implementation does not satisfy its declared interface.
    InterfaceNotSatisfied {
        impl_name: String,
        interface_name: String,
        reason: String,
        span: Span,
    },
    /// A diagnostic that has not yet been migrated to a richer category.
    Other { message: String, span: Span },
}

impl TypeDiagnostic {
    /// Return the primary source span for this diagnostic.
    #[must_use]
    pub fn span(&self) -> &Span {
        match self {
            TypeDiagnostic::UndefinedName { span, .. }
            | TypeDiagnostic::TypeMismatch { span, .. }
            | TypeDiagnostic::WrongArity { span, .. }
            | TypeDiagnostic::NonExhaustiveWhen { span, .. }
            | TypeDiagnostic::InterfaceNotSatisfied { span, .. }
            | TypeDiagnostic::Other { span, .. } => span,
        }
    }

    /// Render the diagnostic with the legacy user-facing text.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            TypeDiagnostic::UndefinedName { name, .. } => format!("undefined: `{name}`"),
            TypeDiagnostic::TypeMismatch {
                context,
                expected,
                actual,
                help,
                ..
            } => {
                let mut message = format!(
                    "{context}: expected {}, got {}",
                    describe_ty(expected),
                    describe_ty(actual)
                );
                if let Some(help) = help {
                    message.push_str(" — ");
                    message.push_str(help);
                }
                message
            }
            TypeDiagnostic::WrongArity {
                task_name,
                expected,
                actual,
                expected_params,
                ..
            } => {
                let hint = if expected_params.is_empty() {
                    "task takes no arguments".to_string()
                } else {
                    format!("expected: {}", expected_params.join(", "))
                };
                format!("task `{task_name}` takes {expected} argument(s), got {actual} — {hint}")
            }
            TypeDiagnostic::NonExhaustiveWhen {
                enum_name, missing, ..
            } => format!(
                "non-exhaustive `when` on enum `{enum_name}` — missing: {}",
                missing.join(", ")
            ),
            TypeDiagnostic::InterfaceNotSatisfied {
                impl_name,
                interface_name,
                reason,
                ..
            } => format!("impl `{interface_name}` for `{impl_name}`: {reason}"),
            TypeDiagnostic::Other { message, .. } => message.clone(),
        }
    }

    /// Create a generic diagnostic with an explicit span.
    pub(crate) fn other(message: impl Into<String>, span: Span) -> Self {
        TypeDiagnostic::Other {
            message: message.into(),
            span,
        }
    }

    /// Convert the diagnostic into a `miette` report.
    #[must_use]
    pub fn to_report(&self, named_src: &miette::NamedSource<String>) -> miette::Report {
        miette::Report::from(self).with_source_code(named_src.clone())
    }
}

impl From<&TypeDiagnostic> for miette::Report {
    fn from(value: &TypeDiagnostic) -> Self {
        let message = value.message();
        miette::miette!(
            labels = vec![miette::LabeledSpan::at(value.span().clone(), message)],
            "Type error"
        )
    }
}

impl std::fmt::Display for TypeDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}
