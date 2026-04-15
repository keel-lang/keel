use std::collections::HashMap;

use colored::Colorize;
use miette::NamedSource;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::ast::*;
use crate::interpreter::environment::Environment;
use crate::interpreter::value::Value;
use crate::lexer;
use crate::parser;
use crate::runtime::Runtime;

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
    env: Environment,
    types: HashMap<String, TypeDef>,
    tasks: HashMap<String, TaskDecl>,
    #[allow(dead_code)]
    runtime: Runtime,
}

impl ReplState {
    fn new() -> Self {
        // Suppress LLM provider output in REPL
        std::env::set_var("KEEL_LLM", "mock");
        let runtime = Runtime::new();
        std::env::remove_var("KEEL_LLM");
        ReplState {
            env: Environment::new(),
            types: HashMap::new(),
            tasks: HashMap::new(),
            runtime,
        }
    }

    fn print_env(&self) {
        println!("  {}", "Variables:".bright_white().bold());
        println!("  {}", "(use variable names to inspect values)".dimmed());
    }

    fn print_types(&self) {
        if self.types.is_empty() {
            println!("  {}", "No types defined.".dimmed());
        } else {
            println!("  {}", "Types:".bright_white().bold());
            for (name, def) in &self.types {
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
        self.env = Environment::new();
        self.types.clear();
        self.tasks.clear();
        println!("  {}", "State cleared.".dimmed());
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

async fn eval_repl_input(state: &mut ReplState, input: &str) -> Result<Option<Value>, String> {
    let named = NamedSource::new("repl", input.to_string());

    let tokens = lexer::lex(input, &named).map_err(|e| e.to_string())?;
    if tokens.is_empty() {
        return Ok(None);
    }

    // Try parsing as a top-level declaration first
    if let Ok(program) = parser::parse(tokens.clone(), input.len(), &named) {
        for (decl, _) in &program.declarations {
            match decl {
                Decl::Type(td) => {
                    if let TypeDef::SimpleEnum(variants) = &td.def {
                        for v in variants {
                            state.env.define(
                                v.clone(),
                                Value::EnumVariant(td.name.clone(), v.clone()),
                            );
                        }
                    }
                    state.types.insert(td.name.clone(), td.def.clone());
                    println!("  {} type {}", "✓".bright_green(), td.name.bright_cyan());
                    return Ok(None);
                }
                Decl::Task(td) => {
                    state.tasks.insert(td.name.clone(), td.clone());
                    state.env.define(
                        td.name.clone(),
                        Value::Task(td.name.clone(), td.clone()),
                    );
                    println!("  {} task {}", "✓".bright_green(), td.name.bright_cyan());
                    return Ok(None);
                }
                Decl::Agent(ad) => {
                    println!(
                        "  {} agent {}",
                        "✓".bright_green(),
                        ad.name.bright_cyan()
                    );
                    return Ok(None);
                }
                Decl::Run(_) => {
                    return Err(
                        "Cannot `run` agents in the REPL. Use a .keel file.".into(),
                    );
                }
                Decl::Connect(_) => {
                    println!("  {} connection registered", "✓".bright_green());
                    return Ok(None);
                }
            }
        }
        return Ok(None);
    }

    // Not a declaration — try wrapping as a statement inside a dummy task
    // and executing it. This handles: expressions, assignments, task calls.
    eval_as_expression(state, input).await
}

/// Try to evaluate input as an expression or statement by wrapping it in
/// a temporary task and running it through the interpreter.
async fn eval_as_expression(
    state: &mut ReplState,
    input: &str,
) -> Result<Option<Value>, String> {
    // Wrap the input so its return value gets displayed.
    // show user always works — for none it prints "none", for values it formats them.
    let wrapper = format!(
        "task __repl__() {{ {} }}\nagent __A {{ role \"repl\" \n every 1.day {{ show user __repl__() }} }}\nrun __A",
        input
    );

    let named = NamedSource::new("repl", wrapper.clone());
    let tokens = lexer::lex(&wrapper, &named).map_err(|e| {
        // If the wrapper fails to lex, give a cleaner error
        format!("Syntax error: {}", e)
    })?;

    let program = parser::parse(tokens, wrapper.len(), &named).map_err(|e| {
        format!("Syntax error: {}", e)
    })?;

    // Register any tasks/types the state already knows about into a fresh program
    let mut full_decls: Vec<Spanned<Decl>> = Vec::new();

    // Add existing type definitions
    for (name, def) in &state.types {
        full_decls.push((
            Decl::Type(TypeDecl {
                name: name.clone(),
                def: def.clone(),
            }),
            0..0,
        ));
    }

    // Add existing task definitions
    for (name, td) in &state.tasks {
        if name != "__repl__" {
            full_decls.push((Decl::Task(td.clone()), 0..0));
        }
    }

    // Add the wrapper declarations
    full_decls.extend(program.declarations);

    let full_program = Program {
        declarations: full_decls,
    };

    // Run with mock LLM and oneshot mode
    let prev_mock = std::env::var("KEEL_LLM").ok();
    let prev_repl = std::env::var("KEEL_REPL").ok();
    std::env::set_var("KEEL_LLM", "mock");
    std::env::set_var("KEEL_REPL", "1");

    let result = crate::interpreter::run(full_program).await;

    // Restore env
    match prev_mock {
        Some(val) => std::env::set_var("KEEL_LLM", val),
        None => std::env::remove_var("KEEL_LLM"),
    }
    match prev_repl {
        Some(val) => std::env::set_var("KEEL_REPL", val),
        None => std::env::remove_var("KEEL_REPL"),
    }

    match result {
        Ok(()) => Ok(None),
        Err(e) => {
            let msg = e.to_string();
            // Filter out expected "agent not found" noise
            if msg.contains("Runtime error") {
                Err(msg)
            } else {
                Ok(None)
            }
        }
    }
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
