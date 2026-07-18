//! Interactive REPL for Keel.
//!
//! Keeps one live `Interpreter` and one top-level `Environment` across
//! every prompt so variables and task definitions persist between
//! inputs. Multi-line input is detected by balancing `{`/`}`, `[`/`]`,
//! and `(`/`)` — a line with open delimiters prompts for continuation.

use miette::{NamedSource, Result};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::sync::Arc;

use crate::ast::{Decl, Node, Stmt};
use crate::interpreter::Interpreter;
use crate::interpreter::environment::Environment;
use crate::interpreter::value::Value;
use crate::runtime::context::RuntimeContext;
use crate::{lexer, parser};

const PROMPT: &str = "keel> ";
const CONT_PROMPT: &str = "  ... ";

pub async fn start_with_runtime(runtime: Arc<RuntimeContext>) -> Result<()> {
    println!("Keel REPL — v0.1 (alpha). Ctrl-D to exit.");

    let mut rl = DefaultEditor::new().map_err(|e| miette::miette!("readline init failed: {e}"))?;
    let history_path = dirs_history_path();
    if let Some(path) = &history_path {
        let _ = rl.load_history(path);
    }

    let mut interp = Interpreter::with_runtime(runtime);
    // The REPL pre-imports the full stdlib for convenience; programs must
    // import modules explicitly with `use std/<name>`.
    interp.bind_all_namespaces();
    let mut env = Environment::new();
    let mut pending = String::new();

    loop {
        let prompt = if pending.is_empty() {
            PROMPT
        } else {
            CONT_PROMPT
        };
        let line = match rl.readline(prompt) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C clears the pending buffer; doesn't exit.
                pending.clear();
                continue;
            }
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("readline error: {e}");
                break;
            }
        };

        pending.push_str(&line);
        pending.push('\n');

        if !is_balanced(&pending) {
            continue;
        }

        let source = std::mem::take(&mut pending);
        let _ = rl.add_history_entry(source.trim_end());

        if source.trim().is_empty() {
            continue;
        }

        match eval_source(&mut interp, &mut env, &source).await {
            Ok(Some(v)) => match v {
                Value::None => {}
                other => println!("  {other}"),
            },
            Ok(None) => {}
            Err(report) => {
                eprintln!("{report:?}");
            }
        }
    }

    if let Some(path) = history_path {
        let _ = rl.save_history(&path);
    }
    println!("goodbye");
    Ok(())
}

/// Simple brace/paren/bracket balance check. String literal contents
/// are ignored (quotes + escape sequences). Triple-quoted strings
/// aren't specially handled — they count as balanced `"` pairs, which
/// works for typical inputs.
fn is_balanced(s: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut prev_backslash = false;
    for ch in s.chars() {
        if in_string {
            if prev_backslash {
                prev_backslash = false;
            } else if ch == '\\' {
                prev_backslash = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
            }
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return true;
        } // let parser error out on mismatched close
    }
    depth == 0
}

/// Parse `source` and evaluate each declaration or statement against
/// the shared REPL state. Returns the last expression's value (if any)
/// so the caller can print it.
async fn eval_source(
    interp: &mut Interpreter,
    env: &mut Environment,
    source: &str,
) -> Result<Option<Value>> {
    let named = NamedSource::new("<repl>", source.to_string());
    let tokens = lexer::lex(source, &named)?;

    // Try top-level program shape first (agent/task/type/interface/extern/use/stmt).
    if let Ok(program) = parser::parse(tokens.clone(), source.len(), &named) {
        let mut last = None;
        for node in &program.declarations {
            match &node.kind {
                Decl::Stmt(stmt_node) => {
                    last = Some(eval_stmt(interp, env, stmt_node).await?);
                }
                decl => {
                    interp.register_decl(decl)?;
                    last = None;
                }
            }
        }
        return Ok(last);
    }

    // Fall back to bare-statement parsing (for expression-only input).
    let stmts = parser::parse_stmts(tokens, source.len(), &named)?;
    let mut last = None;
    for stmt_node in &stmts {
        last = Some(eval_stmt(interp, env, stmt_node).await?);
    }
    Ok(last)
}

async fn eval_stmt(
    interp: &mut Interpreter,
    env: &mut Environment,
    stmt_node: &Node<Stmt>,
) -> Result<Value> {
    match interp.exec_stmt(stmt_node, env).await? {
        crate::interpreter::StmtOutcome::Value(v) => Ok(v),
        crate::interpreter::StmtOutcome::Return(v) => Ok(v),
        crate::interpreter::StmtOutcome::Normal => Ok(Value::None),
        crate::interpreter::StmtOutcome::Break | crate::interpreter::StmtOutcome::Continue => {
            Err(miette::miette!("`break`/`continue` outside a loop"))
        }
    }
}

fn dirs_history_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|home| {
        let mut p = std::path::PathBuf::from(home);
        p.push(".keel_history");
        p
    })
}
