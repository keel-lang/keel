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
        // `Schedule.every(duration, () => { ... })` fires the closure
        // once immediately, then again every `duration` for the life
        // of the enclosing agent. Must be called from an @on_start or
        // an agent task — outside an agent there's no context to bind
        // the closure to.
        "every" => |interp, args| Box::pin(async move {
            schedule_fire(interp, args, /* recurring */ true).await
        }),
        // `Schedule.after(duration, () => { ... })` fires the closure
        // once after `duration`.
        "after" => |interp, args| Box::pin(async move {
            schedule_fire(interp, args, /* recurring */ false).await
        }),
        // `Schedule.at(datetime_str, () => { ... })` fires the closure
        // once at the given absolute time. Accepts:
        //   - RFC 3339 / ISO 8601: `"2026-04-20T10:00:00Z"` or
        //     `"2026-04-20T10:00:00+02:00"`
        //   - Naive local datetime: `"2026-04-20T10:00:00"` (treated as UTC)
        // If the target is already in the past, fires immediately.
        "at" => |interp, args| Box::pin(async move {
            schedule_at(interp, args).await
        }),
        "sleep" => |_i, args| Box::pin(async move {
            if let Some(Value::Duration(secs)) = positional(&args, 0) {
                tokio::time::sleep(std::time::Duration::from_secs_f64(*secs)).await;
            }
            Ok(Value::None)
        }),
    })
}

async fn schedule_at(
    interp: &mut Interpreter,
    args: Vec<CallArgValue>,
) -> miette::Result<Value> {
    let when_str = positional(&args, 0)
        .map(|v| v.as_string())
        .ok_or_else(|| miette::miette!("Schedule.at: missing datetime argument"))?;

    let target = parse_datetime(&when_str)
        .ok_or_else(|| miette::miette!("Schedule.at: cannot parse `{when_str}` as an ISO 8601 datetime"))?;
    let now = chrono::Utc::now();
    let delay_secs = (target - now).num_seconds().max(0) as f64;

    let (params, body) = args.iter().find_map(|a| match &a.value {
        Value::Closure(p, b) => Some((p.clone(), b.clone())),
        _ => None,
    }).ok_or_else(|| miette::miette!("Schedule.at: missing closure argument"))?;

    let agent_name = interp
        .current_agent
        .as_ref()
        .ok_or_else(|| miette::miette!("Schedule.at must be called from within an agent"))?
        .lock()
        .unwrap()
        .def
        .name
        .clone();

    let closure_id = interp.register_closure(agent_name.clone(), params, body);
    let tx = interp.event_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs_f64(delay_secs)).await;
        let _ = tx.send(crate::interpreter::Event::FireClosure { agent_name, closure_id });
    });
    Ok(Value::None)
}

/// Parse an ISO 8601 / RFC 3339 datetime string into UTC. Falls back
/// to naive datetime (treated as UTC) when no timezone is given.
fn parse_datetime(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d"] {
        if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc));
        }
        if let Ok(nd) = chrono::NaiveDate::parse_from_str(s, fmt) {
            let ndt = nd.and_hms_opt(0, 0, 0)?;
            return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc));
        }
    }
    None
}

async fn schedule_fire(
    interp: &mut Interpreter,
    args: Vec<CallArgValue>,
    recurring: bool,
) -> miette::Result<Value> {
    let duration = args.iter().find_map(|a| match &a.value {
        Value::Duration(s) => Some(*s),
        _ => None,
    }).ok_or_else(|| miette::miette!("Schedule: missing duration argument"))?;

    let (params, body) = args.iter().find_map(|a| match &a.value {
        Value::Closure(p, b) => Some((p.clone(), b.clone())),
        _ => None,
    }).ok_or_else(|| miette::miette!("Schedule: missing closure argument"))?;

    let agent_name = interp
        .current_agent
        .as_ref()
        .ok_or_else(|| miette::miette!("Schedule must be called from within an agent"))?
        .lock()
        .unwrap()
        .def
        .name
        .clone();

    let closure_id = interp.register_closure(agent_name.clone(), params, body);
    let tx = interp.event_tx.clone();
    let dur = std::time::Duration::from_secs_f64(duration);

    if recurring {
        // Fire immediately, then on each tick.
        let _ = tx.send(crate::interpreter::Event::FireClosure {
            agent_name: agent_name.clone(),
            closure_id,
        });
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(dur);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await; // consume the immediate tick (already fired above)
            loop {
                interval.tick().await;
                if tx.send(crate::interpreter::Event::FireClosure {
                    agent_name: agent_name.clone(),
                    closure_id,
                }).is_err() {
                    break; // receiver dropped — event loop has exited
                }
            }
        });
    } else {
        tokio::spawn(async move {
            tokio::time::sleep(dur).await;
            let _ = tx.send(crate::interpreter::Event::FireClosure {
                agent_name,
                closure_id,
            });
        });
    }

    Ok(Value::None)
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
            let role = interp.current_role();
            let enum_type = find_arg(&args, "as").and_then(|v| match v {
                Value::Namespace(n) => Some(n.clone()),
                _ => None,
            }).unwrap_or_default();

            let llm = interp.llm.clone();
            match llm.classify(role.as_deref(), &input, &variants, &criteria, &model).await {
                Ok(Some(variant)) => Ok(Value::EnumVariant(enum_type, variant, None)),
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
            let role = interp.current_role();
            let llm = interp.llm.clone();
            match llm.summarize(role.as_deref(), &input, length, &model).await {
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
            let role = interp.current_role();
            let llm = interp.llm.clone();
            match llm
                .draft(role.as_deref(), &description, tone.as_deref(), guidance.as_deref(), max_length, &model)
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
            let role = interp.current_role();
            let llm = interp.llm.clone();
            match llm.extract(role.as_deref(), &input, &schema, &model).await {
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
            let role = interp.current_role();
            let llm = interp.llm.clone();
            match llm.translate(role.as_deref(), &input, &target_langs, &model).await {
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
            let role = interp.current_role();
            let llm = interp.llm.clone();
            match llm.decide(role.as_deref(), &input, &options, &model).await {
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
            let role = interp.current_role();
            let llm = interp.llm.clone();
            match llm.prompt(role.as_deref(), &system, &user, &model).await {
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
                    Value::EnumVariant(_, variant, _) => variant.clone(),
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
// Email — real IMAP fetch + SMTP send via env vars
// ---------------------------------------------------------------------------
//
// Configuration: IMAP_HOST, SMTP_HOST (optional — defaults to IMAP_HOST
// with `imap.` → `smtp.`), EMAIL_USER, EMAIL_PASS.
// If env vars aren't set, Email.fetch returns [] and Email.send is a
// no-op with a one-line stderr warning — programs keep running.

fn email_namespace() -> Namespace {
    ns!("Email", {
        "fetch" => |_i, args| Box::pin(async move {
            let Some(conn) = email_conn_from_env() else {
                eprintln!("  ⚠ Email.fetch: IMAP_HOST/EMAIL_USER/EMAIL_PASS not set — returning empty list");
                return Ok(Value::List(vec![]));
            };
            // `unread: true` is the v0.1 default (and only) filter.
            let _unread_only = matches!(find_arg(&args, "unread"), Some(Value::Bool(false))) == false;
            match tokio::task::spawn_blocking(move || email::fetch_emails(&conn)).await {
                Ok(Ok(emails)) => Ok(Value::List(emails)),
                Ok(Err(msg)) => Err(miette::miette!("{msg}")),
                Err(e) => Err(miette::miette!("email fetch task join error: {e}")),
            }
        }),
        "send" => |_i, args| Box::pin(async move {
            let Some(conn) = email_conn_from_env() else {
                eprintln!("  ⚠ Email.send: IMAP_HOST/EMAIL_USER/EMAIL_PASS not set — skipping");
                return Ok(Value::None);
            };
            // Positional 0 is the message body (str or Map with .body).
            let (body, inferred_subject) = match positional(&args, 0) {
                Some(Value::Map(m)) => (
                    m.get("body").map(|v| v.as_string()).unwrap_or_default(),
                    m.get("subject").map(|v| v.as_string()),
                ),
                Some(v) => (v.as_string(), None),
                None => return Err(miette::miette!("Email.send: missing message body")),
            };
            let to = match find_arg(&args, "to") {
                Some(Value::Map(m)) => m.get("from").map(|v| v.as_string()).unwrap_or_default(),
                Some(v) => v.as_string(),
                None => return Err(miette::miette!("Email.send: missing `to:` argument")),
            };
            let subject = find_arg(&args, "subject")
                .map(|v| v.as_string())
                .or(inferred_subject)
                .unwrap_or_else(|| "(no subject)".to_string());
            match tokio::task::spawn_blocking(move || email::send_email(&conn, &to, &subject, &body)).await {
                Ok(Ok(())) => Ok(Value::None),
                Ok(Err(msg)) => Err(miette::miette!("{msg}")),
                Err(e) => Err(miette::miette!("email send task join error: {e}")),
            }
        }),
        // v0.1: IMAP move-to-archive not implemented. No-op; users who
        // need true archiving can fall back to `Email.send(... to: archive_addr)`
        // or wait for this to land in a follow-up.
        "archive" => |_i, _args| Box::pin(async move {
            Ok(Value::None)
        }),
    })
}

/// Build an `EmailConnection` from environment variables. Returns
/// `None` if required variables are missing (fetch/send then degrade
/// gracefully).
fn email_conn_from_env() -> Option<email::EmailConnection> {
    let imap_host = std::env::var("IMAP_HOST").ok().filter(|s| !s.is_empty())?;
    let user = std::env::var("EMAIL_USER").ok().filter(|s| !s.is_empty())?;
    let pass = std::env::var("EMAIL_PASS").ok().filter(|s| !s.is_empty())?;
    let smtp_host = std::env::var("SMTP_HOST")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| imap_host.replace("imap.", "smtp."));
    Some(email::EmailConnection { imap_host, smtp_host, user, pass })
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
        // Agent.send(target, message) — posts `message` to the target
        // agent's `on message` handler via the event loop. Returns
        // immediately; the handler runs later in the target's context.
        "send" => |interp, args| Box::pin(async move {
            let target = match args.first().map(|a| &a.value) {
                Some(Value::AgentRef(name)) => name.clone(),
                _ => return Err(miette::miette!("Agent.send: first arg must be an agent")),
            };
            let data = args.iter().skip(1)
                .find(|a| a.name.is_none())
                .map(|a| a.value.clone())
                .unwrap_or(Value::None);
            let event_name = find_arg(&args, "event").map(|v| v.as_string()).unwrap_or_else(|| "message".to_string());
            let _ = interp.event_tx.send(crate::interpreter::Event::Dispatch {
                agent_name: target,
                event: event_name,
                data,
            });
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
// Http — reqwest-backed GET / POST / request
// ---------------------------------------------------------------------------

fn http_namespace() -> Namespace {
    ns!("Http", {
        "get" => |_i, args| Box::pin(async move {
            let url = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Http.get: missing URL"))?;
            let headers = map_from_arg(find_arg(&args, "headers"));
            let response = http_send("GET", &url, headers, None).await?;
            Ok(response)
        }),
        "post" => |_i, args| Box::pin(async move {
            let url = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Http.post: missing URL"))?;
            let headers = map_from_arg(find_arg(&args, "headers"));
            let body = find_arg(&args, "json")
                .or_else(|| find_arg(&args, "body"))
                .cloned();
            let response = http_send("POST", &url, headers, body).await?;
            Ok(response)
        }),
        "request" => |_i, args| Box::pin(async move {
            // Accepts a single map argument with keys `method`, `url`,
            // `headers`, `body`, `json`.
            let cfg = match positional(&args, 0) {
                Some(Value::Map(m)) => m.clone(),
                _ => {
                    // Also accept direct named args.
                    let mut m = HashMap::new();
                    for a in &args {
                        if let Some(n) = &a.name {
                            m.insert(n.clone(), a.value.clone());
                        }
                    }
                    m
                }
            };
            let method = cfg.get("method").map(|v| v.as_string()).unwrap_or_else(|| "GET".into());
            let url = cfg.get("url").map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Http.request: missing `url`"))?;
            let headers = cfg.get("headers").cloned().and_then(|v| match v {
                Value::Map(m) => Some(m),
                _ => None,
            }).unwrap_or_default();
            let body = cfg.get("json").or_else(|| cfg.get("body")).cloned();
            http_send(&method, &url, headers, body).await
        }),
    })
}

fn map_from_arg(arg: Option<&Value>) -> HashMap<String, Value> {
    match arg {
        Some(Value::Map(m)) => m.clone(),
        _ => HashMap::new(),
    }
}

async fn http_send(
    method: &str,
    url: &str,
    headers: HashMap<String, Value>,
    body: Option<Value>,
) -> miette::Result<Value> {
    let client = reqwest::Client::new();
    let method_upper = method.to_uppercase();
    let reqwest_method = match method_upper.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        other => return Err(miette::miette!("Http: unsupported method `{other}`")),
    };

    let mut req = client.request(reqwest_method, url);
    for (k, v) in &headers {
        req = req.header(k, v.as_string());
    }
    if let Some(b) = body {
        match b {
            Value::Map(_) | Value::List(_) => {
                // Serialise via serde_json round-trip.
                if let Ok(json) = serde_json::to_value(value_to_json(&b)) {
                    req = req.json(&json);
                }
            }
            Value::String(s) => { req = req.body(s); }
            _ => { req = req.body(b.as_string()); }
        }
    }

    let response = req.send().await.map_err(|e| miette::miette!("Http {method_upper} {url}: {e}"))?;
    let status = response.status().as_u16() as i64;
    let response_headers = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), Value::String(v.to_str().unwrap_or("").to_string())))
        .collect::<HashMap<_, _>>();
    let body_text = response.text().await.unwrap_or_default();

    let mut result = HashMap::new();
    result.insert("status".to_string(), Value::Integer(status));
    result.insert("body".to_string(), Value::String(body_text));
    result.insert("headers".to_string(), Value::Map(response_headers));
    result.insert("is_ok".to_string(), Value::Bool((200..300).contains(&status)));
    Ok(Value::Map(result))
}

/// Convert a Keel `Value` tree into a `serde_json::Value` suitable for
/// sending as an HTTP JSON body.
fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::None => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Integer(n) => serde_json::Value::Number((*n).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::EnumVariant(_, v, _) => serde_json::Value::String(v.clone()),
        Value::List(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Map(m) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in m {
                obj.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::Null,
    }
}
