//! Command-line interface for the Keel binary.

mod init;

use clap::{Parser, Subcommand};
use miette::Result;
use std::path::PathBuf;

use keel_lang::runtime::context::{NativeEnv, RuntimeConfig, RuntimeContext};
use keel_lang::{lsp, pipeline, repl};

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
    /// Compile an Keel file to bytecode
    Build {
        /// Path to the .keel file
        file: PathBuf,
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
        Commands::Check { file, strict } => pipeline::check_file(&file, strict),
        Commands::Init { name } => init::project(name),
        Commands::Repl => {
            let runtime = RuntimeContext::native_with_config(runtime_config);
            repl::start_with_runtime(runtime).await
        }
        Commands::Fmt { file } => pipeline::fmt_file(&file),
        Commands::Build { file } => pipeline::build_file(&file),
        Commands::Lint { file, fix } => pipeline::lint_file(&file, fix),
        Commands::Lsp => {
            lsp::start().await;
            Ok(())
        }
    }
}
