//! Side-effect-free compiler session APIs.
//!
//! These functions encode the canonical lex → parse → HIR → type-check → execute
//! sequence without printing to stderr or touching the filesystem. Consumers such
//! as the LSP, tests, and library embeddings call these functions directly; the CLI
//! wraps them in `pipeline.rs` to add stderr rendering and file I/O.

use std::path::Path;
use std::sync::Arc;

use miette::{NamedSource, Result};

use crate::ast::Program;
use crate::diagnostics::LintWarning;
use crate::runtime::context::RuntimeContext;
use crate::types::diagnostics::TypeDiagnostic;
use crate::{formatter, hir, interpreter, lexer, lint, parser, types};

/// A program that has been parsed and type-checked.
///
/// `diagnostics` is empty on a clean program. `has_errors()` is the idiomatic
/// check before passing this to `run_source`.
pub struct CheckedProgram {
    /// The miette source used for span-anchored diagnostic rendering.
    pub source: NamedSource<String>,
    /// The parsed AST, ready for execution.
    pub ast: Program,
    /// Type errors found during checking. Empty means the program is valid.
    pub diagnostics: Vec<TypeDiagnostic>,
}

impl CheckedProgram {
    /// Returns `true` if the program has any type errors.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

/// Lex and parse source text into an AST.
///
/// Returns the parsed `Program` together with the `NamedSource` built
/// during lexing so callers can pass it directly to `check_source` without
/// allocating a second copy of the source string.
///
/// # Errors
///
/// Returns an error if the source cannot be lexed or parsed.
pub fn parse_source(src: &str, name: &str) -> Result<(Program, NamedSource<String>)> {
    let named = NamedSource::new(name, src.to_string());
    let tokens = lexer::lex(src, &named)?;
    let program = parser::parse(tokens, src.len(), &named)?;
    Ok((program, named))
}

/// HIR-lower and type-check a parsed program.
///
/// Always succeeds; type errors are recorded in `CheckedProgram.diagnostics`
/// rather than returned as `Err`.
#[must_use]
pub fn check_source(program: Program, source: NamedSource<String>) -> CheckedProgram {
    check_impl(program, source, false)
}

/// HIR-lower and type-check in strict mode.
///
/// Like `check_source` but runs `check_strict`, which enforces additional
/// constraints. Errors are in `CheckedProgram.diagnostics`.
#[must_use]
pub fn check_source_strict(program: Program, source: NamedSource<String>) -> CheckedProgram {
    check_impl(program, source, true)
}

fn check_impl(program: Program, source: NamedSource<String>, strict: bool) -> CheckedProgram {
    let hir = hir::lower_ast(&program);
    let diagnostics = if strict {
        types::checker::check_strict(&hir)
    } else {
        types::checker::check(&hir)
    };
    CheckedProgram {
        source,
        ast: program,
        diagnostics,
    }
}

/// Execute a type-checked program.
///
/// Refuses to run a program that has type errors; callers must check
/// `has_errors()` or handle the early-return error themselves.
///
/// `source_path` is optional and used only to derive a `program_name` for
/// runtime error messages. Pass `None` for in-memory programs.
///
/// # Errors
///
/// Returns an error if `checked.has_errors()`, or if the interpreter
/// encounters a runtime error.
pub async fn run_source(
    checked: CheckedProgram,
    runtime: Arc<RuntimeContext>,
    source_path: Option<&Path>,
) -> Result<()> {
    if checked.has_errors() {
        return Err(miette::miette!(
            "{} type error(s) — cannot execute a program with type errors",
            checked.diagnostics.len()
        ));
    }
    interpreter::run_with_source_and_runtime(
        checked.ast,
        Some(checked.source),
        source_path,
        runtime,
    )
    .await
}

/// Format source text and return the formatted string.
///
/// No file I/O or stderr output occurs.
///
/// # Errors
///
/// Returns an error if the source cannot be lexed or parsed.
pub fn fmt_source(src: &str, name: &str) -> Result<String> {
    let (program, _) = parse_source(src, name)?;
    Ok(formatter::format_program(&program))
}

/// Lint source text. Type-checking is performed first; linting requires a
/// clean program.
///
/// No file I/O or stderr output occurs.
///
/// # Errors
///
/// Returns an error (without rendering) if lexing, parsing, or type-checking
/// fails. On success, returns the (possibly empty) list of lint warnings.
pub fn lint_source(src: &str, name: &str) -> Result<Vec<LintWarning>> {
    let (program, source) = parse_source(src, name)?;
    let checked = check_source(program, source);
    if checked.has_errors() {
        return Err(miette::miette!(
            "{} type error(s) in {name} — fix before linting",
            checked.diagnostics.len(),
        ));
    }
    Ok(lint::lint(&checked.ast))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_source ────────────────────────────────────────────────────────

    #[test]
    fn parse_source_valid() {
        let result = parse_source("task t() { }", "t.keel");
        assert!(result.is_ok());
    }

    #[test]
    fn parse_source_invalid_returns_err() {
        let result = parse_source("task (", "t.keel");
        assert!(result.is_err());
    }

    // ── check_source ────────────────────────────────────────────────────────

    #[test]
    fn check_source_clean_has_no_errors() {
        let (program, source) = parse_source("task t() { }", "t.keel").unwrap();
        let checked = check_source(program, source);
        assert!(!checked.has_errors());
        assert!(checked.diagnostics.is_empty());
    }

    #[test]
    fn check_source_undefined_name_returns_diagnostic() {
        let src = "task t() { x = undefined_var }";
        let (program, source) = parse_source(src, "t.keel").unwrap();
        let checked = check_source(program, source);
        assert!(checked.has_errors());
        assert!(
            checked
                .diagnostics
                .iter()
                .any(|d| d.message().contains("undefined")),
            "expected an undefined-name diagnostic, got: {:?}",
            checked
                .diagnostics
                .iter()
                .map(|d| d.message())
                .collect::<Vec<_>>()
        );
    }

    // ── run_source ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn run_source_refuses_program_with_type_errors() {
        let src = "task t() { x = undefined_var }";
        let (program, source) = parse_source(src, "t.keel").unwrap();
        let checked = check_source(program, source);
        assert!(checked.has_errors());

        let runtime = RuntimeContext::native();
        let result = run_source(checked, runtime, None).await;
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("type error"),
            "expected 'type error' in message, got: {msg}"
        );
    }

    #[tokio::test]
    async fn run_source_clean_program_succeeds() {
        let src = "task t() { }";
        let (program, source) = parse_source(src, "t.keel").unwrap();
        let checked = check_source(program, source);
        assert!(!checked.has_errors());

        let runtime = RuntimeContext::native();
        run_source(checked, runtime, None).await.unwrap();
    }

    // ── fmt_source ──────────────────────────────────────────────────────────

    #[test]
    fn fmt_source_returns_formatted_text() {
        let src = "task   t(  ) { }";
        let formatted = fmt_source(src, "t.keel").unwrap();
        assert_eq!(formatted, "task t() {\n}\n");
    }

    #[test]
    fn fmt_source_parse_error_returns_err() {
        let result = fmt_source("task (", "t.keel");
        assert!(result.is_err());
    }

    // ── lint_source ─────────────────────────────────────────────────────────

    #[test]
    fn lint_source_clean_returns_empty_warnings() {
        // Empty program has no declarations to trigger lint rules.
        let result = lint_source("", "t.keel");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn lint_source_type_error_returns_err() {
        let src = "task t() { x = undefined_var }";
        let result = lint_source(src, "t.keel");
        assert!(result.is_err());
    }
}
