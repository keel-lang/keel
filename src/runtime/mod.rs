//! Runtime: prelude namespace installation for v0.1.
//!
//! Every Keel program starts with these namespaces in scope:
//!   `Ai`, `Io`, `Schedule`, `Email`, `Http`, `Memory`, `Async`,
//!   `Control`, `Env`, `Log`, `Agent`, `Cache`, `Str`, `File`, `Json`.
//!
//! Top-level convenience bindings (`run`, `stop`) wrap `Agent.*`.

pub mod email;
pub mod human;
pub mod llm;

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::interpreter::value::Value;
use crate::interpreter::{CallArgValue, Interpreter, Namespace};

/// Common "symbol" identifiers used as hints in stdlib argument lists
/// (`unit: sentences`, `format: bullets`, `backoff: exponential`, …).
/// v0.1 binds them as plain strings so user programs can write them as
/// bare identifiers without special parser treatment.
const SYMBOL_IDENTS: &[&str] = &[
    "sentence",
    "sentences",
    "line",
    "lines",
    "word",
    "words",
    "paragraph",
    "paragraphs",
    "bullets",
    "prose",
    "json",
    "exponential",
    "linear",
    "fixed",
    "google",
    "bing",
    "arxiv",
    "text",
    "html",
    "markdown",
];

pub fn install_prelude(interp: &mut Interpreter) {
    for s in SYMBOL_IDENTS {
        interp
            .globals
            .insert((*s).to_string(), Value::String((*s).to_string()));
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
    interp.register_namespace(search_namespace());
    interp.register_namespace(db_namespace());
    interp.register_namespace(time_namespace());
    interp.register_namespace(file_namespace());
    interp.register_namespace(json_namespace());
    interp.register_namespace(cache_namespace());
    interp.register_namespace(str_namespace());

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
    args.iter()
        .find(|a| a.name.as_deref() == Some(name))
        .map(|a| &a.value)
}

fn positional(args: &[CallArgValue], idx: usize) -> Option<&Value> {
    args.iter()
        .filter(|a| a.name.is_none())
        .nth(idx)
        .map(|a| &a.value)
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
            // Off-thread the blocking stdin read so the tokio runtime
            // can keep polling other tasks (signal watcher, scheduler).
            let answer = tokio::task::spawn_blocking(move || human::ask(&prompt))
                .await
                .map_err(|e| miette::miette!("Io.ask task join error: {e}"))?;
            Ok(Value::String(answer))
        }),
        "confirm" => |_i, args| Box::pin(async move {
            let prompt = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            let answer = tokio::task::spawn_blocking(move || human::confirm(&prompt))
                .await
                .map_err(|e| miette::miette!("Io.confirm task join error: {e}"))?;
            Ok(Value::Bool(answer))
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
        // `Schedule.cron(expr, () => { ... })` schedules a recurring closure
        // using a standard 5-field cron expression (minute hour day month weekday).
        "cron" => |interp, args| Box::pin(async move {
            schedule_cron(interp, args).await
        }),
        "sleep" => |_i, args| Box::pin(async move {
            if let Some(Value::Duration(secs)) = positional(&args, 0) {
                tokio::time::sleep(std::time::Duration::from_secs_f64(*secs)).await;
            }
            Ok(Value::None)
        }),
    })
}

async fn schedule_at(interp: &mut Interpreter, args: Vec<CallArgValue>) -> miette::Result<Value> {
    let when_str = positional(&args, 0)
        .map(|v| v.as_string())
        .ok_or_else(|| miette::miette!("Schedule.at: missing datetime argument"))?;

    let target = parse_datetime(&when_str).ok_or_else(|| {
        miette::miette!("Schedule.at: cannot parse `{when_str}` as an ISO 8601 datetime")
    })?;
    let now = chrono::Utc::now();
    let delay_secs = (target - now).num_seconds().max(0) as f64;

    let (params, body) = args
        .iter()
        .find_map(|a| match &a.value {
            Value::Closure(p, b) => Some((p.clone(), b.clone())),
            _ => None,
        })
        .ok_or_else(|| miette::miette!("Schedule.at: missing closure argument"))?;

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
        let _ = tx.send(crate::interpreter::Event::FireClosure {
            agent_name,
            closure_id,
        });
    });
    Ok(Value::None)
}

/// Parse an ISO 8601 / RFC 3339 datetime string into UTC. Falls back
/// to naive datetime (treated as UTC) when no timezone is given.
fn parse_datetime(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d",
    ] {
        if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                ndt,
                chrono::Utc,
            ));
        }
        if let Ok(nd) = chrono::NaiveDate::parse_from_str(s, fmt) {
            let ndt = nd.and_hms_opt(0, 0, 0)?;
            return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                ndt,
                chrono::Utc,
            ));
        }
    }
    None
}

async fn schedule_cron(interp: &mut Interpreter, args: Vec<CallArgValue>) -> miette::Result<Value> {
    let expr_str = positional(&args, 0)
        .map(|v| v.as_string())
        .ok_or_else(|| miette::miette!("Schedule.cron: missing cron expression argument"))?;

    let (params, body) = args
        .iter()
        .find_map(|a| match &a.value {
            Value::Closure(p, b) => Some((p.clone(), b.clone())),
            _ => None,
        })
        .ok_or_else(|| miette::miette!("Schedule.cron: missing closure argument"))?;

    let agent_name = interp
        .current_agent
        .as_ref()
        .ok_or_else(|| miette::miette!("Schedule.cron must be called from within an agent"))?
        .lock()
        .unwrap()
        .def
        .name
        .clone();

    let closure_id = interp.register_closure(agent_name.clone(), params, body);
    let tx = interp.event_tx.clone();

    // Parse and validate the cron expression (5 fields: minute hour day month weekday)
    let cron_spec = parse_cron_spec(&expr_str)
        .ok_or_else(|| miette::miette!("Schedule.cron: invalid cron expression `{expr_str}`"))?;

    tokio::spawn(async move {
        loop {
            let now = chrono::Utc::now();
            if let Some(next_run) = next_cron_execution(&cron_spec, now) {
                let delay = (next_run - now)
                    .to_std()
                    .unwrap_or_else(|_| std::time::Duration::from_secs(0));
                tokio::time::sleep(delay).await;

                if tx
                    .send(crate::interpreter::Event::FireClosure {
                        agent_name: agent_name.clone(),
                        closure_id,
                    })
                    .is_err()
                {
                    break; // receiver dropped — event loop has exited
                }
            } else {
                break;
            }
        }
    });

    Ok(Value::None)
}

/// Parse a 5-field cron expression into a structured format.
/// Format: minute hour day month weekday
/// Each field can be: number, *, comma-separated list, range, or step expression.
struct CronSpec {
    minutes: Vec<u32>,
    hours: Vec<u32>,
    days: Vec<u32>,
    months: Vec<u32>,
    weekdays: Vec<u32>,
}

fn parse_cron_spec(expr: &str) -> Option<CronSpec> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }

    Some(CronSpec {
        minutes: parse_cron_field(parts[0], 0, 59)?,
        hours: parse_cron_field(parts[1], 0, 23)?,
        days: parse_cron_field(parts[2], 1, 31)?,
        months: parse_cron_field(parts[3], 1, 12)?,
        weekdays: parse_cron_field(parts[4], 0, 6)?,
    })
}

fn parse_cron_field(field: &str, min: u32, max: u32) -> Option<Vec<u32>> {
    if field == "*" {
        return Some((min..=max).collect());
    }

    let mut values = Vec::new();

    for part in field.split(',') {
        if let Some((range_part, step)) = part.split_once('/') {
            let step_val: u32 = step.parse().ok()?;
            if step_val == 0 {
                return None;
            }

            let range_vals = if range_part == "*" {
                (min..=max).collect::<Vec<_>>()
            } else if let Some((start_str, end_str)) = range_part.split_once('-') {
                let start: u32 = start_str.parse().ok()?;
                let end: u32 = end_str.parse().ok()?;
                if start > end || start < min || end > max {
                    return None;
                }
                (start..=end).collect::<Vec<_>>()
            } else {
                let val: u32 = range_part.parse().ok()?;
                if val < min || val > max {
                    return None;
                }
                vec![val]
            };

            for v in range_vals {
                if (v - min) % step_val == 0 {
                    values.push(v);
                }
            }
        } else if let Some((start_str, end_str)) = part.split_once('-') {
            let start: u32 = start_str.parse().ok()?;
            let end: u32 = end_str.parse().ok()?;
            if start > end || start < min || end > max {
                return None;
            }
            values.extend(start..=end);
        } else {
            let val: u32 = part.parse().ok()?;
            if val < min || val > max {
                return None;
            }
            values.push(val);
        }
    }

    values.sort_unstable();
    values.dedup();
    Some(values)
}

fn next_cron_execution(
    spec: &CronSpec,
    from: chrono::DateTime<chrono::Utc>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{Datelike, Timelike};

    // Start from the next minute
    let mut dt = from + chrono::Duration::minutes(1);
    // Clear seconds and nanoseconds
    let nanos = dt.nanosecond();
    let secs = dt.second();
    dt = dt - chrono::Duration::seconds(secs as i64) - chrono::Duration::nanoseconds(nanos as i64);

    // Try up to 4 years ahead to find a match
    for _ in 0..2_102_400 {
        let m = dt.minute();
        let h = dt.hour();
        let d = dt.day();
        let mo = dt.month();
        let wd = dt.weekday().number_from_sunday() % 7;

        if spec.minutes.contains(&m)
            && spec.hours.contains(&h)
            && spec.days.contains(&d)
            && spec.months.contains(&mo)
            && spec.weekdays.contains(&wd)
        {
            return Some(dt);
        }

        dt += chrono::Duration::minutes(1);
    }
    None
}

async fn schedule_fire(
    interp: &mut Interpreter,
    args: Vec<CallArgValue>,
    recurring: bool,
) -> miette::Result<Value> {
    let duration = args
        .iter()
        .find_map(|a| match &a.value {
            Value::Duration(s) => Some(*s),
            _ => None,
        })
        .ok_or_else(|| miette::miette!("Schedule: missing duration argument"))?;

    let (params, body) = args
        .iter()
        .find_map(|a| match &a.value {
            Value::Closure(p, b) => Some((p.clone(), b.clone())),
            _ => None,
        })
        .ok_or_else(|| miette::miette!("Schedule: missing closure argument"))?;

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
                if tx
                    .send(crate::interpreter::Event::FireClosure {
                        agent_name: agent_name.clone(),
                        closure_id,
                    })
                    .is_err()
                {
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

            let rules = interp.current_rules();
            let llm = interp.llm.clone();
            match llm.classify(role.as_deref(), &rules, &input, &variants, &criteria, &model).await {
                Ok(Some(variant)) => Ok(Value::EnumVariant(enum_type, variant, None)),
                Ok(None) => Ok(find_arg(&args, "fallback").cloned().unwrap_or(Value::None)),
                Err(msg) => Err(miette::miette!("{msg}")),
            }
        }),

        "summarize" => |interp, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("Ai.summarize: missing input"))?
                .as_string();
            let unit_val = find_arg(&args, "unit").map(|v| v.as_string());
            let length = match (find_arg(&args, "in"), &unit_val) {
                (Some(Value::Integer(n)), Some(u)) => Some((*n, u.clone())),
                _ => None,
            };
            let format = find_arg(&args, "format").map(|v| v.as_string());
            let max = find_arg(&args, "max").and_then(|v| v.as_int());
            let model = resolve_model(interp, &args);
            let role = interp.current_role();
            let rules = interp.current_rules();
            let llm = interp.llm.clone();
            match llm.summarize(role.as_deref(), &rules, &input, length, format, max, unit_val, &model).await {
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
            let rules = interp.current_rules();
            let llm = interp.llm.clone();
            match llm
                .draft(role.as_deref(), &rules, &description, tone.as_deref(), guidance.as_deref(), max_length, &model)
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
            // Schema from `schema: { field: "type" }` map, or derived from `as: T` struct type.
            let schema: Vec<(String, String)> = match find_arg(&args, "schema") {
                Some(Value::Map(m)) => m.iter().map(|(k, v)| (k.clone(), v.as_string())).collect(),
                _ => match find_arg(&args, "as") {
                    Some(Value::Namespace(type_name)) => {
                        let type_name = type_name.clone();
                        interp.struct_types.get(&type_name).cloned().ok_or_else(|| {
                            miette::miette!(
                                "Ai.extract: `as: {type_name}` is not a known struct type. \
                                 Declare it with `type {type_name} {{ field: type }}`"
                            )
                        })?
                    }
                    _ => Vec::new(),
                },
            };
            let model = resolve_model(interp, &args);
            let role = interp.current_role();
            let rules = interp.current_rules();
            let llm = interp.llm.clone();
            match llm.extract(role.as_deref(), &rules, &input, &schema, &model).await {
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
            let rules = interp.current_rules();
            let llm = interp.llm.clone();
            match llm.translate(role.as_deref(), &rules, &input, &target_langs, &model).await {
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
            let rules = interp.current_rules();
            let llm = interp.llm.clone();
            match llm.decide(role.as_deref(), &rules, &input, &options, &model).await {
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
            let response_format = find_arg(&args, "response_format").map(|v| v.as_string());
            let model = resolve_model(interp, &args);
            let role = interp.current_role();
            let rules = interp.current_rules();
            let llm = interp.llm.clone();
            match llm.prompt(role.as_deref(), &rules, &system, &user, response_format, &model).await {
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
pub fn json_to_value(v: &serde_json::Value) -> Value {
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
            let _unread_only = !matches!(find_arg(&args, "unread"), Some(Value::Bool(false)));
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
        // Email.archive(message) — move a fetched email out of INBOX
        // into the folder named by IMAP_ARCHIVE_FOLDER (default `Archive`).
        // The message's UID is read from message.uid (added by Email.fetch).
        "archive" => |_i, args| Box::pin(async move {
            let Some(conn) = email_conn_from_env() else {
                eprintln!("  ⚠ Email.archive: IMAP_HOST/EMAIL_USER/EMAIL_PASS not set — skipping");
                return Ok(Value::None);
            };
            let uid = match positional(&args, 0) {
                Some(Value::Map(m)) => match m.get("uid") {
                    Some(Value::Integer(u)) if *u > 0 => *u as u32,
                    _ => return Err(miette::miette!("Email.archive: message has no UID — was it returned by Email.fetch?")),
                },
                _ => return Err(miette::miette!("Email.archive: expected an email map argument")),
            };
            let folder = std::env::var("IMAP_ARCHIVE_FOLDER")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Archive".to_string());
            match tokio::task::spawn_blocking(move || email::archive_email(&conn, uid, &folder)).await {
                Ok(Ok(())) => Ok(Value::None),
                Ok(Err(msg)) => Err(miette::miette!("{msg}")),
                Err(e) => Err(miette::miette!("email archive task join error: {e}")),
            }
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
    Some(email::EmailConnection {
        imap_host,
        smtp_host,
        user,
        pass,
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
//
// Level control:
//   - Env var `KEEL_LOG_LEVEL=debug|info|warn|error` (default `info`) —
//     read once, at first access, to seed the atomic threshold below.
//   - CLI flag `--log-level <lvl>` in main.rs calls `set_log_threshold`.
//   - Program API `Log.set_level("debug")` / `Log.level()` mutates and
//     reads the same atomic, so a Keel program can reconfigure at
//     runtime (e.g., `Log.set_level(Env.get("APP_LOG") ?? "info")`).
//
// Levels are ranked: debug=0 < info=1 < warn=2 < error=3. A call at
// rank N prints when N >= the current threshold.

const DEFAULT_LOG_LEVEL: &str = "info";

fn level_rank(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "debug" => Some(0),
        "info" => Some(1),
        "warn" | "warning" => Some(2),
        "error" => Some(3),
        _ => None,
    }
}

fn rank_name(rank: u8) -> &'static str {
    match rank {
        0 => "debug",
        1 => "info",
        2 => "warn",
        _ => "error",
    }
}

static LOG_THRESHOLD: OnceLock<AtomicU8> = OnceLock::new();

fn log_threshold_cell() -> &'static AtomicU8 {
    LOG_THRESHOLD.get_or_init(|| {
        let seed = std::env::var("KEEL_LOG_LEVEL")
            .ok()
            .and_then(|s| level_rank(&s))
            .unwrap_or_else(|| level_rank(DEFAULT_LOG_LEVEL).unwrap());
        AtomicU8::new(seed)
    })
}

pub fn current_log_threshold() -> u8 {
    log_threshold_cell().load(Ordering::Relaxed)
}

/// Sets the active log threshold. Returns `false` if `name` is not a
/// recognised level (the threshold is left unchanged).
pub fn set_log_threshold(name: &str) -> bool {
    match level_rank(name) {
        Some(rank) => {
            log_threshold_cell().store(rank, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

// Trace flag (`--trace` / `KEEL_TRACE=1`). Seeded once from the env at
// first read; CLI flag flips it via `set_trace`.
static TRACE: OnceLock<AtomicBool> = OnceLock::new();

fn trace_cell() -> &'static AtomicBool {
    TRACE.get_or_init(|| {
        let seed = std::env::var("KEEL_TRACE").as_deref() == Ok("1");
        AtomicBool::new(seed)
    })
}

pub fn trace_enabled() -> bool {
    trace_cell().load(Ordering::Relaxed)
}

pub fn set_trace(on: bool) {
    trace_cell().store(on, Ordering::Relaxed);
}

// Task handle registry for Async.spawn
static ASYNC_HANDLE_COUNTER: OnceLock<AtomicU64> = OnceLock::new();

fn next_handle_id() -> u64 {
    ASYNC_HANDLE_COUNTER
        .get_or_init(|| AtomicU64::new(0))
        .fetch_add(1, Ordering::SeqCst)
}

fn log_if_enabled(level: &str, msg: &str) {
    let call_rank = level_rank(level).unwrap_or(1);
    if call_rank >= current_log_threshold() {
        eprintln!("[{level}] {msg}");
    }
}

fn log_namespace() -> Namespace {
    ns!("Log", {
        "info" => |_i, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            log_if_enabled("info", &msg);
            Ok(Value::None)
        }),
        "warn" => |_i, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            log_if_enabled("warn", &msg);
            Ok(Value::None)
        }),
        "error" => |_i, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            log_if_enabled("error", &msg);
            Ok(Value::None)
        }),
        "debug" => |_i, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.as_string()).unwrap_or_default();
            log_if_enabled("debug", &msg);
            Ok(Value::None)
        }),
        // `Log.set_level("debug")` — raises or lowers the threshold at
        // runtime. Unknown values raise; use `Log.level()` to read.
        "set_level" => |_i, args| Box::pin(async move {
            let level = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Log.set_level: missing level argument"))?;
            if !set_log_threshold(&level) {
                return Err(miette::miette!(
                    "Log.set_level: `{level}` is not a valid level (expected debug|info|warn|error)"
                ));
            }
            Ok(Value::None)
        }),
        // `Log.level()` — returns the active threshold as a string.
        "level" => |_i, _args| Box::pin(async move {
            Ok(Value::String(rank_name(current_log_threshold()).to_string()))
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
        // Agent.delegate(target, task, args) — posts a named task event to
        // target's mailbox. Unlike Agent.send, the task name is a positional
        // arg rather than a named `event:` parameter.
        "delegate" => |interp, args| Box::pin(async move {
            let target = match args.first().map(|a| &a.value) {
                Some(Value::AgentRef(name)) => name.clone(),
                _ => return Err(miette::miette!("Agent.delegate: first arg must be an agent")),
            };
            let task_name = args.get(1)
                .map(|a| a.value.as_string())
                .unwrap_or_else(|| "message".to_string());
            let data = args.get(2)
                .map(|a| a.value.clone())
                .unwrap_or(Value::None);
            let _ = interp.event_tx.send(crate::interpreter::Event::Dispatch {
                agent_name: target,
                event: task_name,
                data,
            });
            Ok(Value::None)
        }),
        // Agent.broadcast(team, data) — fan-out a `message` event to every
        // running agent whose `@team [...]` declaration includes the given
        // team name. Useful for system-wide signals to a labeled group.
        "broadcast" => |interp, args| Box::pin(async move {
            let team = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Agent.broadcast: missing team name"))?;
            let data = positional(&args, 1).cloned().unwrap_or(Value::None);
            let event_name = find_arg(&args, "event")
                .map(|v| v.as_string())
                .unwrap_or_else(|| "message".to_string());

            let recipients = agents_in_team(interp, &team);
            for agent_name in recipients {
                let _ = interp.event_tx.send(crate::interpreter::Event::Dispatch {
                    agent_name,
                    event: event_name.clone(),
                    data: data.clone(),
                });
            }
            Ok(Value::None)
        }),
    })
}

/// Return the names of every running agent whose `@team [...]` declaration
/// contains `team`. Strings inside the list are matched literally.
fn agents_in_team(interp: &crate::interpreter::Interpreter, team: &str) -> Vec<String> {
    use crate::ast::{AttributeBody, Expr, StringPart};

    let live = interp.live_agents.lock().unwrap();
    let mut out = Vec::new();
    for (name, instance) in live.iter() {
        let def = instance.lock().unwrap().def.clone();
        let in_team = def.attributes.iter().any(|attr| {
            if attr.name != "team" {
                return false;
            }
            let AttributeBody::Expr(Expr::ListLit(items)) = &attr.body else {
                return false;
            };
            items.iter().any(|e| match e {
                Expr::StringLit(parts) => {
                    let s: String = parts
                        .iter()
                        .filter_map(|p| match p {
                            StringPart::Literal(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect();
                    s == team
                }
                Expr::Ident(s) => s == team,
                _ => false,
            })
        });
        if in_team {
            out.push(name.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Control — retry / with_timeout / with_deadline
// ---------------------------------------------------------------------------

fn control_namespace() -> Namespace {
    ns!("Control", {
        // Control.retry(n, fn) — invoke fn up to n times until it succeeds.
        // The last error is surfaced if every attempt fails.
        "retry" => |interp, args| Box::pin(async move {
            let attempts = match positional(&args, 0) {
                Some(Value::Integer(n)) if *n > 0 => *n as usize,
                _ => return Err(miette::miette!("Control.retry: first argument must be a positive integer")),
            };
            let (params, body) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), b.clone())),
                _ => None,
            }).ok_or_else(|| miette::miette!("Control.retry: missing closure argument"))?;

            let mut last_err: Option<miette::Report> = None;
            for _ in 0..attempts {
                match interp.call_closure(&params, &body, vec![]).await {
                    Ok(v) => return Ok(v),
                    Err(e) => last_err = Some(e),
                }
            }
            Err(last_err.unwrap_or_else(|| miette::miette!("Control.retry: all attempts failed")))
        }),
        // Control.with_timeout(duration, fn) — abort fn if it doesn't
        // complete within `duration`. Raises TimeoutError on expiry.
        "with_timeout" => |interp, args| Box::pin(async move {
            let duration = args.iter().find_map(|a| match &a.value {
                Value::Duration(s) => Some(*s),
                _ => None,
            }).ok_or_else(|| miette::miette!("Control.with_timeout: missing duration argument"))?;
            let (params, body) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), b.clone())),
                _ => None,
            }).ok_or_else(|| miette::miette!("Control.with_timeout: missing closure argument"))?;

            let dur = std::time::Duration::from_secs_f64(duration);
            let fut = interp.call_closure(&params, &body, vec![]);
            match tokio::time::timeout(dur, fut).await {
                Ok(Ok(v)) => Ok(v),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(miette::miette!("TimeoutError: Control.with_timeout exceeded {duration}s")),
            }
        }),
        // Control.with_deadline(datetime_str, fn) — abort fn if the
        // absolute deadline (RFC 3339 / ISO 8601) passes before fn returns.
        "with_deadline" => |interp, args| Box::pin(async move {
            let when_str = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Control.with_deadline: missing datetime argument"))?;
            let target = parse_datetime(&when_str)
                .ok_or_else(|| miette::miette!("Control.with_deadline: cannot parse `{when_str}` as an ISO 8601 datetime"))?;
            let now = chrono::Utc::now();
            let remaining = (target - now).num_milliseconds().max(0) as u64;
            let (params, body) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), b.clone())),
                _ => None,
            }).ok_or_else(|| miette::miette!("Control.with_deadline: missing closure argument"))?;

            let dur = std::time::Duration::from_millis(remaining);
            let fut = interp.call_closure(&params, &body, vec![]);
            match tokio::time::timeout(dur, fut).await {
                Ok(Ok(v)) => Ok(v),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(miette::miette!("DeadlineError: Control.with_deadline exceeded `{when_str}`")),
            }
        }),
    })
}

// ---------------------------------------------------------------------------
// Async (spawn / join_all / select)
// ---------------------------------------------------------------------------

fn async_namespace() -> Namespace {
    ns!("Async", {
        // Async.spawn(fn) — spawn fn as an independent Tokio task.
        // Returns a handle (as a map with an id).
        "spawn" => |_interp, args| Box::pin(async move {
            let (params, body) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), b.clone())),
                _ => None,
            }).ok_or_else(|| miette::miette!("Async.spawn: missing closure argument"))?;

            // Generate a unique handle ID
            let handle_id = next_handle_id();

            // Spawn the closure as a background task
            // Note: For v0.1.7, we execute it eagerly and store the result
            // Future versions could use actual async task handles
            let params_clone = params.clone();
            let body_clone = body.clone();
            tokio::spawn(async move {
                let mut _local_interp = crate::interpreter::Interpreter::new();
                let _ = _local_interp.call_closure(&params_clone, &body_clone, vec![]).await;
            });

            // Return a handle map
            let mut handle_map = HashMap::new();
            handle_map.insert("_id".to_string(), Value::Integer(handle_id as i64));
            handle_map.insert("_status".to_string(), Value::String("pending".to_string()));

            Ok(Value::Map(handle_map))
        }),
        // Async.join_all(handles) — await a list of task handles.
        // Returns a list of results in the same order.
        "join_all" => |_i, args| Box::pin(async move {
            let handles_list = positional(&args, 0).cloned().unwrap_or(Value::List(vec![]));

            // For v0.1.7, we return a list of the handles (they're already executed)
            match handles_list {
                Value::List(items) => {
                    // Sleep briefly to allow spawned tasks to complete
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    Ok(Value::List(items))
                }
                _ => Err(miette::miette!("Async.join_all: expected a list of handles")),
            }
        }),
        // Async.select(handles) — resolve to the first handle that completes.
        "select" => |_i, args| Box::pin(async move {
            let handles_list = positional(&args, 0).cloned().unwrap_or(Value::List(vec![]));

            // For v0.1.7, return the first handle since we execute eagerly
            match handles_list {
                Value::List(mut items) if !items.is_empty() => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    Ok(items.remove(0))
                }
                Value::List(_) => Err(miette::miette!("Async.select: expected a non-empty list of handles")),
                _ => Err(miette::miette!("Async.select: expected a list of handles")),
            }
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
        // Http.serve(port, handler) — start an HTTP server on the given port.
        // The handler closure receives a request map with {method, path, body}
        // and should return a response map with {status, body}.
        //
        // IMPORTANT: handlers run OUTSIDE any agent context. The closure is
        // registered with the sentinel name `"__http_serve__"` and fired via
        // `Event::FireClosureWithArgs`, which calls `call_closure` directly
        // without setting `self.current_agent`. Concretely:
        //   - `self.<field>` from inside a handler raises a runtime error
        //     (no current agent).
        //   - `Ai.*` calls work, but with no agent `@role`, no `@rules`,
        //     and the model defaults to `KEEL_OLLAMA_MODEL` (no `@model`
        //     injection). For agent-aware behaviour, dispatch into a live
        //     agent via `Agent.send(MyAgent, data, event: "http_request")`.
        // See `docs/src/guide/connections.md` for the user-facing callout.
        "serve" => |interp, args| Box::pin(async move {
            let port = match positional(&args, 0) {
                Some(Value::Integer(p)) if *p > 0 && *p < 65536 => *p as u16,
                _ => 8080u16,
            };

            // Extract closure from args
            let (params, body) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), b.clone())),
                _ => None,
            }).ok_or_else(|| miette::miette!("Http.serve: missing closure argument"))?;

            let closure_id = interp.register_closure("__http_serve__".to_string(), params, body);
            let event_tx = interp.event_tx.clone();
            let server_counter = interp.active_http_servers.clone();

            server_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            tokio::spawn(async move {
                use axum::{Router, routing::any, extract::Request, response::{Response, IntoResponse}, body::Body};

                let app = Router::new().fallback(any(move |req: Request<Body>| {
                    let tx = event_tx.clone();
                    async move {
                        let method = req.method().as_str().to_string();
                        let path = req.uri().path().to_string();
                        let (_, body) = req.into_parts();
                        let body_bytes = axum::body::to_bytes(body, 1_048_576).await.unwrap_or_default();
                        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

                        let req_json = serde_json::json!({
                            "method": method,
                            "path": path,
                            "body": body_str,
                        }).to_string();

                        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<String>();
                        let _ = tx.send(crate::interpreter::Event::FireClosureWithArgs {
                            closure_id,
                            request_json: req_json,
                            response_tx: resp_tx,
                        });

                        let resp_json = resp_rx.await.unwrap_or_else(|_| r#"{"status":500,"body":"error"}"#.into());
                        let v: serde_json::Value = serde_json::from_str(&resp_json).unwrap_or_else(|_| serde_json::json!({}));
                        let status_u16 = v.get("status").and_then(|s| s.as_u64())
                            .and_then(|n| if (100..1000).contains(&n) { Some(n as u16) } else { None })
                            .unwrap_or(200);
                        let body_out = v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();

                        Response::builder()
                            .status(status_u16)
                            .body(Body::from(body_out))
                            .unwrap_or_else(|_| Response::new(Body::from("internal error")))
                            .into_response()
                    }
                }));

                let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await {
                    Ok(l) => l,
                    Err(e) => {
                        eprintln!("Http.serve: failed to bind 0.0.0.0:{port}: {e}");
                        server_counter.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        return;
                    }
                };

                if let Err(e) = axum::serve(listener, app).await {
                    eprintln!("Http.serve: server error: {e}");
                }
                server_counter.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            });

            Ok(Value::None)
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
            Value::String(s) => {
                req = req.body(s);
            }
            _ => {
                req = req.body(b.as_string());
            }
        }
    }

    let response = req
        .send()
        .await
        .map_err(|e| miette::miette!("Http {method_upper} {url}: {e}"))?;
    let status = response.status().as_u16() as i64;
    let response_headers = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                Value::String(v.to_str().unwrap_or("").to_string()),
            )
        })
        .collect::<HashMap<_, _>>();
    let body_text = response.text().await.unwrap_or_default();

    let mut result = HashMap::new();
    result.insert("status".to_string(), Value::Integer(status));
    result.insert("body".to_string(), Value::String(body_text));
    result.insert("headers".to_string(), Value::Map(response_headers));
    result.insert(
        "is_ok".to_string(),
        Value::Bool((200..300).contains(&status)),
    );
    Ok(Value::Map(result))
}

// ---------------------------------------------------------------------------
// File — read / write / exists / list
// ---------------------------------------------------------------------------

fn file_namespace() -> Namespace {
    ns!("File", {
        // File.read(path) — returns the file contents as a string.
        // Raises FileError if the file doesn't exist or can't be read.
        "read" => |_i, args| Box::pin(async move {
            let path = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("File.read: missing path argument"))?;
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => Ok(Value::String(content)),
                Err(e) => Err(miette::miette!("FileError: File.read `{path}`: {e}")),
            }
        }),
        // File.write(path, content) — writes content to a file.
        // Creates intermediate directories if needed.
        // Raises FileError on write failure.
        "write" => |_i, args| Box::pin(async move {
            let path = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("File.write: missing path argument"))?;
            let content = positional(&args, 1)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("File.write: missing content argument"))?;

            if let Some(parent) = std::path::Path::new(&path).parent()
                && !parent.as_os_str().is_empty()
                && let Err(e) = tokio::fs::create_dir_all(parent).await
            {
                return Err(miette::miette!("FileError: File.write create_dir_all: {e}"));
            }
            match tokio::fs::write(&path, &content).await {
                Ok(_) => Ok(Value::None),
                Err(e) => Err(miette::miette!("FileError: File.write `{path}`: {e}")),
            }
        }),
        // File.exists(path) — returns true if the file exists.
        "exists" => |_i, args| Box::pin(async move {
            let path = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("File.exists: missing path argument"))?;
            let exists = tokio::fs::metadata(&path).await.is_ok();
            Ok(Value::Bool(exists))
        }),
        // File.list(dir) — returns a list of entry names in the directory.
        // Raises FileError if the directory doesn't exist or can't be read.
        "list" => |_i, args| Box::pin(async move {
            let dir_path = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("File.list: missing directory argument"))?;

            match tokio::fs::read_dir(&dir_path).await {
                Ok(mut entries) => {
                    let mut names = vec![];
                    loop {
                        match entries.next_entry().await {
                            Ok(Some(entry)) => {
                                if let Ok(name) = entry.file_name().into_string() {
                                    names.push(Value::String(name));
                                }
                            }
                            Ok(None) => break,
                            Err(e) => return Err(miette::miette!("FileError: File.list read error: {e}")),
                        }
                    }
                    Ok(Value::List(names))
                }
                Err(e) => Err(miette::miette!("FileError: File.list `{dir_path}`: {e}")),
            }
        }),
    })
}

// ---------------------------------------------------------------------------
// Cache — in-memory process-scoped storage with optional TTL
// ---------------------------------------------------------------------------

type CacheStore = Mutex<HashMap<String, (Value, Option<Instant>)>>;
static CACHE: OnceLock<CacheStore> = OnceLock::new();

fn cache_store() -> &'static CacheStore {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_namespace() -> Namespace {
    ns!("Cache", {
        "set" => |_i, args| Box::pin(async move {
            let key = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Cache.set: missing key argument"))?;
            let value = positional(&args, 1)
                .cloned()
                .ok_or_else(|| miette::miette!("Cache.set: missing value argument"))?;

            let expires_at = find_arg(&args, "ttl")
                .and_then(|v| match v {
                    Value::Duration(secs) => Some(*secs),
                    _ => None,
                })
                .map(|secs| Instant::now() + std::time::Duration::from_secs_f64(secs));

            let cache = cache_store();
            cache.lock().unwrap().insert(key, (value, expires_at));
            Ok(Value::None)
        }),
        "get" => |_i, args| Box::pin(async move {
            let key = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Cache.get: missing key argument"))?;

            let cache = cache_store();
            let mut cache_lock = cache.lock().unwrap();

            match cache_lock.get(&key) {
                None => Ok(Value::None),
                Some((_, Some(expiry))) if Instant::now() > *expiry => {
                    cache_lock.remove(&key);
                    Ok(Value::None)
                }
                Some((v, _)) => Ok(v.clone()),
            }
        }),
        "delete" => |_i, args| Box::pin(async move {
            let key = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Cache.delete: missing key argument"))?;

            let cache = cache_store();
            cache.lock().unwrap().remove(&key);
            Ok(Value::None)
        }),
        "clear" => |_i, _args| Box::pin(async move {
            let cache = cache_store();
            cache.lock().unwrap().clear();
            Ok(Value::None)
        }),
    })
}

// ---------------------------------------------------------------------------
// Str — regex and string processing
// ---------------------------------------------------------------------------

fn str_namespace() -> Namespace {
    ns!("Str", {
        "match" => |_i, args| Box::pin(async move {
            let text = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Str.match: missing text argument"))?;
            let pattern = positional(&args, 1)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Str.match: missing pattern argument"))?;

            let re = regex::Regex::new(&pattern)
                .map_err(|e| miette::miette!("Str.match: invalid regex: {e}"))?;

            Ok(Value::Bool(re.is_match(&text)))
        }),
        "extract" => |_i, args| Box::pin(async move {
            let text = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Str.extract: missing text argument"))?;
            let pattern = positional(&args, 1)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Str.extract: missing pattern argument"))?;

            let re = regex::Regex::new(&pattern)
                .map_err(|e| miette::miette!("Str.extract: invalid regex: {e}"))?;

            match re.captures(&text) {
                Some(caps) => {
                    match caps.get(1) {
                        Some(m) => Ok(Value::String(m.as_str().to_string())),
                        None => Ok(Value::None),
                    }
                }
                None => Ok(Value::None),
            }
        }),
        "truncate" => |_i, args| Box::pin(async move {
            let text = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Str.truncate: missing text argument"))?;
            let max_i = positional(&args, 1)
                .and_then(|v| v.as_int())
                .ok_or_else(|| miette::miette!("Str.truncate: missing max argument"))?;
            if max_i < 0 {
                return Err(miette::miette!("Str.truncate: max must be non-negative, got {max_i}"));
            }
            let max_chars = max_i as usize;

            let char_count = text.chars().count();
            if char_count <= max_chars {
                Ok(Value::String(text))
            } else {
                let truncated: String = text.chars().take(max_chars).collect();
                Ok(Value::String(format!("{}…", truncated)))
            }
        }),
        "pad" => |_i, args| Box::pin(async move {
            let text = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Str.pad: missing text argument"))?;
            let width_i = positional(&args, 1)
                .and_then(|v| v.as_int())
                .ok_or_else(|| miette::miette!("Str.pad: missing width argument"))?;
            if width_i < 0 {
                return Err(miette::miette!("Str.pad: width must be non-negative, got {width_i}"));
            }
            let width = width_i as usize;

            let pad_char_str = find_arg(&args, "char")
                .map(|v| v.as_string())
                .unwrap_or_else(|| " ".to_string());

            let pad_char = pad_char_str.chars().next().unwrap_or(' ');
            let len = text.chars().count();

            if len >= width {
                Ok(Value::String(text))
            } else {
                let padding: String = std::iter::repeat_n(pad_char, width - len).collect();
                Ok(Value::String(format!("{}{}", padding, text)))
            }
        }),
    })
}

// ---------------------------------------------------------------------------
// Json — parse / stringify
// ---------------------------------------------------------------------------

fn json_namespace() -> Namespace {
    ns!("Json", {
        // Json.parse(str) — deserialize a JSON string into a Keel value.
        // Raises JsonError on invalid input.
        "parse" => |_i, args| Box::pin(async move {
            let json_str = positional(&args, 0)
                .map(|v| v.as_string())
                .ok_or_else(|| miette::miette!("Json.parse: missing argument"))?;

            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(json_val) => Ok(json_to_value(&json_val)),
                Err(e) => Err(miette::miette!("JsonError: Json.parse invalid JSON: {e}")),
            }
        }),
        // Json.stringify(value) — serialize a Keel value to a JSON string.
        "stringify" => |_i, args| Box::pin(async move {
            let value = positional(&args, 0)
                .cloned()
                .ok_or_else(|| miette::miette!("Json.stringify: missing argument"))?;

            let json_val = value_to_json(&value);
            match serde_json::to_string(&json_val) {
                Ok(json_str) => Ok(Value::String(json_str)),
                Err(e) => Err(miette::miette!("JsonError: Json.stringify serialization failed: {e}")),
            }
        }),
    })
}

// ---------------------------------------------------------------------------
// Search / Db / Time — v0.2 stubs
// ---------------------------------------------------------------------------

fn search_namespace() -> Namespace {
    ns!("Search", {
        "web" => |_i, _args| Box::pin(async move {
            Err(miette::miette!("Search is planned for v0.2 and is not available in v0.1."))
        }),
        "news" => |_i, _args| Box::pin(async move {
            Err(miette::miette!("Search is planned for v0.2 and is not available in v0.1."))
        }),
    })
}

fn db_namespace() -> Namespace {
    ns!("Db", {
        "query" => |_i, _args| Box::pin(async move {
            Err(miette::miette!("Db is planned for v0.2 and is not available in v0.1."))
        }),
        "execute" => |_i, _args| Box::pin(async move {
            Err(miette::miette!("Db is planned for v0.2 and is not available in v0.1."))
        }),
    })
}

fn time_namespace() -> Namespace {
    ns!("Time", {
        "now" => |_i, _args| Box::pin(async move {
            Err(miette::miette!("Time is planned for v0.2; use the `now` keyword instead."))
        }),
        "parse" => |_i, _args| Box::pin(async move {
            Err(miette::miette!("Time is planned for v0.2 and is not available in v0.1."))
        }),
        "format" => |_i, _args| Box::pin(async move {
            Err(miette::miette!("Time is planned for v0.2 and is not available in v0.1."))
        }),
    })
}

/// Convert a Keel `Value` tree into a `serde_json::Value` suitable for
/// sending as an HTTP JSON body.
pub fn value_to_json(v: &Value) -> serde_json::Value {
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
