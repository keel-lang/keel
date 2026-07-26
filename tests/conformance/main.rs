//! Conformance harness — M0 of `designs/llvm-compilation.md` (issue #111).
//!
//! Runs every program in the corpus (`examples/*.keel` +
//! `tests/conformance/fixtures/*.keel`) under an [`Engine`] and diffs the
//! result (stdout + exit code) across engines. M0 has exactly one working
//! engine (the tree-walking interpreter), so this proves the plumbing and
//! determinism by running the interpreter twice and diffing its own output
//! against itself — the exit criterion in the design doc §4 M0: "harness
//! runs green interpreter-vs-interpreter (infrastructure proven)".
//!
//! `Engine::Compiled` is wired into the enum and the diff machinery now, as
//! a stub, so M1+ only has to fill in one match arm — the corpus, the skip
//! list, the timeout, and the diffing logic do not change shape when a real
//! second engine shows up.
//!
//! # Why in-process, not `cargo run`/a subprocess
//!
//! The task calls for invoking the library pipeline directly rather than
//! shelling out. `Engine::Interpreter::run` calls
//! `keel_lang`'s public `session::load_and_check_graph` /
//! `session::run_graph` (via the `keel_runtime`/`keel_compiler` crates
//! directly — `keel-lang`'s own `pipeline`/`interpreter` modules are
//! `pub(crate)`, so an external integration test binary reaches the same
//! machinery through the lower-level crates instead) as ordinary Rust
//! function calls in this test process, not a spawned `keel` binary.
//!
//! Because every namespace method (`io.show`, `log.*`, …) writes to real
//! process stdout via `println!` (no injectable writer exists in the
//! runtime today — out of scope to add here), capturing a single program's
//! output requires redirecting fd 1 for the duration of that one call; see
//! [`capture_stdout`]. That's safe here specifically because this whole
//! corpus runs from **one sequential test function** in **its own test
//! binary process** (`tests/conformance/main.rs` — Cargo gives every
//! `tests/*.rs`/`tests/<dir>/main.rs` file its own binary): no other test
//! shares this process, and nothing here spawns concurrent programs, so
//! fd 1 is never contended.
//!
//! This is a hard requirement, not just tidy structure: a second
//! `#[tokio::test]` in this binary — even one that never touches fd 1
//! itself — is enough to break it. libtest runs sibling tests concurrently
//! by default and writes each one's `test <name> ... ok` status line to
//! real stdout (fd 1) some time after that test's future resolves (the
//! print is not synchronous with the function returning — it goes through
//! libtest's own result-reporting path). A mutex serializing the two test
//! *bodies* was tried and was not sufficient: it narrows the race but the
//! delayed status print can still land inside this test's `capture_stdout`
//! window regardless of body ordering. The only fully robust fix is what
//! this file does now — exactly one `#[tokio::test]` in the binary, so
//! there is only ever one status line, printed once, after everything here
//! has already finished.
//!
//! Unix-only (`libc::dup`/`dup2`) — deferred for Windows, same as the rest
//! of the native-backend work this issue is scaffolding for.
//!
//! The M1 interpreter-vs-compiled comparison (issue #136) is the one
//! exception to "in-process, not a subprocess": it runs *both* engines as
//! piped child processes (see `run_interpreter_subprocess_one` and
//! `run_compiled_one`) rather than reusing this fd-1 dance, since mixing
//! `capture_stdout` with the compiled engine's subprocess spawning in the
//! same test lost the interpreter's captured output. The M0 corpus loop
//! below is unaffected — it stays in-process, per the reasoning above.
//!
//! M2 (issue #151, the milestone-closing issue for `designs/llvm-
//! compilation.md`'s M2) reuses this exact same subprocess-comparison shape
//! against a second, separately curated fixture set
//! (`fixtures/m2_features/`, discovered via
//! [`discover_m2_features_fixtures`]) spanning all six M2 features: named
//! structs + spread-update, enums + `when` (statement and expression
//! form), containers (list/map/set, including a list CoW-aliasing case),
//! nullable (`?`, `??`, `?.`), string interpolation, and raise/try/catch —
//! plus one fixture combining several of them in a single program. No new
//! harness machinery was needed, exactly as #151 anticipated: just a
//! second fixture-discovery call and comparison loop, sharing
//! `run_interpreter_subprocess_one`/`run_compiled_one` with the M1 loop.

mod corpus;
mod engine;
#[cfg(feature = "build-backend")]
mod runtime_link;

use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::os::unix::io::AsRawFd as _;
use std::path::Path;
use std::time::Duration;

use corpus::discover_corpus;
#[cfg(feature = "build-backend")]
use corpus::{discover_m1_scalar_fixtures, discover_m2_features_fixtures};
use engine::{Engine, EngineError, EngineOutput};

/// Per-program wall-clock budget. Generous enough for the mock LLM path and
/// agent handshake overhead, tight enough that a genuinely hung program
/// (e.g. missing `KEEL_ONESHOT` idle-exit, an accidental real network call)
/// fails the run instead of stalling `cargo test` indefinitely.
const PER_PROGRAM_TIMEOUT: Duration = Duration::from_secs(20);

/// Runs `f` with fd 1 redirected to a fresh temp file, returning `f`'s
/// result together with everything written to stdout during the call. See
/// the module doc comment for why this is safe in this specific harness.
async fn capture_stdout<Fut, T>(f: Fut) -> (T, String)
where
    Fut: std::future::Future<Output = T>,
{
    std::io::stdout().flush().ok();
    let mut tmp = tempfile::tempfile().expect("create stdout-capture tempfile");
    // SAFETY: dup/dup2/close on well-formed fds we own (a fresh tempfile fd
    // and the process's real stdout fd), matching the standard
    // save/redirect/restore dance (what `gag::BufferRedirect` does
    // internally). No other thread in this process touches fd 1 for the
    // duration — see the module doc comment.
    let saved_stdout = unsafe { libc::dup(libc::STDOUT_FILENO) };
    assert!(saved_stdout >= 0, "dup(STDOUT_FILENO) failed");
    let redirect_result = unsafe { libc::dup2(tmp.as_raw_fd(), libc::STDOUT_FILENO) };
    assert!(redirect_result >= 0, "dup2 onto capture file failed");

    let result = f.await;

    std::io::stdout().flush().ok();
    // SAFETY: `saved_stdout` was produced by the `dup` above and is closed
    // exactly once, right here.
    unsafe {
        libc::dup2(saved_stdout, libc::STDOUT_FILENO);
        libc::close(saved_stdout);
    }

    tmp.seek(SeekFrom::Start(0)).expect("seek capture tempfile");
    let mut captured = String::new();
    tmp.read_to_string(&mut captured)
        .expect("capture tempfile must be valid UTF-8");
    (result, strip_slow_test_watchdog_noise(captured))
}

/// Strips libtest's own "slow test" watchdog line from captured output.
///
/// This is not a sibling test writing to fd 1 (the risk the module doc
/// already calls out) — it's the test harness's *own* built-in monitor
/// thread, which unconditionally prints `test <name> has been running for
/// over N seconds` to the process's real stdout if this single
/// `#[tokio::test]` (which now runs the entire corpus, plus the M1/M2
/// subprocess comparisons, sequentially) takes longer than the harness's
/// slow-test threshold — observed in CI once the M2 fixture set (#151)
/// pushed total runtime close to it, even though it stays comfortably under
/// locally. There is no stable flag to disable that monitor thread, and the
/// alternative (redirecting the *interpreter's* own output through an
/// injectable writer instead of OS-level fd 1) is the "no injectable writer
/// exists in the runtime today" gap the module doc already defers. Whichever
/// corpus entry happens to be mid-capture when the monitor fires gets this
/// line spliced into its otherwise-deterministic output — filtering the
/// exact, recognizable line here is the narrow fix for that, not a general
/// license to scrub arbitrary "unexpected" output.
fn strip_slow_test_watchdog_noise(captured: String) -> String {
    const MARKER: &str = "has been running for over";
    // Fast path: the overwhelming majority of captures never see the
    // watchdog fire at all — return byte-identical in that case rather than
    // risk altering a program's output that doesn't end in a trailing
    // newline via a lines()-based rebuild.
    if !captured.contains(MARKER) {
        return captured;
    }
    // `split('\n')` (not `.lines()`) preserves the exact segment structure
    // — including a trailing empty segment when `captured` ends in `\n` —
    // so rejoining with `\n` reconstructs byte-identically except for the
    // one filtered-out line.
    captured
        .split('\n')
        .filter(|line| !(line.starts_with("test ") && line.contains(MARKER)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs one program under `engine`, in an isolated temp working directory
/// (several examples write relative-path files — `file_processing.keel` and
/// friends — isolating cwd keeps the repo tree clean and runs independent of
/// what earlier corpus entries left behind) and under
/// [`PER_PROGRAM_TIMEOUT`].
async fn run_one(engine: Engine, path: &Path) -> Result<EngineOutput, EngineError> {
    let path = path
        .canonicalize()
        .unwrap_or_else(|e| panic!("canonicalize {}: {e}", path.display()));
    let workdir = tempfile::tempdir().expect("create isolated run workdir");
    let previous_dir = std::env::current_dir().expect("read current dir");
    std::env::set_current_dir(workdir.path()).expect("chdir into isolated workdir");

    let (timeout_outcome, stdout) = capture_stdout(async {
        tokio::time::timeout(PER_PROGRAM_TIMEOUT, engine.run(&path)).await
    })
    .await;

    std::env::set_current_dir(&previous_dir).expect("chdir back to original dir");

    match timeout_outcome {
        Ok(Ok(exit_code)) => Ok(EngineOutput { stdout, exit_code }),
        Ok(Err(engine_err)) => Err(engine_err),
        Err(_elapsed) => Err(EngineError::TimedOut(PER_PROGRAM_TIMEOUT)),
    }
}

/// The interpreter-side counterpart to [`run_compiled_one`], used only by
/// the M1 comparison below — spawns the already-built `keel` binary
/// (`CARGO_BIN_EXE_keel`, set by Cargo for this integration-test target
/// since the root package defines `[[bin]] name = "keel"`) as a piped child,
/// rather than going through [`run_one`]'s in-process fd-1 capture.
///
/// This exists because mixing `run_one`'s fd-1 dup/dup2 dance with
/// `run_compiled_one`'s subprocess-spawning in the same test lost the
/// interpreter's captured output intermittently (and, once the big corpus
/// loop below also ran in-process in the same test binary, *deterministically*):
/// nothing here pinned down which tokio-internal timing caused it, and
/// `capture_stdout`'s own doc comment already flags it as fragile to any
/// shift in the process's async/fd state. Running the interpreter as a
/// piped child too sidesteps the question entirely — same shape as
/// `run_compiled_one`, and it never touches this process's fd 1.
#[cfg(feature = "build-backend")]
async fn run_interpreter_subprocess_one(path: &Path) -> Result<EngineOutput, EngineError> {
    let path = path
        .canonicalize()
        .unwrap_or_else(|e| panic!("canonicalize {}: {e}", path.display()));
    let workdir = tempfile::tempdir().expect("create isolated run workdir");

    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_keel"));
    cmd.arg("run")
        .arg(&path)
        .env("KEEL_LLM", "mock")
        .env("KEEL_ONESHOT", "1")
        .current_dir(workdir.path());

    match tokio::time::timeout(PER_PROGRAM_TIMEOUT, cmd.output()).await {
        Ok(Ok(output)) => Ok(EngineOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            exit_code: output.status.code().unwrap_or(1),
        }),
        Ok(Err(e)) => Err(EngineError::LoadFailed(format!("spawn `keel run`: {e}"))),
        Err(_elapsed) => Err(EngineError::TimedOut(PER_PROGRAM_TIMEOUT)),
    }
}

/// The `Engine::Compiled` analogue of [`run_one`] — same isolated-workdir
/// and timeout treatment, but no fd-1 capture: `engine::run_compiled`
/// captures the child's stdout itself via a piped `Command`, deliberately
/// not sharing this process's stdio at all (see `engine.rs`'s module doc
/// for why `run_one`'s fd-1 approach isn't used here). `workdir` becomes
/// the child's cwd directly, so there's no `chdir`/restore dance on this
/// process either.
#[cfg(feature = "build-backend")]
async fn run_compiled_one(path: &Path) -> Result<EngineOutput, EngineError> {
    let path = path
        .canonicalize()
        .unwrap_or_else(|e| panic!("canonicalize {}: {e}", path.display()));
    let workdir = tempfile::tempdir().expect("create isolated run workdir");

    match tokio::time::timeout(
        PER_PROGRAM_TIMEOUT,
        engine::run_compiled(&path, Some(workdir.path())),
    )
    .await
    {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(engine_err)) => Err(engine_err),
        Err(_elapsed) => Err(EngineError::TimedOut(PER_PROGRAM_TIMEOUT)),
    }
}

/// Programs excluded from the corpus, one line each with a concrete reason.
/// Categories: real network I/O, interactive stdin, outbound email,
/// non-deterministic output (random/uuid/wall-clock), a real subprocess
/// whose output depends on repo/filesystem state, a long-running HTTP
/// server, and concurrent multi-agent dispatch with no defined handler
/// ordering. None of these are specific to the interpreter-vs-interpreter
/// determinism check this harness runs today — they'd need the same
/// exclusions once a second engine exists.
fn skip_reason(stem: &str) -> Option<&'static str> {
    match stem {
        "broadcast_team" => Some(
            "broadcast() dispatches to team members concurrently with no ordering guarantee; \
             handler output interleaving is non-deterministic by design, not a determinism bug",
        ),
        "parallel_execution" => Some(
            "async.spawn'd tasks race to completion with equal sleep durations; \
             `Task N completed` ordering is non-deterministic by design (verified: 8/8 \
             manual runs produced differing interleavings), not a determinism bug",
        ),
        "capability_gating_fail" => Some("performs a real outbound HTTP request"),
        "http_demo" => Some("performs real outbound HTTP requests"),
        "webhook_agent" => Some("starts a long-running HTTP server (http.serve)"),
        "code_reviewer" => Some("blocks on interactive stdin (io.confirm/io.ask)"),
        "customer_support" => Some("blocks on interactive stdin and sends real email"),
        "multi_agent_inbox" => Some("blocks on interactive stdin and sends real email"),
        "email_agent" => {
            Some("blocks on interactive stdin, sends real email, and prints wall-clock time")
        }
        "daily_digest" => Some("sends real email"),
        "data_pipeline" => Some("sends real email"),
        "if_guard" => Some("sends real email"),
        "tool_guards" => Some("sends real email and opens a real db connection"),
        "db_trade_log" => Some("opens a real db connection and prints wall-clock time"),
        "random_demo" => Some("non-deterministic: crypto/random-backed output"),
        "crypto_demo" => Some("non-deterministic: crypto.random_bytes output"),
        "uuid_demo" => Some("non-deterministic: uuid.v4()/v7() are random"),
        "time_basic" => Some("non-deterministic: prints wall-clock time"),
        "cron_schedule" => {
            Some("wall-clock cron scheduling — timing-sensitive, not a pure determinism check")
        }
        "shell_bridge" => Some(
            "spawns real subprocesses whose output depends on repo/filesystem state (ls *.keel)",
        ),
        _ => None,
    }
}

#[tokio::test]
async fn interpreter_is_deterministic_across_the_corpus() {
    // KEEL_LLM=mock keeps `ai.*` deterministic and offline; KEEL_ONESHOT=1
    // makes agent programs exit after one idle window instead of serving
    // forever. Set once, process-wide — safe because this whole corpus runs
    // sequentially from this one test function in its own test-binary
    // process (see the module doc comment), and nothing else in this
    // process reads env vars concurrently.
    // SAFETY: process-wide env mutation is unsafe in general (data races with
    // concurrent readers), but this runs before any corpus program (hence
    // any concurrent env reader) starts.
    unsafe {
        std::env::set_var("KEEL_LLM", "mock");
        std::env::set_var("KEEL_ONESHOT", "1");
    }

    // Without the LLVM toolchain (`build-backend` off), `Engine::Compiled`
    // stays a stub — exercise that contract directly (not through the
    // corpus loop below) so it has its own regression coverage independent
    // of which examples happen to be skipped. Calls `Engine::run` directly
    // rather than through `run_one`: that helper redirects fd 1 and changes
    // the process cwd, which this doesn't need.
    #[cfg(not(feature = "build-backend"))]
    {
        let fixtures = corpus::fixtures_dir();
        let path = fixtures.join("agent_lifecycle.keel");
        let err = Engine::Compiled
            .run(&path)
            .await
            .expect_err("Engine::Compiled has no backend yet");
        assert!(matches!(err, EngineError::NotImplemented));
    }

    // Issue #136 (M1's own exit criterion): the compiled engine, run on a
    // dedicated scalar-only fixture set (M1's scope — int/float/bool/str
    // literals, arithmetic, if/while/for-over-ranges, direct task calls,
    // io.show), must produce byte-identical stdout and exit codes to the
    // interpreter. Both sides run as piped child processes here (see
    // `run_interpreter_subprocess_one` and `run_compiled_one`) rather than
    // going through `run_one`'s in-process fd-1 capture — that capture
    // mechanism is documented as fragile to shifts in the process's
    // async/fd state, and mixing it with `run_compiled_one`'s subprocess
    // spawning in this same test lost the interpreter's captured output
    // intermittently. Position relative to the corpus loop below no longer
    // matters, since neither side of this comparison touches this
    // process's fd 1 at all.
    #[cfg(feature = "build-backend")]
    {
        let fixtures = discover_m1_scalar_fixtures();
        assert!(
            !fixtures.is_empty(),
            "expected a non-empty M1-scalar conformance fixture set"
        );

        let mut m1_failures = Vec::new();
        for entry in &fixtures {
            let interpreted = run_interpreter_subprocess_one(&entry.path).await;
            let compiled = run_compiled_one(&entry.path).await;
            match (interpreted, compiled) {
                (Ok(a), Ok(b)) if a == b => {}
                (Ok(a), Ok(b)) => m1_failures.push(format!(
                    "{}: compiled engine diverges from the interpreter:\n  interpreter: exit={} stdout={:?}\n  compiled:    exit={} stdout={:?}",
                    entry.stem, a.exit_code, a.stdout, b.exit_code, b.stdout
                )),
                (Err(e), _) | (_, Err(e)) => {
                    m1_failures.push(format!("{}: engine error: {e}", entry.stem));
                }
            }
        }

        eprintln!(
            "conformance (M1 scalar, interpreter vs. compiled): {} run, {} failed",
            fixtures.len(),
            m1_failures.len()
        );
        assert!(
            m1_failures.is_empty(),
            "M1 conformance failures ({}):\n{}",
            m1_failures.len(),
            m1_failures.join("\n")
        );
    }

    // Issue #151 (M2's own exit criterion, = #113's exit criterion refined
    // to the curated-fixture-set reality — see this file's module doc): the
    // compiled engine, run on the curated M2-scope fixture set (structs,
    // enums/`when`, containers, nullable, string interpolation, raise/try/
    // catch), must produce byte-identical stdout and exit codes to the
    // interpreter. Same subprocess-comparison shape as the M1 loop above,
    // for the same reasons (see that block's comment).
    #[cfg(feature = "build-backend")]
    {
        let fixtures = discover_m2_features_fixtures();
        assert!(
            !fixtures.is_empty(),
            "expected a non-empty M2-features conformance fixture set"
        );

        let mut m2_failures = Vec::new();
        for entry in &fixtures {
            let interpreted = run_interpreter_subprocess_one(&entry.path).await;
            let compiled = run_compiled_one(&entry.path).await;
            match (interpreted, compiled) {
                (Ok(a), Ok(b)) if a == b => {}
                (Ok(a), Ok(b)) => m2_failures.push(format!(
                    "{}: compiled engine diverges from the interpreter:\n  interpreter: exit={} stdout={:?}\n  compiled:    exit={} stdout={:?}",
                    entry.stem, a.exit_code, a.stdout, b.exit_code, b.stdout
                )),
                (Err(e), _) | (_, Err(e)) => {
                    m2_failures.push(format!("{}: engine error: {e}", entry.stem));
                }
            }
        }

        eprintln!(
            "conformance (M2 features, interpreter vs. compiled): {} run, {} failed",
            fixtures.len(),
            m2_failures.len()
        );
        assert!(
            m2_failures.is_empty(),
            "M2 conformance failures ({}):\n{}",
            m2_failures.len(),
            m2_failures.join("\n")
        );
    }

    let corpus = discover_corpus();
    assert!(
        !corpus.is_empty(),
        "expected a non-empty conformance corpus"
    );

    let mut ran = 0usize;
    let mut skipped = 0usize;
    let mut failures = Vec::new();

    for entry in &corpus {
        if let Some(reason) = skip_reason(&entry.stem) {
            skipped += 1;
            eprintln!("skip {} ({reason})", entry.stem);
            continue;
        }

        let first = run_one(Engine::Interpreter, &entry.path).await;
        let second = run_one(Engine::Interpreter, &entry.path).await;
        ran += 1;

        match (first, second) {
            (Ok(a), Ok(b)) if a == b => {}
            (Ok(a), Ok(b)) => failures.push(format!(
                "{}: interpreter is non-deterministic:\n  run 1: exit={} stdout={:?}\n  run 2: exit={} stdout={:?}",
                entry.stem, a.exit_code, a.stdout, b.exit_code, b.stdout
            )),
            (Err(e), _) | (_, Err(e)) => {
                failures.push(format!("{}: engine error: {e}", entry.stem));
            }
        }
    }

    eprintln!(
        "conformance: {ran} run, {skipped} skipped, {} failed",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "conformance failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg(test)]
mod noise_filter_tests {
    use super::strip_slow_test_watchdog_noise;

    #[test]
    fn clean_output_is_returned_byte_identical() {
        let clean = "  1. 1\n  2. 2\n".to_string();
        assert_eq!(strip_slow_test_watchdog_noise(clean.clone()), clean);
    }

    #[test]
    fn output_with_no_trailing_newline_is_untouched() {
        let clean = "no trailing newline".to_string();
        assert_eq!(strip_slow_test_watchdog_noise(clean.clone()), clean);
    }

    #[test]
    fn watchdog_line_spliced_mid_output_is_removed() {
        let polluted = "  hello\ntest interpreter_is_deterministic_across_the_corpus has been running for over 60 seconds\n  world\n".to_string();
        assert_eq!(
            strip_slow_test_watchdog_noise(polluted),
            "  hello\n  world\n"
        );
    }
}
