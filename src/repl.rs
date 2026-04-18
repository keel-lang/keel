//! Interactive REPL for Keel.
//!
//! Keeps one live `Interpreter` and one top-level `Environment` across
//! every prompt so variables and task definitions persist between
//! inputs. Multi-line input is detected by balancing `{`/`}`, `[`/`]`,
//! and `(`/`)` — a line with open delimiters prompts for continuation.

use miette::{NamedSource, Result};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::ast::{Decl, Stmt};
use crate::interpreter::environment::Environment;
use crate::interpreter::value::Value;
use crate::interpreter::Interpreter;
use crate::{lexer, parser};

const PROMPT: &str = "keel> ";
const CONT_PROMPT: &str = "  ... ";

pub async fn start() -> Result<()> {
    println!("Keel REPL — v0.1 (alpha). Ctrl-D to exit.");

    let mut rl = DefaultEditor::new()
        .map_err(|e| miette::miette!("readline init failed: {e}"))?;
    let history_path = dirs_history_path();
    if let Some(path) = &history_path {
        let _ = rl.load_history(path);
    }

    let mut interp = Interpreter::new();
    let mut env = Environment::new();
    let mut pending = String::new();

    loop {
        let prompt = if pending.is_empty() { PROMPT } else { CONT_PROMPT };
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
            '"' => { in_string = true; }
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            _ => {}
        }
        if depth < 0 { return true; } // let parser error out on mismatched close
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
        for (decl, _span) in &program.declarations {
            match decl {
                Decl::Stmt((stmt, _)) => {
                    last = Some(eval_stmt(interp, env, stmt).await?);
                }
                _ => {
                    interp_register(interp, decl)?;
                    last = None;
                }
            }
        }
        return Ok(last);
    }

    // Fall back to bare-statement parsing (for expression-only input).
    let stmts = parser::parse_stmts(tokens, source.len(), &named)?;
    let mut last = None;
    for (stmt, _span) in &stmts {
        last = Some(eval_stmt(interp, env, stmt).await?);
    }
    Ok(last)
}

async fn eval_stmt(
    interp: &mut Interpreter,
    env: &mut Environment,
    stmt: &Stmt,
) -> Result<Value> {
    match interp.exec_stmt(stmt, env).await? {
        crate::interpreter::StmtOutcome::Value(v) => Ok(v),
        crate::interpreter::StmtOutcome::Return(v) => Ok(v),
        crate::interpreter::StmtOutcome::Normal => Ok(Value::None),
    }
}

/// Register a declaration (type / task / agent / interface / extern /
/// use) in the REPL's persistent interpreter.
fn interp_register(interp: &mut Interpreter, decl: &Decl) -> Result<()> {
    use crate::ast::{AgentItem, AttributeDecl, AttributeBody};
    match decl {
        Decl::Type(t) => {
            interp.globals.insert(t.name.clone(), Value::Namespace(t.name.clone()));
            if let crate::ast::TypeDef::SimpleEnum(variants) = &t.def {
                interp.enum_types.insert(t.name.clone(), variants.clone());
            }
        }
        Decl::Task(t) => {
            interp.globals.insert(t.name.clone(), Value::Task(t.name.clone(), t.clone()));
        }
        Decl::Agent(a) => {
            let def = crate::interpreter::AgentDef {
                name: a.name.clone(),
                attributes: a.items.iter().filter_map(|it| match it {
                    AgentItem::Attribute(attr @ AttributeDecl { .. }) => Some(attr.clone()),
                    _ => None,
                }).collect(),
                state_fields: a.items.iter().filter_map(|it| match it {
                    AgentItem::State(fields) => Some(fields.clone()),
                    _ => None,
                }).flatten().collect(),
                tasks: a.items.iter().filter_map(|it| match it {
                    AgentItem::Task(t) => Some(t.clone()),
                    _ => None,
                }).collect(),
                handlers: a.items.iter().filter_map(|it| match it {
                    AgentItem::On(h) => Some(h.clone()),
                    _ => None,
                }).collect(),
            };
            interp.globals.insert(a.name.clone(), Value::AgentRef(a.name.clone()));
            interp.agents.insert(a.name.clone(), def);
            // Silence unused-import warning.
            let _: Option<AttributeBody> = None;
        }
        _ => {} // interface / extern / use — registered at program scope, not runtime
    }
    Ok(())
}

fn dirs_history_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|home| {
        let mut p = std::path::PathBuf::from(home);
        p.push(".keel_history");
        p
    })
}
