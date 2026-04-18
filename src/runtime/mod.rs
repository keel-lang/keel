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
        "classify" => |interp, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("Ai.classify: missing input"))?
                .as_string();
            let variants = classify_variants(interp, &args)?;
            let criteria = extract_criteria(&args);
            let model = resolve_model(interp, &args);
            let enum_type = find_arg(&args, "as").and_then(|v| match v {
                Value::Namespace(n) => Some(n.clone()),
                _ => None,
            }).unwrap_or_default();

            let llm = interp.llm.clone();
            match llm.classify(&input, &variants, &criteria, &model).await {
                Ok(Some(variant)) => Ok(Value::EnumVariant(enum_type, variant)),
                Ok(None) => Ok(find_arg(&args, "fallback").cloned().unwrap_or(Value::None)),
                Err(msg) => Err(miette::miette!("{msg}")),
            }
        }),

        "summarize" => |interp, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("Ai.summarize: missing input"))?
                .as_string();
            let length = match (find_arg(&args, "in"), find_arg(&args, "unit")) {
                (Some(Value::Integer(n)), Some(unit)) => Some((*n, unit.as_string())),
                _ => None,
            };
            let model = resolve_model(interp, &args);
            let llm = interp.llm.clone();
            match llm.summarize(&input, length, &model).await {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(find_arg(&args, "fallback").cloned().unwrap_or(Value::None)),
                Err(msg) => Err(miette::miette!("{msg}")),
            }
        }),

        "draft" => |interp, args| Box::pin(async move {
            let description = positional(&args, 0)
                .ok_or_else(|| miette::miette!("Ai.draft: missing description"))?
                .as_string();
            let tone = find_arg(&args, "tone").map(|v| v.as_string());
            let guidance = find_arg(&args, "guidance").map(|v| v.as_string());
            let max_length = find_arg(&args, "max_length").and_then(|v| v.as_int());
            let model = resolve_model(interp, &args);
            let llm = interp.llm.clone();
            match llm
                .draft(&description, tone.as_deref(), guidance.as_deref(), max_length, &model)
                .await
            {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(Value::None),
                Err(msg) => Err(miette::miette!("{msg}")),
            }
        }),

        "extract" => |interp, args| Box::pin(async move {
            let input = match find_arg(&args, "from") {
                Some(v) => v.as_string(),
                None => positional(&args, 0)
                    .ok_or_else(|| miette::miette!("Ai.extract: missing input"))?
                    .as_string(),
            };
            // Schema is either `schema: { field: "type" }` (map) or unspecified.
            let schema: Vec<(String, String)> = match find_arg(&args, "schema") {
                Some(Value::Map(m)) => m.iter().map(|(k, v)| (k.clone(), v.as_string())).collect(),
                _ => Vec::new(),
            };
            let model = resolve_model(interp, &args);
            let llm = interp.llm.clone();
            match llm.extract(&input, &schema, &model).await {
                Ok(Some(json)) => {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json) {
                        Ok(json_to_value(&parsed))
                    } else {
                        Ok(Value::String(json))
                    }
                }
                Ok(None) => Ok(Value::None),
                Err(msg) => Err(miette::miette!("{msg}")),
            }
        }),

        "translate" => |interp, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("Ai.translate: missing input"))?
                .as_string();
            let target_langs: Vec<String> = match find_arg(&args, "to") {
                Some(Value::List(items)) => items.iter().map(|v| v.as_string()).collect(),
                Some(other) => vec![other.as_string()],
                None => return Err(miette::miette!("Ai.translate: missing `to:` argument")),
            };
            let model = resolve_model(interp, &args);
            let llm = interp.llm.clone();
            match llm.translate(&input, &target_langs, &model).await {
                Ok(Some(map)) if target_langs.len() == 1 => {
                    Ok(Value::String(map.into_values().next().unwrap_or_default()))
                }
                Ok(Some(map)) => {
                    let mut out = HashMap::new();
                    for (k, v) in map { out.insert(k, Value::String(v)); }
                    Ok(Value::Map(out))
                }
                Ok(None) => Ok(Value::None),
                Err(msg) => Err(miette::miette!("{msg}")),
            }
        }),

        "decide" => |interp, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("Ai.decide: missing input"))?
                .as_string();
            let options: Vec<String> = match find_arg(&args, "options") {
                Some(Value::List(items)) => items.iter().map(|v| v.as_string()).collect(),
                _ => Vec::new(),
            };
            let model = resolve_model(interp, &args);
            let llm = interp.llm.clone();
            match llm.decide(&input, &options, &model).await {
                Ok(Some((choice, reason))) => {
                    let mut m = HashMap::new();
                    m.insert("choice".to_string(), Value::String(choice));
                    m.insert("reason".to_string(), Value::String(reason));
                    m.insert("confidence".to_string(), Value::Float(1.0));
                    Ok(Value::Map(m))
                }
                Ok(None) => Ok(Value::None),
                Err(msg) => Err(miette::miette!("{msg}")),
            }
        }),

        "prompt" => |interp, args| Box::pin(async move {
            let system = find_arg(&args, "system").map(|v| v.as_string()).unwrap_or_default();
            let user = find_arg(&args, "user").map(|v| v.as_string()).unwrap_or_default();
            let model = resolve_model(interp, &args);
            let llm = interp.llm.clone();
            match llm.prompt(&system, &user, &model).await {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(Value::None),
                Err(msg) => Err(miette::miette!("{msg}")),
            }
        }),

        "embed" => |_i, _args| Box::pin(async move {
            // v0.1: embeddings not wired yet.
            Ok(Value::List(vec![]))
        }),
    })
}

/// Resolve the model string for an Ai.* call:
///   1. explicit `using: "model"` argument
///   2. enclosing agent's `@model` attribute
///   3. `"default"` (triggers KEEL_OLLAMA_MODEL catch-all)
fn resolve_model(interp: &Interpreter, args: &[CallArgValue]) -> String {
    if let Some(v) = find_arg(args, "using") {
        return v.as_string();
    }
    interp.current_model()
}

/// Extract enum variants from `as: T` (Value::Namespace(T)) by looking
/// T up in the interpreter's enum registry.
fn classify_variants(interp: &Interpreter, args: &[CallArgValue]) -> miette::Result<Vec<String>> {
    match find_arg(args, "as") {
        Some(Value::Namespace(name)) => {
            interp.enum_types.get(name).cloned().ok_or_else(|| {
                miette::miette!("Ai.classify: `as: {name}` is not a simple enum type")
            })
        }
        Some(Value::List(items)) => {
            // Inline form: `as: [low, medium, high]`
            Ok(items.iter().map(|v| v.as_string()).collect())
        }
        _ => Err(miette::miette!("Ai.classify: missing `as:` argument")),
    }
}

/// Extract classification criteria from `considering: { "hint": Variant }`.
fn extract_criteria(args: &[CallArgValue]) -> Vec<(String, String)> {
    match find_arg(args, "considering") {
        Some(Value::Map(m)) => m
            .iter()
            .map(|(k, v)| {
                let variant_name = match v {
                    Value::EnumVariant(_, variant) => variant.clone(),
                    other => other.as_string(),
                };
                (k.clone(), variant_name)
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Convert a serde_json::Value to a Keel Value (for Ai.extract results).
fn json_to_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::None,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::None
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => Value::List(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => {
            let mut m = HashMap::new();
            for (k, v) in obj {
                m.insert(k.clone(), json_to_value(v));
            }
            Value::Map(m)
        }
    }
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
