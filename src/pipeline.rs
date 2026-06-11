//! CLI entry points for `keel run`, `check`, `fmt`, `build`, and `lint`.
//!
//! Thin wrappers around `session::*` that add file I/O and stderr rendering.
//! All side-effect-free compilation logic lives in `session.rs`; this module
//! is responsible only for reading files, printing diagnostics, and returning
//! exit-code-shaped `miette::Result<()>` values.

#![warn(missing_docs)]

use miette::{IntoDiagnostic, NamedSource, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use colored::Colorize;

use crate::interpreter::TestOutcome;
use crate::lint;
use crate::runtime::context::RuntimeContext;
use crate::session;
use crate::types::diagnostics::TypeDiagnostic;
use crate::vm;

fn load_source(path: &Path) -> Result<(String, String)> {
    let source = fs::read_to_string(path)
        .into_diagnostic()
        .map_err(|e| miette::miette!("Could not read '{}': {}", path.display(), e))?;
    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    Ok((source, filename))
}

/// Print type errors in miette diagnostic format. Only called on error paths,
/// so we hint the compiler to keep this out of the hot instruction cache.
#[cold]
fn report_type_errors(errors: &[TypeDiagnostic], named_src: &NamedSource<String>) {
    for err in errors {
        eprintln!("{:?}", err.to_report(named_src));
    }
}

fn fail_on_type_errors(checked: &session::CheckedProgram, path: &Path) -> Result<()> {
    if checked.has_errors() {
        report_type_errors(&checked.diagnostics, &checked.source);
        return Err(miette::miette!(
            "{} type error(s) in {}",
            checked.diagnostics.len(),
            path.display()
        ));
    }
    Ok(())
}

fn fail_on_graph_type_errors(checked: &session::CheckedGraph, path: &Path) -> Result<()> {
    if !checked.has_errors() {
        return Ok(());
    }
    for (index, diagnostics) in checked.diagnostics.iter().enumerate() {
        if !diagnostics.is_empty() {
            report_type_errors(diagnostics, &checked.graph.modules[index].source);
        }
    }
    Err(miette::miette!(
        "{} type error(s) in {}",
        checked.error_count(),
        path.display()
    ))
}

fn load_checked_graph(path: &Path) -> Result<session::CheckedGraph> {
    let (src, name) = load_source(path)?;
    let checked = session::load_and_check_graph(&src, &name, Some(path))?;
    fail_on_graph_type_errors(&checked, path)?;
    Ok(checked)
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

    let checked = load_checked_graph(path)?;
    session::run_graph(&checked, runtime).await
}

/// Execute all `test` blocks in a `.keel` file.
///
/// # Errors
///
/// Returns an error if the file cannot be read, parsed, or type-checked, or if
/// one or more tests fail.
pub async fn test_file_with_runtime(
    path: &Path,
    runtime: Arc<RuntimeContext>,
    filter: Option<&str>,
    list: bool,
    fail_fast: bool,
    quiet: bool,
) -> Result<()> {
    if path.is_dir() {
        return test_directory_with_runtime(path, runtime, filter, list, fail_fast, quiet).await;
    }

    let checked = load_checked_graph(path)?;

    if list {
        let names = session::graph_test_names(&checked, filter);
        if names.is_empty() {
            eprintln!("0 tests found");
        } else {
            for name in names {
                eprintln!("{name}");
            }
        }
        return Ok(());
    }

    let suite_started = Instant::now();
    let outcomes = session::test_graph(&checked, runtime, filter, fail_fast).await?;
    let suite_elapsed = suite_started.elapsed();
    if outcomes.is_empty() && filter.is_none() {
        eprintln!("0 tests found");
        return Ok(());
    }
    if outcomes.is_empty()
        && let Some(filter) = filter
    {
        return Err(miette::miette!(
            "no tests matched filter `{filter}` in {}",
            path.display()
        ));
    }
    let passed = outcomes.iter().filter(|outcome| outcome.passed).count();
    let failed = outcomes.len().saturating_sub(passed);

    print_test_outcomes(&outcomes, None, quiet);

    if failed == 0 {
        print_test_summary(passed, failed, suite_elapsed);
        return Ok(());
    }

    print_test_summary(passed, failed, suite_elapsed);

    Err(miette::miette!(
        "{passed} passed, {failed} failed in {}",
        path.display()
    ))
}

async fn test_directory_with_runtime(
    dir: &Path,
    runtime: Arc<RuntimeContext>,
    filter: Option<&str>,
    list: bool,
    fail_fast: bool,
    quiet: bool,
) -> Result<()> {
    let files = discover_test_files(dir)?;
    if files.is_empty() {
        eprintln!("0 tests found");
        return Ok(());
    }

    if list {
        let mut listed = 0_usize;
        for file in files {
            let checked = load_checked_graph(&file)?;
            for name in session::graph_test_names(&checked, filter) {
                eprintln!("{}: {name}", file.display());
                listed += 1;
            }
        }
        if listed == 0 {
            eprintln!("0 tests found");
        }
        return Ok(());
    }

    let suite_started = Instant::now();
    let mut passed = 0_usize;
    let mut failed = 0_usize;
    let mut matched = 0_usize;

    for file in files {
        let checked = load_checked_graph(&file)?;
        let outcomes =
            session::test_graph(&checked, Arc::clone(&runtime), filter, fail_fast).await?;
        if outcomes.is_empty() {
            continue;
        }
        matched += outcomes.len();
        passed += outcomes.iter().filter(|outcome| outcome.passed).count();
        failed += outcomes.iter().filter(|outcome| !outcome.passed).count();
        print_test_outcomes(&outcomes, Some(&file), quiet);
        if fail_fast && outcomes.iter().any(|outcome| !outcome.passed) {
            break;
        }
    }

    if matched == 0 {
        if let Some(filter) = filter {
            return Err(miette::miette!(
                "no tests matched filter `{filter}` in {}",
                dir.display()
            ));
        }
        eprintln!("0 tests found");
        return Ok(());
    }

    let suite_elapsed = suite_started.elapsed();
    print_test_summary(passed, failed, suite_elapsed);

    if failed == 0 {
        return Ok(());
    }

    Err(miette::miette!(
        "{passed} passed, {failed} failed in {}",
        dir.display()
    ))
}

fn discover_test_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_test_files(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_test_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)
        .into_diagnostic()
        .map_err(|e| miette::miette!("Could not read directory '{}': {}", dir.display(), e))?
    {
        let path = entry.into_diagnostic()?.path();
        if path.is_dir() {
            collect_test_files(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("keel") {
            let source = fs::read_to_string(&path)
                .into_diagnostic()
                .map_err(|e| miette::miette!("Could not read '{}': {}", path.display(), e))?;
            if source.contains("test \"") {
                files.push(path);
            }
        }
    }
    Ok(())
}

fn print_test_outcomes(outcomes: &[TestOutcome], path: Option<&Path>, quiet: bool) {
    for outcome in outcomes {
        let elapsed = format_test_duration(outcome.elapsed).dimmed();
        let name = match path {
            Some(path) => format!("{}: {}", path.display(), outcome.name),
            None => outcome.name.clone(),
        };
        if outcome.passed {
            if quiet {
                continue;
            }
            eprintln!("{} {} {}", "PASS".green(), name, elapsed);
        } else {
            eprintln!("{} {} {}", "FAIL".red(), name, elapsed);
            if let Some(location) = &outcome.failure_location {
                eprintln!("  {location}");
            }
            if let Some(error) = &outcome.error {
                eprintln!("  {error}");
            }
        }
    }
}

fn print_test_summary(passed: usize, failed: usize, elapsed: Duration) {
    if failed == 0 {
        eprintln!(
            "{} in {}",
            format_passed_summary(passed).green(),
            format_test_duration_value(elapsed).dimmed()
        );
        return;
    }

    eprintln!(
        "{}, {} in {}",
        format_passed_summary(passed).green(),
        format_failed_summary(failed).red(),
        format_test_duration_value(elapsed).dimmed()
    );
}

fn format_test_duration(duration: Duration) -> String {
    format!("({})", format_test_duration_value(duration))
}

fn format_test_duration_value(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis == 0 {
        return "<1ms".to_string();
    }
    if millis < 1_000 {
        return format!("{millis}ms");
    }
    format!("{:.2}s", duration.as_secs_f64())
}

fn format_passed_summary(passed: usize) -> String {
    format!(
        "{} test{} passed",
        passed,
        if passed == 1 { "" } else { "s" }
    )
}

fn format_failed_summary(failed: usize) -> String {
    format!(
        "{} test{} failed",
        failed,
        if failed == 1 { "" } else { "s" }
    )
}

/// Type-check a `.keel` file without executing it.
///
/// # Errors
///
/// Returns an error with a count if one or more type errors are found.
pub fn check_file(path: &Path, strict: bool) -> Result<()> {
    let (src, name) = load_source(path)?;
    let checked = if strict {
        session::load_and_check_graph_strict(&src, &name, Some(path))?
    } else {
        session::load_and_check_graph(&src, &name, Some(path))?
    };
    fail_on_graph_type_errors(&checked, path)?;

    eprintln!("✓ {} is valid", path.display());
    Ok(())
}

/// Compile a `.keel` file to a `.keelc` bytecode bundle.
///
/// # Errors
///
/// Returns an error if parsing, type-checking, or compilation fails.
pub fn build_file(path: &Path) -> Result<()> {
    let (src, name) = load_source(path)?;
    let (program, named_src) = session::parse_source(&src, &name)?;
    let checked = session::check_source(program, named_src);
    fail_on_type_errors(&checked, path)?;

    let compiled = vm::compiler::compile(&checked.ast)
        .map_err(|e| miette::miette!("Compilation error: {e}"))?;

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
    let (src, name) = load_source(path)?;
    let formatted = session::fmt_source(&src, &name)?;
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
    let (src, name) = load_source(path)?;
    let checked = session::load_and_check_graph(&src, &name, Some(path))?;
    if checked.has_errors() {
        for (index, diagnostics) in checked.diagnostics.iter().enumerate() {
            if !diagnostics.is_empty() {
                report_type_errors(diagnostics, &checked.graph.modules[index].source);
            }
        }
        return Err(miette::miette!(
            "{} type error(s) in {} — fix before linting",
            checked.error_count(),
            path.display()
        ));
    }

    let warnings = lint::lint(&checked.graph.entry().program);

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
            .with_source_code(checked.graph.entry().source.clone());
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
        let fixed_source = apply_lint_fixes(&src, &warnings);
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
pub fn apply_lint_fixes(source: &str, warnings: &[crate::diagnostics::LintWarning]) -> String {
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

#[cfg(test)]
mod tests {
    use super::{
        apply_lint_fixes, build_file, check_file, fmt_file, format_failed_summary,
        format_passed_summary, format_test_duration, format_test_duration_value, lint_file,
        run_file_with_runtime,
    };
    use crate::diagnostics::LintWarning;
    use crate::runtime::context::RuntimeContext;
    use std::io::Write as _;

    fn write_keel_file(source: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::Builder::new()
            .suffix(".keel")
            .tempfile()
            .expect("create temporary keel file");
        file.write_all(source.as_bytes())
            .expect("write temporary keel file");
        file
    }

    #[test]
    fn pipeline_check_reports_missing_file_as_named_path_error() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let missing = dir.path().join("missing.keel");

        let err = check_file(&missing, false).expect_err("missing file should fail");

        let message = err.to_string();
        assert!(
            message.contains("Could not read") && message.contains("missing.keel"),
            "expected readable missing-file diagnostic, got: {message}"
        );
    }

    #[test]
    fn pipeline_format_rewrites_valid_program_in_place() {
        let file = write_keel_file(
            r#"
task greet(name: str) -> str { "hi {name}" }
"#,
        );

        fmt_file(file.path()).expect("format should succeed");

        let formatted = std::fs::read_to_string(file.path()).expect("read formatted source");
        assert!(
            formatted.contains("task greet(name: str) -> str"),
            "formatted output should retain task signature:\n{formatted}"
        );
        assert!(
            formatted.ends_with('\n'),
            "formatter should write a trailing newline:\n{formatted:?}"
        );
    }

    #[test]
    fn pipeline_lint_fix_removes_safe_unused_binding_and_keeps_program_valid() {
        let file = write_keel_file(
            r#"
use std/io

agent A {
  @on_start {
    unused = "hello"
    io.show("done")
  }
}
run(A)
"#,
        );

        let err = lint_file(file.path(), true).expect_err("lint warnings still fail command");
        let fixed = std::fs::read_to_string(file.path()).expect("read fixed source");

        assert!(
            err.to_string().contains("lint warning"),
            "expected lint warning summary, got: {err}"
        );
        assert!(
            !fixed.contains("unused ="),
            "fixable unused binding should be removed:\n{fixed}"
        );
        check_file(file.path(), false).expect("fixed program should still type-check");
    }

    #[test]
    fn pipeline_build_reaches_deferred_vm_compiler_without_writing_bytecode() {
        let file = write_keel_file(
            r#"
task answer() -> int {
  42
}
"#,
        );
        let bytecode_path = file.path().with_extension("keelc");

        let err = build_file(file.path()).expect_err("build is deferred in v0.1");

        let message = err.to_string();
        assert!(
            message.contains("deferred post-v0.1"),
            "expected deferred build diagnostic, got: {message}"
        );
        assert!(
            !bytecode_path.exists(),
            "deferred build must not write stale bytecode at {}",
            bytecode_path.display()
        );
    }

    #[tokio::test]
    async fn pipeline_run_keelc_rejected_on_extension_before_reading_file() {
        // The extension check fires before any I/O, so any .keelc file —
        // valid, invalid, or empty — produces the same deferred error.
        let file = tempfile::Builder::new()
            .suffix(".keelc")
            .tempfile()
            .expect("create temporary keelc file");

        let err = run_file_with_runtime(file.path(), RuntimeContext::native())
            .await
            .expect_err(".keelc execution should be rejected");

        assert!(
            err.to_string()
                .contains("Bytecode execution (.keelc) is not yet supported"),
            "expected bytecode-deferred diagnostic, got: {err}"
        );
    }

    #[tokio::test]
    async fn pipeline_run_file_reports_type_errors_before_execution() {
        let file = write_keel_file(
            r#"
use std/io

agent A {
  @on_start {
    x: int = "wrong"
    io.show("should not run")
  }
}
run(A)
"#,
        );

        let err = run_file_with_runtime(file.path(), RuntimeContext::native())
            .await
            .expect_err("type errors should prevent execution");

        assert!(
            err.to_string().contains("type error"),
            "expected type error summary, got: {err}"
        );
    }

    #[test]
    fn pipeline_build_reports_type_errors_before_deferred_compiler() {
        let file = write_keel_file(
            r#"
task answer() -> int {
  return "wrong"
}
"#,
        );

        let err = build_file(file.path()).expect_err("type error should fail build");

        let message = err.to_string();
        assert!(
            message.contains("type error"),
            "expected build type-error summary, got: {message}"
        );
    }

    #[test]
    fn pipeline_lint_clean_program_succeeds_without_fixes() {
        let file = write_keel_file(
            r#"
use std/io

task greet(name: str) -> str {
  "hello {name}"
}

agent A {
  @on_start {
    msg = greet("keel")
    io.show(msg)
  }
}
run(A)
"#,
        );

        lint_file(file.path(), false).expect("clean program should lint cleanly");
    }

    #[test]
    fn pipeline_lint_reports_type_errors_before_warnings() {
        let file = write_keel_file(
            r#"
agent A {
  @on_start {
    unused = "still not the first problem"
    x: int = "wrong"
  }
}
run(A)
"#,
        );

        let err = lint_file(file.path(), false).expect_err("type error should stop lint");

        let message = err.to_string();
        assert!(
            message.contains("fix before linting"),
            "expected lint type-check guard, got: {message}"
        );
    }

    #[test]
    fn test_duration_format_reports_sub_millisecond_runs() {
        assert_eq!(
            format_test_duration(std::time::Duration::from_micros(500)),
            "(<1ms)"
        );
        assert_eq!(
            format_test_duration(std::time::Duration::from_millis(12)),
            "(12ms)"
        );
        assert_eq!(
            format_test_duration(std::time::Duration::from_millis(1_250)),
            "(1.25s)"
        );
    }

    #[test]
    fn suite_summary_formats_counts_and_elapsed_time() {
        assert_eq!(format_passed_summary(1), "1 test passed");
        assert_eq!(format_passed_summary(2), "2 tests passed");
        assert_eq!(format_failed_summary(1), "1 test failed");
        assert_eq!(format_failed_summary(2), "2 tests failed");
        assert_eq!(
            format_test_duration_value(std::time::Duration::from_micros(500)),
            "<1ms"
        );
    }

    #[test]
    fn apply_lint_fixes_two_non_overlapping_warnings_both_removed() {
        let source = "aaa\nbbb\nccc\n";

        let warnings = vec![
            LintWarning {
                message: "unused".into(),
                span: Some(1..2),
                fixable: true,
                hint: None,
            },
            LintWarning {
                message: "unused".into(),
                span: Some(5..6),
                fixable: true,
                hint: None,
            },
        ];

        let result = apply_lint_fixes(source, &warnings);
        assert!(
            !result.contains("aaa"),
            "first fixable line should be removed:\n{result:?}"
        );
        assert!(
            !result.contains("bbb"),
            "second fixable line should be removed:\n{result:?}"
        );
        assert!(
            result.contains("ccc"),
            "unfixed line should remain:\n{result:?}"
        );
    }

    #[test]
    fn apply_lint_fixes_overlapping_ranges_do_not_panic() {
        let source = "aaa\nbbb\nccc\n";

        let warnings = vec![
            LintWarning {
                message: "unused".into(),
                span: Some(0..4),
                fixable: true,
                hint: None,
            },
            LintWarning {
                message: "unused".into(),
                span: Some(4..8),
                fixable: true,
                hint: None,
            },
        ];

        let result = apply_lint_fixes(source, &warnings);
        assert!(
            !result.contains("aaa"),
            "first overlapping span should be removed:\n{result:?}"
        );
        assert!(
            !result.contains("bbb"),
            "second overlapping span should be removed:\n{result:?}"
        );
    }

    // ── smoke: every example must pass `keel check` ──────────────────────────

    #[test]
    fn examples_all_parse() {
        let examples_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
        let mut names: Vec<String> = std::fs::read_dir(&examples_dir)
            .expect("read examples directory")
            .filter_map(|entry| {
                let path = entry.expect("read examples entry").path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("keel") {
                    path.file_stem()
                        .and_then(|stem| stem.to_str())
                        .map(str::to_owned)
                } else {
                    None
                }
            })
            .collect();
        names.sort();

        for name in names {
            let path = examples_dir.join(format!("{name}.keel"));
            check_file(&path, false)
                .unwrap_or_else(|e| panic!("`keel check {name}.keel` failed: {e}"));
        }
    }
}
