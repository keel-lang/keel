use colored::Colorize;
use miette::NamedSource;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::ast::*;
use crate::interpreter::{Interpreter, value::Value};
use crate::lexer;
use crate::parser;

pub async fn start() -> miette::Result<()> {
    println!("{}", "Keel REPL v0.1".bright_cyan().bold());
    println!(
        "{}",
        "Type expressions, define tasks, or :help for commands.".dimmed()
    );
    println!();

    let mut rl = DefaultEditor::new().unwrap();
    let history_path = dirs_home().join(".keel_history");
    let _ = rl.load_history(&history_path);

    let mut state = ReplState::new();

    loop {
        let prompt = "keel> ".bright_green().bold().to_string();
        match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                rl.add_history_entry(trimmed).ok();

                if trimmed.starts_with(':') {
                    match trimmed {
                        ":help" | ":h" => print_help(),
                        ":quit" | ":q" | ":exit" => break,
                        ":env" => state.print_env(),
                        ":types" => state.print_types(),
                        ":clear" => state.clear(),
                        _ => eprintln!("{}", format!("Unknown command: {trimmed}").bright_red()),
                    }
                    continue;
                }

                // Multi-line input for blocks
                let mut input = trimmed.to_string();
                while count_braces(&input) > 0 {
                    match rl.readline("  ... ") {
                        Ok(cont) => {
                            input.push('\n');
                            input.push_str(&cont);
                        }
                        Err(_) => break,
                    }
                }

                match eval_repl_input(&mut state, &input).await {
                    Ok(Some(val)) => {
                        println!("  {}", format_value(&val));
                    }
                    Ok(None) => {}
                    Err(msg) => {
                        eprintln!("  {}", msg.bright_red());
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("  {}", "(Ctrl+C) Type :quit to exit".dimmed());
            }
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("Error: {err}");
                break;
            }
        }
    }

    let _ = rl.save_history(&history_path);
    println!("{}", "Goodbye.".dimmed());
    Ok(())
}

// ---------------------------------------------------------------------------
// REPL state
// ---------------------------------------------------------------------------

struct ReplState {
    interp: Interpreter,
}

impl ReplState {
    fn new() -> Self {
        // Mock LLM + quiet agent boilerplate in the REPL.
        std::env::set_var("KEEL_LLM", "mock");
        std::env::set_var("KEEL_REPL", "1");
        ReplState {
            interp: Interpreter::new(),
        }
    }

    fn print_env(&self) {
        let names = self.interp.env_var_names();
        if names.is_empty() {
            println!("  {}", "No variables defined.".dimmed());
        } else {
            println!("  {}", "Variables:".bright_white().bold());
            for name in names {
                if let Some(val) = self.interp.env_get(&name) {
                    println!("  {} = {}", name.bright_cyan(), format_value(&val));
                }
            }
        }
    }

    fn print_types(&self) {
        let types = self.interp.types_snapshot();
        if types.is_empty() {
            println!("  {}", "No types defined.".dimmed());
        } else {
            println!("  {}", "Types:".bright_white().bold());
            for (name, def) in types {
                match def {
                    TypeDef::SimpleEnum(variants) => {
                        println!("  {} = {}", name.bright_cyan(), variants.join(" | "));
                    }
                    TypeDef::Struct(fields) => {
                        let fs: Vec<String> =
                            fields.iter().map(|f| format!("{}: ...", f.name)).collect();
                        println!("  {} {{ {} }}", name.bright_cyan(), fs.join(", "));
                    }
                    TypeDef::RichEnum(variants) => {
                        let vs: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
                        println!("  {} = {}", name.bright_cyan(), vs.join(" | "));
                    }
                }
            }
        }
    }

    fn clear(&mut self) {
        self.interp = Interpreter::new();
        println!("  {}", "State cleared.".dimmed());
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

async fn eval_repl_input(state: &mut ReplState, input: &str) -> Result<Option<Value>, String> {
    let named = NamedSource::new("repl", input.to_string());

    let tokens = lexer::lex(input, &named).map_err(|e| format!("{e:?}"))?;
    if tokens.is_empty() {
        return Ok(None);
    }

    // Make error reports point at the current input.
    state.interp.set_source(named.clone());

    // Try a top-level program first (type/task/agent/connect).
    if let Ok(program) = parser::parse(tokens.clone(), input.len(), &named) {
        for (decl, _) in &program.declarations {
            match decl {
                Decl::Run(_) => {
                    return Err("Cannot `run` agents in the REPL. Use a .keel file.".into());
                }
                _ => state.interp.register_decl(decl),
            }
            match decl {
                Decl::Type(td) => {
                    println!("  {} type {}", "✓".bright_green(), td.name.bright_cyan());
                }
                Decl::Task(td) => {
                    println!("  {} task {}", "✓".bright_green(), td.name.bright_cyan());
                }
                Decl::Agent(ad) => {
                    println!("  {} agent {}", "✓".bright_green(), ad.name.bright_cyan());
                }
                Decl::Connect(cd) => {
                    println!("  {} connection {}", "✓".bright_green(), cd.name.bright_cyan());
                }
                Decl::Run(_) => {}
            }
        }
        return Ok(None);
    }

    // Otherwise, parse as a sequence of statements (assignments, expressions,
    // calls) and run them directly against the persistent interpreter.
    let stmts = parser::parse_stmts(tokens, input.len(), &named)
        .map_err(|e| format!("{e:?}"))?;

    state
        .interp
        .eval_repl_stmts(&stmts)
        .await
        .map_err(|e| format!("{e:?}"))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn count_braces(s: &str) -> i32 {
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn format_value(val: &Value) -> String {
    match val {
        Value::String(s) => format!("\"{}\"", s).bright_green().to_string(),
        Value::Integer(n) => format!("{n}").bright_yellow().to_string(),
        Value::Float(n) => format!("{n}").bright_yellow().to_string(),
        Value::Bool(b) => format!("{b}").bright_magenta().to_string(),
        Value::None => "none".dimmed().to_string(),
        Value::EnumVariant(ty, var) => format!("{ty}.{var}").bright_cyan().to_string(),
        Value::List(items) => {
            let inner: Vec<String> = items.iter().map(format_value).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Map(fields) => {
            let inner: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{}: {}", k, format_value(v)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
        other => format!("{other}"),
    }
}

fn print_help() {
    println!("  {}", "REPL Commands:".bright_white().bold());
    println!("  {}  — show this help", ":help".bright_cyan());
    println!("  {}  — show defined types", ":types".bright_cyan());
    println!("  {}   — show environment", ":env".bright_cyan());
    println!("  {} — clear all state", ":clear".bright_cyan());
    println!("  {}  — exit", ":quit".bright_cyan());
    println!();
    println!("  {}", "You can:".bright_white().bold());
    println!(
        "  - Define types:  {}",
        "type Mood = happy | sad".dimmed()
    );
    println!(
        "  - Define tasks:  {}",
        "task greet(n: str) -> str { \"Hi {n}\" }".dimmed()
    );
    println!(
        "  - Call tasks:    {}",
        "greet(\"World\")".dimmed()
    );
    println!(
        "  - Expressions:   {}",
        "2 + 3 * 4".dimmed()
    );
    println!(
        "  - Notify:        {}",
        "notify user \"hello\"".dimmed()
    );
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}
