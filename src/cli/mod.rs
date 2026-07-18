//! Command-line interface for the Keel binary.

mod init;

use clap::{Parser, Subcommand};
use miette::Result;
use std::path::PathBuf;

use crate::runtime::context::{NativeEnv, RuntimeConfig, RuntimeContext};
use crate::{dap, lsp, pipeline, repl};

#[derive(Parser)]
#[command(
    name = "keel",
    version,
    about = "Keel — AI agents as first-class citizens"
)]
struct Cli {
    /// Print internal runtime detail: LLM call metadata, input previews,
    /// per-call results, provider banner. Off by default.
    #[arg(long, global = true)]
    trace: bool,

    /// Log threshold for the program's `Log.*` calls: debug, info, warn,
    /// or error. Default: info. Can also be set via `KEEL_LOG_LEVEL` or
    /// at runtime via `Log.set_level("...")`.
    #[arg(long, global = true, value_name = "LEVEL")]
    log_level: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute an Keel program
    Run {
        /// Path to the .keel file
        file: PathBuf,
    },
    /// Execute test blocks in a Keel program
    Test {
        /// Path to a .keel file or directory
        file: PathBuf,
        /// Run only tests whose name contains this text
        #[arg(long, value_name = "TEXT")]
        filter: Option<String>,
        /// List matching tests without running them
        #[arg(long)]
        list: bool,
        /// Stop after the first failing test
        #[arg(long)]
        fail_fast: bool,
        /// Print only failures and the final summary
        #[arg(long)]
        quiet: bool,
        /// Debug the single test matched by --filter over the Debug Adapter
        /// Protocol (stdio) instead of running it directly
        #[arg(long)]
        debug: bool,
    },
    /// Type-check an Keel program without executing
    Check {
        /// Path to the .keel file
        file: PathBuf,
        /// Reject bindings whose type the checker cannot resolve (Ty::Unknown)
        #[arg(long)]
        strict: bool,
    },
    /// Scaffold a new Keel project
    Init {
        /// Project name (defaults to current directory name)
        name: Option<String>,
    },
    /// Interactive REPL
    Repl,
    /// Format an Keel file
    Fmt {
        /// Path to the .keel file
        file: PathBuf,
    },
    /// Compile a Keel file to a native binary (not yet implemented)
    ///
    /// `keel build` is the future LLVM AOT backend entry point
    /// (`designs/llvm-compilation.md`). M0 wires the pipeline up to the KIR
    /// skeleton only: `--emit=kir` type-checks the file and prints its
    /// mid-level IR dump. Without `--emit`, the command errors — there is no
    /// codegen yet.
    Build {
        /// Path to the .keel file
        file: PathBuf,
        /// Intermediate representation to print instead of compiling.
        /// Only `kir` is implemented today.
        #[arg(long, value_name = "FORMAT")]
        emit: Option<String>,
    },
    /// Run style and best-practice checks on a Keel file
    Lint {
        /// Path to the .keel file
        file: PathBuf,
        /// Automatically remove safe single-line fixable warnings
        #[arg(long)]
        fix: bool,
    },
    /// Start the Language Server Protocol server
    Lsp,
    /// Run a Keel program under the Debug Adapter Protocol (stdio)
    Dap {
        /// Path to the .keel file
        file: PathBuf,
    },
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    // CLI and env settings seed this command's runtime context. They
    // are not process-global, so sibling Keel programs remain isolated.
    let mut runtime_config = RuntimeConfig::from_env(&NativeEnv);
    if cli.trace {
        runtime_config.set_trace(true);
    }

    // `--log-level <lvl>` sets the Log namespace's threshold. Validate
    // up-front so a typo fails fast instead of silently falling back
    // to the default.
    if let Some(level) = &cli.log_level
        && !runtime_config.set_log_threshold(level)
    {
        return Err(miette::miette!(
            "--log-level: `{level}` is not a valid level (expected debug|info|warn|error)"
        ));
    }
    // Top-level SIGINT watcher: exits the process regardless of what
    // the interpreter is blocked on (stdin read in `Io.ask` /
    // `Io.confirm`, IMAP fetch, HTTP request, in-flight LLM call).
    // The event loop in `Interpreter::execute` has its own Ctrl-C
    // branch for graceful shutdown when the program is idle; this
    // watcher is the hard-exit fallback so a user pressing Ctrl-C
    // never has to press Enter first.
    //
    // 130 is the standard SIGINT exit code. Repl uses rustyline's
    // own Ctrl-C handling, so suppress the watcher there.
    if !matches!(cli.command, Commands::Repl) {
        tokio::spawn(async {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!();
                std::process::exit(130);
            }
        });
    }

    match cli.command {
        Commands::Run { file } => {
            let runtime = RuntimeContext::native_with_config(runtime_config);
            pipeline::run_file_with_runtime(&file, runtime).await
        }
        Commands::Test {
            file,
            filter,
            list,
            fail_fast,
            quiet,
            debug,
        } => {
            let runtime = RuntimeContext::native_with_config(runtime_config);
            if debug {
                let result = dap::debug_test_file(&file, runtime, filter.as_deref()).await;
                exit_after_dap_session(result)
            } else {
                pipeline::test_file_with_runtime(
                    &file,
                    runtime,
                    filter.as_deref(),
                    list,
                    fail_fast,
                    quiet,
                )
                .await
            }
        }
        Commands::Check { file, strict } => pipeline::check_file(&file, strict),
        Commands::Init { name } => init::project(name),
        Commands::Repl => {
            let runtime = RuntimeContext::native_with_config(runtime_config);
            repl::start_with_runtime(runtime).await
        }
        Commands::Fmt { file } => pipeline::fmt_file(&file),
        Commands::Build { file, emit } => pipeline::build_file(&file, emit.as_deref()),
        Commands::Lint { file, fix } => pipeline::lint_file(&file, fix),
        Commands::Lsp => {
            lsp::start().await;
            Ok(())
        }
        Commands::Dap { file } => {
            let runtime = RuntimeContext::native_with_config(runtime_config);
            let result = dap::run_file(&file, runtime).await;
            exit_after_dap_session(result)
        }
    }
}

/// `keel dap`/`keel test --debug` read from stdin in a loop racing against
/// the interpreter via `tokio::select!`. When the interpreter finishes
/// first, the stdin read Tokio dispatched to its blocking-thread pool is
/// still in flight — it can't be cancelled, and returning normally from
/// `main` blocks process exit on that thread until the DAP client closes
/// its end of stdin. Exiting directly (the same technique the Ctrl-C
/// watcher above uses) skips that wait entirely.
fn exit_after_dap_session(result: Result<()>) -> ! {
    match result {
        Ok(()) => std::process::exit(0),
        Err(err) => {
            eprintln!("{err:?}");
            std::process::exit(1);
        }
    }
}
