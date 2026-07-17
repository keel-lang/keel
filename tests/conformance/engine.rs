//! The `Engine` abstraction the conformance harness diffs across.
//!
//! M0 has exactly one real engine (`Interpreter`); `Compiled` is wired in as
//! a stub returning [`EngineError::NotImplemented`] so `main.rs`'s corpus
//! loop, skip list, and diff logic already have the shape they'll need once
//! `keel-codegen` (M1+, `designs/llvm-compilation.md`) exists — only the
//! `Compiled` match arm in [`Engine::run`] changes then, presumably to spawn
//! the compiled binary as a subprocess (unlike `Interpreter`, which calls
//! straight into the library).

use std::fmt;
use std::path::Path;
use std::time::Duration;

use keel_lang::session;
use keel_runtime::runtime::context::RuntimeContext;

/// The result the conformance harness diffs: an exit code plus (assembled
/// by `main.rs`, which owns the fd-1 capture) the program's stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineOutput {
    pub stdout: String,
    pub exit_code: i32,
}

/// A harness-level failure — distinct from the *program* legitimately
/// exiting non-zero (that's `Ok(EngineOutput { exit_code: 1, .. })`: plenty
/// of examples demonstrate error handling on purpose). This is for the
/// corpus/engine machinery itself failing.
#[derive(Debug)]
pub enum EngineError {
    /// The file couldn't even be loaded/parsed/module-resolved (as opposed
    /// to type-checking cleanly and then failing at runtime, which is a
    /// normal nonzero exit).
    LoadFailed(String),
    /// `Engine::Compiled` — no backend exists yet.
    NotImplemented,
    /// Killed by `main.rs`'s per-program timeout.
    TimedOut(Duration),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::LoadFailed(msg) => write!(f, "could not load program: {msg}"),
            EngineError::NotImplemented => write!(f, "engine has no backend implemented yet"),
            EngineError::TimedOut(d) => write!(f, "timed out after {d:?}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// The tree-walking async interpreter (`keel-runtime`) — the reference
    /// semantics (`designs/llvm-compilation.md` §3).
    Interpreter,
    /// The native/LLVM backend. Does not exist yet (M1+).
    Compiled,
}

impl Engine {
    /// Runs `path` under this engine, returning its exit code (`0` on a
    /// clean `Ok(())`, `1` on any runtime error — byte-for-byte stderr
    /// diffing across engines is a later milestone's concern, not M0's).
    pub async fn run(&self, path: &Path) -> Result<i32, EngineError> {
        match self {
            Engine::Interpreter => run_interpreter(path).await,
            Engine::Compiled => Err(EngineError::NotImplemented),
        }
    }
}

async fn run_interpreter(path: &Path) -> Result<i32, EngineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| EngineError::LoadFailed(format!("read {}: {e}", path.display())))?;
    let name = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_string());

    let checked = session::load_and_check_graph(&source, &name, Some(path))
        .map_err(|e| EngineError::LoadFailed(format!("{e:?}")))?;
    if checked.has_errors() {
        return Err(EngineError::LoadFailed(format!(
            "{} type error(s) in a corpus program that is expected to be clean",
            checked.error_count()
        )));
    }

    let runtime = RuntimeContext::native();
    Ok(match session::run_graph(&checked, runtime).await {
        Ok(()) => 0,
        Err(_) => 1,
    })
}
