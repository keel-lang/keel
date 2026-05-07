use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, NamedSource, Result};
use std::fs;
use std::path::PathBuf;

use keel_lang::{formatter, interpreter, lexer, lint, lsp, parser, repl, runtime, types, vm};

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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // `--trace` flips the runtime's trace flag. Stdlib modules that
    // need to decide whether to print internal detail check it via
    // `runtime::trace_enabled()`. `KEEL_TRACE=1` in the env seeds the
    // same flag on first read.
    if cli.trace {
        runtime::set_trace(true);
    }

    // `--log-level <lvl>` sets the Log namespace's threshold. Validate
    // up-front so a typo fails fast instead of silently falling back
    // to the default.
    if let Some(level) = &cli.log_level
        && !runtime::set_log_threshold(level)
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
        Commands::Run { file } => run_file(&file).await,
        Commands::Check { file, strict } => check_file(&file, strict),
        Commands::Init { name } => init_project(name),
        Commands::Repl => repl::start().await,
        Commands::Fmt { file } => fmt_file(&file),
        Commands::Build { file } => build_file(&file),
        Commands::Lint { file, fix } => lint_file(&file, fix),
        Commands::Lsp => {
            lsp::start().await;
            Ok(())
        }
    }
}

async fn run_file(path: &PathBuf) -> Result<()> {
    // If it's a compiled .keelc file, run via the bytecode VM
    if path.extension().map(|e| e == "keelc").unwrap_or(false) {
        let bytes = fs::read(path)
            .into_diagnostic()
            .map_err(|e| miette::miette!("Could not read '{}': {}", path.display(), e))?;
        let program: vm::bytecode::CompiledProgram =
            serde_json::from_slice(&bytes).into_diagnostic()?;
        let mut machine = vm::machine::VM::new();
        machine
            .execute(&program)
            .map_err(|e| miette::miette!("VM error: {e}"))?;
        return Ok(());
    }

    let source = fs::read_to_string(path)
        .into_diagnostic()
        .map_err(|e| miette::miette!("Could not read '{}': {}", path.display(), e))?;

    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let named_src = NamedSource::new(&filename, source.clone());

    // Lex
    let tokens = lexer::lex(&source, &named_src)?;

    // Parse
    let program = parser::parse(tokens, source.len(), &named_src)?;

    // Type check
    let errors = types::checker::check(&program);
    if !errors.is_empty() {
        for err in &errors {
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
        return Err(miette::miette!(
            "{} type error(s) in {}",
            errors.len(),
            path.display()
        ));
    }

    // Interpret
    interpreter::run_with_source(program, Some(named_src.clone()), Some(path)).await?;

    Ok(())
}

fn check_file(path: &PathBuf, strict: bool) -> Result<()> {
    let source = fs::read_to_string(path)
        .into_diagnostic()
        .map_err(|e| miette::miette!("Could not read '{}': {}", path.display(), e))?;

    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let named_src = NamedSource::new(&filename, source.clone());

    // Lex
    let tokens = lexer::lex(&source, &named_src)?;

    // Parse
    let program = parser::parse(tokens, source.len(), &named_src)?;

    // Type check
    let errors = if strict {
        types::checker::check_strict(&program)
    } else {
        types::checker::check(&program)
    };
    if !errors.is_empty() {
        for err in &errors {
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
        return Err(miette::miette!(
            "{} type error(s) in {}",
            errors.len(),
            path.display()
        ));
    }

    eprintln!("✓ {} is valid", path.display());
    Ok(())
}

fn build_file(path: &PathBuf) -> Result<()> {
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

    // Type check first
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

    // Compile to bytecode
    let compiled =
        vm::compiler::compile(&program).map_err(|e| miette::miette!("Compilation error: {e}"))?;

    // Write bytecode to .keelc file
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

fn fmt_file(path: &PathBuf) -> Result<()> {
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
    let formatted = formatter::format_program(&program);

    fs::write(path, &formatted).into_diagnostic()?;
    eprintln!("✓ Formatted {}", path.display());
    Ok(())
}

fn lint_file(path: &PathBuf, fix: bool) -> Result<()> {
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

    // Type-check first — lint only runs on valid programs.
    let type_errors = types::checker::check(&program);
    if !type_errors.is_empty() {
        for err in &type_errors {
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

/// Remove lines corresponding to fixable warnings. Spans are processed in
/// reverse byte order so earlier removals don't shift later positions.
fn apply_lint_fixes(source: &str, warnings: &[lint::LintWarning]) -> String {
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

    // Reverse order so byte positions remain valid as we remove slices.
    ranges.sort_by(|a, b| b.0.cmp(&a.0));
    ranges.dedup();

    let mut result = source.to_string();
    for (start, end) in ranges {
        result.replace_range(start..end, "");
    }
    result
}

fn init_project(name: Option<String>) -> Result<()> {
    let (project_name, dir, in_place) = match name {
        Some(n) => {
            let dir = PathBuf::from(&n);
            if dir.exists() {
                return Err(miette::miette!("Directory '{}' already exists", n));
            }
            fs::create_dir_all(&dir)
                .into_diagnostic()
                .map_err(|e| miette::miette!("Failed to create directory: {e}"))?;
            let project_name = dir
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(n);
            (project_name, dir, false)
        }
        None => {
            let cwd = std::env::current_dir()
                .into_diagnostic()
                .map_err(|e| miette::miette!("Cannot read current directory: {e}"))?;
            let project_name = cwd
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "my_agent".to_string());
            (project_name, cwd, true)
        }
    };

    let main_keel_path = dir.join("main.keel");
    if main_keel_path.exists() {
        return Err(miette::miette!("main.keel already exists"));
    }

    // main.keel
    let main_keel = format!(
        r#"# {project_name} — built with Keel

agent {agent_name} {{
  @role "Describe what this agent does"

  @on_start {{
    Io.show("Hello from {project_name}!")
    stop(self)
  }}
}}

run({agent_name})
"#,
        agent_name = to_pascal_case(&project_name)
    );
    fs::write(&main_keel_path, main_keel).into_diagnostic()?;

    // .gitignore
    let gitignore_path = dir.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(gitignore_path, "*.log\n.env\n").into_diagnostic()?;
    }

    eprintln!("✓ Initialized project '{project_name}'");
    if in_place {
        eprintln!("  main.keel");
        eprintln!();
        eprintln!("  Run it:  keel run main.keel");
    } else {
        eprintln!("  {}/main.keel", project_name);
        eprintln!();
        eprintln!("  Run it:  keel run {}/main.keel", project_name);
    }
    Ok(())
}

fn to_pascal_case(s: &str) -> String {
    s.split(['_', '-', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect()
}
