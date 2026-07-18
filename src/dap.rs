//! `keel dap` and `keel test --debug` CLI entry points.
//!
//! Thin wrappers around `session::*` (same load/check/type-error gate every
//! other subcommand uses) that hand an already-checked `ModuleGraph` off to
//! `keel-dap`. All DAP protocol logic lives in that crate — this module only
//! does file I/O, the type-check gate, and (for `--debug`) resolving the
//! `--filter` down to exactly one test.

use std::path::Path;
use std::sync::Arc;

use miette::Result;

use crate::runtime::context::RuntimeContext;
use crate::session;

fn load_source(path: &Path) -> Result<(String, String)> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| miette::miette!("Could not read '{}': {}", path.display(), e))?;
    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    Ok((source, filename))
}

/// `keel dap <file>` — run a program under the debugger.
///
/// # Errors
///
/// Returns an error if the file cannot be read, parsed, or type-checked, or
/// if the debugged program itself fails.
pub async fn run_file(path: &Path, runtime: Arc<RuntimeContext>) -> Result<()> {
    let (src, name) = load_source(path)?;
    let checked = session::load_and_check_graph(&src, &name, Some(path))?;
    if checked.has_errors() {
        return Err(miette::miette!(
            "{} type error(s) in {} — fix before debugging",
            checked.error_count(),
            path.display()
        ));
    }
    keel_dap::run_dap_session(&checked.graph, runtime, keel_dap::SessionMode::Run).await
}

/// `keel test --debug --filter <name> <file>` — debug exactly one test.
///
/// # Errors
///
/// Returns an error if the file cannot be read, parsed, or type-checked; if
/// `filter` matches zero or more than one test; or if the debugged test
/// fails.
pub async fn debug_test_file(
    path: &Path,
    runtime: Arc<RuntimeContext>,
    filter: Option<&str>,
) -> Result<()> {
    let (src, name) = load_source(path)?;
    let checked = session::load_and_check_graph(&src, &name, Some(path))?;
    if checked.has_errors() {
        return Err(miette::miette!(
            "{} type error(s) in {} — fix before debugging",
            checked.error_count(),
            path.display()
        ));
    }

    let matches = session::graph_test_names(&checked, filter);
    let test_name = match matches.as_slice() {
        [name] => name.clone(),
        [] => {
            return Err(miette::miette!(
                "--debug requires --filter to match exactly one test, but none matched in {}",
                path.display()
            ));
        }
        names => {
            return Err(miette::miette!(
                "--debug requires --filter to match exactly one test, but {} matched in {}: {}",
                names.len(),
                path.display(),
                names.join(", ")
            ));
        }
    };

    keel_dap::run_dap_session(
        &checked.graph,
        runtime,
        keel_dap::SessionMode::Test { name: test_name },
    )
    .await
}
