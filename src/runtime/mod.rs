//! Runtime: prelude namespace installation for v0.1.
//!
//! Every Keel program starts with these namespaces in scope:
//!   `Ai`, `Io`, `Schedule`, `Email`, `Http`, `Memory`, `Async`,
//!   `Control`, `Env`, `Log`, `Agent`.
//!
//! Top-level convenience bindings (`run`, `stop`) wrap `Agent.*`.

pub mod email;
pub mod human;
pub mod llm;

use std::collections::HashMap;
use std::sync::Arc;

use crate::interpreter::{
    CallArgValue, Interpreter, Namespace,
};
use crate::interpreter::value::Value;

/// Common "symbol" identifiers used as hints in stdlib argument lists
/// (`unit: sentences`, `format: bullets`, `backoff: exponential`, …).
/// v0.1 binds them as plain strings so user programs can write them as
/// bare identifiers without special parser treatment.
const SYMBOL_IDENTS: &[&str] = &[
    "sentence", "sentences", "line", "lines", "word", "words",
    "paragraph", "paragraphs",
    "bullets", "prose", "json",
    "exponential", "linear", "fixed",
    "google", "bing", "arxiv",
    "text", "html", "markdown",
];

pub fn install_prelude(interp: &mut Interpreter) {
    for s in SYMBOL_IDENTS {
        interp.globals.insert((*s).to_string(), Value::String((*s).to_string()));
    }
    interp.register_namespace(io_namespace());
    interp.register_namespace(schedule_namespace());
    interp.register_namespace(ai_namespace());
    interp.register_namespace(email_namespace());
    interp.register_namespace(env_namespace());
    interp.register_namespace(memory_namespace());
    interp.register_namespace(log_namespace());
    interp.register_namespace(agent_namespace());
    interp.register_namespace(control_namespace());
    interp.register_namespace(async_namespace());
    interp.register_namespace(http_namespace());

    // Top-level: run / stop are convenience re-exports of Agent.run / Agent.stop.
    interp.register_top_fn(
        "run",
        Arc::new(|interp: &mut Interpreter, args: Vec<CallArgValue>| {
            Box::pin(async move {
                let agent_name = match args.first().map(|a| &a.value) {
                    Some(Value::AgentRef(name)) => name.clone(),
                    Some(other) => {
                        return Err(miette::miette!(
                            "run() expects an agent, got {}",
                            other.type_name()
                        ));
                    }
                    None => return Err(miette::miette!("run() requires an agent argument")),
                };
                interp.start_agent(&agent_name).await?;
                Ok(Value::None)
            })
        }),
    );

    interp.register_top_fn(
        "stop",
        Arc::new(|interp: &mut Interpreter, args: Vec<CallArgValue>| {
            Box::pin(async move {
                let agent_name = match args.first().map(|a| &a.value) {
                    Some(Value::AgentRef(name)) => name.clone(),
                    _ => return Err(miette::miette!("stop() requires an agent argument")),
                };
                interp.stop_agent(&agent_name).await?;
                Ok(Value::None)
            })
        }),
    );
}

// ---------------------------------------------------------------------------
// Helpers to build namespaces concisely
// ---------------------------------------------------------------------------

macro_rules! ns {
    ($name:expr, { $($method:expr => $impl:expr),* $(,)? }) => {{
        let mut m: HashMap<String, crate::interpreter::BuiltinFn> = HashMap::new();
        $(
            m.insert($method.to_string(), Arc::new($impl));
        )*
        Namespace { name: $name.to_string(), methods: m }
    }};
}

fn find_arg<'a>(args: &'a [CallArgValue], name: &str) -> Option<&'a Value> {
    args.iter().find(|a| a.name.as_deref() == Some(name)).map(|a| &a.value)
}

fn positional(args: &[CallArgValue], idx: usize) -> Option<&Value> {
    args.iter().filter(|a| a.name.is_none()).nth(idx).map(|a| &a.value)
}

// ---------------------------------------------------------------------------
// Io
// ---------------------------------------------------------------------------

fn io_namespace() -> Namespace {
    ns!("Io", {
        "notify" => |_i, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            human::notify(&msg);
            Ok(Value::None)
        }),
        "show" => |_i, args| Box::pin(async move {
            let v = positional(&args, 0).cloned().unwrap_or(Value::None);
            human::show(&v);
            Ok(Value::None)
        }),
        "ask" => |_i, args| Box::pin(async move {
            let prompt = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            Ok(Value::String(human::ask(&prompt)))
        }),
        "confirm" => |_i, args| Box::pin(async move {
            let prompt = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            Ok(Value::Bool(human::confirm(&prompt)))
        }),
    })
}

// ---------------------------------------------------------------------------
// Schedule
// ---------------------------------------------------------------------------

fn schedule_namespace() -> Namespace {
    ns!("Schedule", {
        // v0.1 MVP: `Schedule.every(duration, fn)` runs `fn` once
        // immediately and returns. Recurring execution lands when the
        // agent event loop is in place.
        "every" => |interp, args| Box::pin(async move {
            if let Some(closure) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), b.clone())),
                _ => None,
            }) {
                let (params, body) = closure;
                interp.call_closure(&params, &body, vec![]).await?;
            }
            Ok(Value::None)
        }),
        "after" => |interp, args| Box::pin(async move {
            if let Some(closure) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), b.clone())),
                _ => None,
            }) {
                let (params, body) = closure;
                interp.call_closure(&params, &body, vec![]).await?;
            }
            Ok(Value::None)
        }),
        "at" => |_i, _args| Box::pin(async move {
            Ok(Value::None)
        }),
        "sleep" => |_i, args| Box::pin(async move {
            if let Some(Value::Duration(secs)) = positional(&args, 0) {
                tokio::time::sleep(std::time::Duration::from_secs_f64(*secs)).await;
            }
            Ok(Value::None)
        }),
    })
}

// ---------------------------------------------------------------------------
// Ai (minimal — delegates to runtime::llm for real use)
// ---------------------------------------------------------------------------

fn ai_namespace() -> Namespace {
    ns!("Ai", {
        "classify" => |_i, args| Box::pin(async move {
            // v0.1 MVP: return the `fallback:` value if present, else None.
            // Full LLM-backed classification lands when the LlmProvider
            // interface is wired up.
            Ok(find_arg(&args, "fallback").cloned().unwrap_or(Value::None))
        }),
        "summarize" => |_i, args| Box::pin(async move {
            Ok(find_arg(&args, "fallback").cloned().unwrap_or(Value::None))
        }),
        "draft" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "extract" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "translate" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "decide" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "prompt" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "embed" => |_i, _args| Box::pin(async move { Ok(Value::List(vec![])) }),
    })
}

// ---------------------------------------------------------------------------
// Email (stub)
// ---------------------------------------------------------------------------

fn email_namespace() -> Namespace {
    ns!("Email", {
        "fetch" => |_i, _args| Box::pin(async move { Ok(Value::List(vec![])) }),
        "send" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "archive" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
    })
}

// ---------------------------------------------------------------------------
// Env
// ---------------------------------------------------------------------------

fn env_namespace() -> Namespace {
    ns!("Env", {
        "get" => |_i, args| Box::pin(async move {
            let name = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            match std::env::var(&name) {
                Ok(v) => Ok(Value::String(v)),
                Err(_) => Ok(Value::None),
            }
        }),
        "require" => |_i, args| Box::pin(async move {
            let name = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            match std::env::var(&name) {
                Ok(v) => Ok(Value::String(v)),
                Err(_) => Err(miette::miette!("Env.require: `{name}` is not set")),
            }
        }),
    })
}

// ---------------------------------------------------------------------------
// Memory (stub)
// ---------------------------------------------------------------------------

fn memory_namespace() -> Namespace {
    ns!("Memory", {
        "remember" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "recall" => |_i, _args| Box::pin(async move { Ok(Value::List(vec![])) }),
        "forget" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
    })
}

// ---------------------------------------------------------------------------
// Log
// ---------------------------------------------------------------------------

fn log_namespace() -> Namespace {
    ns!("Log", {
        "info" => |_i, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            eprintln!("[info] {msg}");
            Ok(Value::None)
        }),
        "warn" => |_i, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            eprintln!("[warn] {msg}");
            Ok(Value::None)
        }),
        "error" => |_i, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            eprintln!("[error] {msg}");
            Ok(Value::None)
        }),
        "debug" => |_i, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            eprintln!("[debug] {msg}");
            Ok(Value::None)
        }),
    })
}

// ---------------------------------------------------------------------------
// Agent (lifecycle)
// ---------------------------------------------------------------------------

fn agent_namespace() -> Namespace {
    ns!("Agent", {
        "run" => |interp, args| Box::pin(async move {
            let agent_name = match args.first().map(|a| &a.value) {
                Some(Value::AgentRef(name)) => name.clone(),
                _ => return Err(miette::miette!("Agent.run expects an agent argument")),
            };
            interp.start_agent(&agent_name).await?;
            Ok(Value::None)
        }),
        "stop" => |interp, args| Box::pin(async move {
            let agent_name = match args.first().map(|a| &a.value) {
                Some(Value::AgentRef(name)) => name.clone(),
                _ => return Err(miette::miette!("Agent.stop expects an agent argument")),
            };
            interp.stop_agent(&agent_name).await?;
            Ok(Value::None)
        }),
    })
}

// ---------------------------------------------------------------------------
// Control (retry / with_timeout — MVP stubs)
// ---------------------------------------------------------------------------

fn control_namespace() -> Namespace {
    ns!("Control", {
        "retry" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "with_timeout" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "with_deadline" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
    })
}

// ---------------------------------------------------------------------------
// Async (spawn / join — MVP stubs)
// ---------------------------------------------------------------------------

fn async_namespace() -> Namespace {
    ns!("Async", {
        "spawn" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "join_all" => |_i, _args| Box::pin(async move { Ok(Value::List(vec![])) }),
        "select" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "sleep" => |_i, args| Box::pin(async move {
            if let Some(Value::Duration(secs)) = positional(&args, 0) {
                tokio::time::sleep(std::time::Duration::from_secs_f64(*secs)).await;
            }
            Ok(Value::None)
        }),
    })
}

// ---------------------------------------------------------------------------
// Http (stub)
// ---------------------------------------------------------------------------

fn http_namespace() -> Namespace {
    ns!("Http", {
        "get" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "post" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
        "request" => |_i, _args| Box::pin(async move { Ok(Value::None) }),
    })
}
