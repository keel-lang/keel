//! Compiler pipeline entry points for `keel run`, `check`, `fmt`, `build`, and `lint`.
//!
//! These functions encode the canonical lex → parse → type-check → execute sequence.
//! They live in the library so integration tests can call them directly without
//! spawning the `keel` binary as a subprocess.

#![warn(missing_docs)]

use miette::{IntoDiagnostic, NamedSource, Result};
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::runtime::context::RuntimeContext;
use crate::{formatter, interpreter, lexer, lint, parser, types, vm};

fn load_and_parse(path: &Path) -> Result<(String, NamedSource<String>, crate::ast::Program)> {
    let source = fs::read_to_string(path)
        .into_diagnostic()
        .map_err(|e| miette::miette!("Could not read '{}': {}", path.display(), e))?;
    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let named_src = NamedSource::new(&filename, source.clone());
    let tokens = lexer::lex(&source, &named_src)?;
    let program = parser::parse(tokens, source.len(), &named_src)?;
    Ok((source, named_src, program))
}

/// Print type errors in miette diagnostic format. Only called on error paths,
/// so we hint the compiler to keep this out of the hot instruction cache.
///
/// # Errors
///
/// Nothing is returned; errors are printed to stderr. The caller is responsible
/// for constructing a summary error after this function returns.
#[cold]
fn report_type_errors(errors: &[types::checker::TypeError], named_src: &NamedSource<String>) {
    for err in errors {
        if let Some(span) = &err.span {
            let report = miette::miette!(
                labels = vec![miette::LabeledSpan::at(span.clone(), &err.message)],
                "Type error"
            )
            .with_source_code(named_src.clone());
            eprintln!("{:?}", report);
        } else {
            eprintln!("  Type error: {err}");
        }
    }
}

/// Execute a `.keel` file.
///
/// # Errors
///
/// Returns an error if the file cannot be read, parsed, or type-checked,
/// or if the interpreter encounters a runtime error.
pub async fn run_file(path: &Path) -> Result<()> {
    run_file_with_runtime(path, RuntimeContext::native()).await
}

/// Execute a `.keel` file with an explicit runtime context.
///
/// # Errors
///
/// Returns an error if the file cannot be read, parsed, or type-checked,
/// if the interpreter encounters a runtime error, or if a `.keelc`
/// bytecode file is passed (bytecode execution is deferred post-v0.1).
pub async fn run_file_with_runtime(path: &Path, runtime: Arc<RuntimeContext>) -> Result<()> {
    if path.extension().map(|e| e == "keelc").unwrap_or(false) {
        return Err(miette::miette!(
            "Bytecode execution (.keelc) is not yet supported. Use the .keel source file instead."
        ));
    }

    let (_, named_src, program) = load_and_parse(path)?;

    let errors = types::checker::check(&program);
    if !errors.is_empty() {
        report_type_errors(&errors, &named_src);
        return Err(miette::miette!(
            "{} type error(s) in {}",
            errors.len(),
            path.display()
        ));
    }

    interpreter::run_with_source_and_runtime(program, Some(named_src.clone()), Some(path), runtime)
        .await?;
    Ok(())
}

/// Type-check a `.keel` file without executing it.
///
/// # Errors
///
/// Returns an error with a count if one or more type errors are found.
pub fn check_file(path: &Path, strict: bool) -> Result<()> {
    let (_, named_src, program) = load_and_parse(path)?;

    let errors = if strict {
        types::checker::check_strict(&program)
    } else {
        types::checker::check(&program)
    };

    if !errors.is_empty() {
        report_type_errors(&errors, &named_src);
        return Err(miette::miette!(
            "{} type error(s) in {}",
            errors.len(),
            path.display()
        ));
    }

    eprintln!("✓ {} is valid", path.display());
    Ok(())
}

/// Compile a `.keel` file to a `.keelc` bytecode bundle.
///
/// # Errors
///
/// Returns an error if parsing, type-checking, or compilation fails.
pub fn build_file(path: &Path) -> Result<()> {
    let (_, _, program) = load_and_parse(path)?;

    let errors = types::checker::check(&program);
    if !errors.is_empty() {
        for err in &errors {
            eprintln!("  Type error: {err}");
        }
        return Err(miette::miette!(
            "{} type error(s) in {}",
            errors.len(),
            path.display()
        ));
    }

    let compiled =
        vm::compiler::compile(&program).map_err(|e| miette::miette!("Compilation error: {e}"))?;

    let out_path = path.with_extension("keelc");
    let bytes = serde_json::to_vec_pretty(&compiled).into_diagnostic()?;
    fs::write(&out_path, bytes).into_diagnostic()?;

    let op_count: usize = compiled.main.ops.len()
        + compiled
            .functions
            .iter()
            .map(|f| f.ops.len())
            .sum::<usize>();

    eprintln!(
        "✓ Compiled {} → {} ({} ops, {} functions)",
        path.display(),
        out_path.display(),
        op_count,
        compiled.functions.len()
    );
    Ok(())
}

/// Format a `.keel` file in-place.
///
/// # Errors
///
/// Returns an error if the file cannot be read, parsed, or written.
pub fn fmt_file(path: &Path) -> Result<()> {
    let (_, _, program) = load_and_parse(path)?;
    let formatted = formatter::format_program(&program);
    fs::write(path, &formatted).into_diagnostic()?;
    eprintln!("✓ Formatted {}", path.display());
    Ok(())
}

/// Lint a `.keel` file, optionally applying auto-fixes.
///
/// # Errors
///
/// Returns an error if type-checking fails (lint requires clean types), or if
/// one or more lint warnings are found.
pub fn lint_file(path: &Path, fix: bool) -> Result<()> {
    let (source, named_src, program) = load_and_parse(path)?;

    let type_errors = types::checker::check(&program);
    if !type_errors.is_empty() {
        report_type_errors(&type_errors, &named_src);
        return Err(miette::miette!(
            "{} type error(s) in {} — fix before linting",
            type_errors.len(),
            path.display()
        ));
    }

    let warnings = lint::lint(&program);

    if warnings.is_empty() {
        eprintln!("✓ {} — no lint warnings", path.display());
        return Ok(());
    }

    for w in &warnings {
        if let Some(span) = &w.span {
            let mut label = w.message.clone();
            if let Some(hint) = &w.hint {
                label = format!("{} — hint: {}", label, hint);
            }
            let report = miette::miette!(
                labels = vec![miette::LabeledSpan::at(span.clone(), &label)],
                "Lint warning"
            )
            .with_source_code(named_src.clone());
            eprintln!("{:?}", report);
        } else {
            eprint!("  warning: {}", w.message);
            if let Some(hint) = &w.hint {
                eprint!(" — hint: {hint}");
            }
            eprintln!();
        }
    }

    let fixable_count = warnings
        .iter()
        .filter(|w| w.fixable && w.span.is_some())
        .count();

    if fix && fixable_count > 0 {
        let fixed_source = apply_lint_fixes(&source, &warnings);
        fs::write(path, &fixed_source).into_diagnostic()?;
        eprintln!("✓ Applied {fixable_count} fix(es) to {}", path.display());
    } else if !fix && fixable_count > 0 {
        eprintln!(
            "  {} warning(s) can be fixed automatically — run `keel lint --fix {}`",
            fixable_count,
            path.display()
        );
    }

    let total = warnings.len();
    Err(miette::miette!(
        "{total} lint warning(s) in {}",
        path.display()
    ))
}

/// Remove lines corresponding to fixable lint warnings. Spans are processed in
/// reverse byte order so earlier removals don't shift later positions.
///
/// Returns the original source unchanged when there are no fixable warnings.
/// Otherwise returns a new `String` with fixable lines removed.
#[must_use]
pub fn apply_lint_fixes(source: &str, warnings: &[lint::LintWarning]) -> String {
    let mut ranges: Vec<(usize, usize)> = warnings
        .iter()
        .filter(|w| w.fixable)
        .filter_map(|w| w.span.as_ref())
        .map(|span| {
            let line_start = source[..span.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let line_end = source[span.end..]
                .find('\n')
                .map(|i| span.end + i + 1)
                .unwrap_or(source.len());
            (line_start, line_end)
        })
        .collect();

    // If there are no fixable ranges, return the source as-is without cloning.
    if ranges.is_empty() {
        return source.to_owned();
    }

    ranges.sort_by_key(|b| std::cmp::Reverse(b.0));

    // Merge overlapping ranges. Sorted descending by start, so a new range
    // overlaps the last merged one when its end extends past the last merged
    // start. Adjacent ranges (e == last.0) are kept separate — they can each
    // be replaced safely in descending order without index shifting.
    let mut merged: Vec<(usize, usize)> = vec![];
    for (s, e) in ranges {
        if let Some(last) = merged.last_mut()
            && e > last.0
        {
            // Ranges overlap: extend the merged entry in both directions.
            last.0 = last.0.min(s);
            last.1 = last.1.max(e);
            continue;
        }
        merged.push((s, e));
    }

    let mut result = source.to_string();
    for (start, end) in merged {
        result.replace_range(start..end, "");
    }
    result
}
