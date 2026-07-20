//! The `Engine` abstraction the conformance harness diffs across.
//!
//! M0 shipped exactly one real engine (`Interpreter`); `Compiled` was wired
//! in as a stub returning [`EngineError::NotImplemented`] so the corpus
//! loop, skip list, and diff logic in `main.rs` already had the shape a
//! second engine would need. M1 (issue #136) fills in the real
//! implementation, feature-gated behind `build-backend` (the root crate's
//! optional dependency on `keel-codegen`, which is the only crate here that
//! links LLVM) so this harness still runs — with `Compiled` degrading back
//! to `NotImplemented` — on a checkout without the LLVM toolchain installed.
//!
//! Unlike `Interpreter` (calls straight into the library, output captured by
//! `main.rs`'s fd-1 redirection around the current process), `Compiled`
//! spawns the built binary as a **separate process** and captures its
//! output via a piped `Command` instead — deliberately *not* by having the
//! child inherit the parent's (possibly fd-1-redirected) stdio. An earlier
//! version of this tried exactly that inheritance trick and it was
//! intermittently flaky: `tokio::process::Command::status()` (which waits
//! for the child to exit) does not guarantee the *pipe/fd cleanup* around
//! that exit is synchronized with the parent's next `dup2` restore closely
//! enough for this to be reliable across repeated runs in one process. A
//! piped child (`Command::output()`) sidesteps the question entirely: its
//! stdout never touches the parent's fd 1, so there is nothing for the
//! surrounding `capture_stdout` machinery to race with.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use keel_lang::session;
use keel_runtime::runtime::context::RuntimeContext;

/// The result the conformance harness diffs: an exit code plus the
/// program's stdout (assembled differently per engine — see the module
/// doc — but identical in shape either way).
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
    /// `Engine::Compiled` without the `build-backend` feature — no LLVM
    /// toolchain, so no backend to run. Only ever constructed in that
    /// build configuration; harmless dead-code with it on.
    #[cfg_attr(feature = "build-backend", allow(dead_code))]
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
    /// The native/LLVM backend. With `build-backend` on, `main.rs`'s M1
    /// comparison calls [`run_compiled`] directly (it needs stdout, which
    /// this variant's `Engine::run` doesn't expose) rather than constructing
    /// this variant — harmless dead code in that configuration.
    #[cfg_attr(feature = "build-backend", allow(dead_code))]
    Compiled,
}

impl Engine {
    /// Runs `path` under this engine, returning its exit code (`0` on a
    /// clean `Ok(())`, `1` on any runtime error — byte-for-byte stderr
    /// diffing across engines is a later milestone's concern, not M0's).
    /// Used only by `main.rs`'s interpreter-vs-interpreter loop and (when
    /// `build-backend` is off) the `Compiled`-stub regression check —
    /// `main.rs`'s M1 interpreter-vs-compiled comparison calls
    /// [`run_compiled`] directly instead, since it needs stdout too.
    pub async fn run(&self, path: &Path) -> Result<i32, EngineError> {
        match self {
            Engine::Interpreter => run_interpreter(path).await,
            #[cfg(feature = "build-backend")]
            Engine::Compiled => run_compiled(path, None).await.map(|o| o.exit_code),
            #[cfg(not(feature = "build-backend"))]
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

/// Parses, lowers to KIR, compiles, and runs `path` as a native binary,
/// returning its exit code *and* captured stdout (via a piped child, not
/// fd-1 inheritance — see the module doc). `workdir`, if given, becomes the
/// child's cwd (`Command::current_dir` — no need to `chdir` this process,
/// unlike the interpreter path, since the child gets its own process-wide
/// cwd for free).
///
/// `LoadFailed` covers the whole front half of the pipeline (parse, lower,
/// codegen) — a corpus fixture curated for M1 is expected to sail through
/// all of it, so a failure at any stage is a harness-level problem, same as
/// `run_interpreter`'s parse/check failures.
#[cfg(feature = "build-backend")]
pub async fn run_compiled(
    path: &Path,
    workdir: Option<&Path>,
) -> Result<EngineOutput, EngineError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| EngineError::LoadFailed(format!("read {}: {e}", path.display())))?;
    let name = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_string());

    let (program, _named) = keel_syntax::parse_source(&source, &name)
        .map_err(|e| EngineError::LoadFailed(format!("parse: {e:?}")))?;
    let kir = keel_kir::lower(&program, &name)
        .map_err(|e| EngineError::LoadFailed(format!("lower to KIR: {e}")))?;

    let out_dir = tempfile::tempdir()
        .map_err(|e| EngineError::LoadFailed(format!("create codegen out_dir: {e}")))?;
    let opts = keel_codegen::BuildOptions {
        out_dir: out_dir.path().to_path_buf(),
        runtime_link_args: crate::runtime_link::runtime_link_args().clone(),
    };
    let bin = keel_codegen::compile(&kir, &opts)
        .map_err(|e| EngineError::LoadFailed(format!("codegen: {e}")))?;

    let mut cmd = tokio::process::Command::new(&bin);
    if let Some(dir) = workdir {
        cmd.current_dir(dir);
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| EngineError::LoadFailed(format!("spawn compiled binary: {e}")))?;

    Ok(EngineOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        exit_code: output.status.code().unwrap_or(1),
    })
}
