//! Runtime: stdlib module and built-in installation.
//!
//! std modules (`ai`, `io`, `file`, …) are registered in the interpreter's
//! namespace registry but enter a program's scope only through
//! `use std/<name>`. The always-ambient surface is limited to the agent
//! verbs (`run`, `stop`, `send`, `delegate`, `broadcast`), generic
//! utilities (`min`, `max`, `typeof`), and the symbol-hint identifiers.

use std::sync::Arc;

use crate::interpreter::value::Value;
use crate::interpreter::{CallArgValue, Host};

pub(crate) mod args;
pub mod context;
pub mod convert;
pub mod db_provider;
pub mod email;
pub mod human;
pub mod llm;
pub mod namespace;
pub mod namespaces;

pub use convert::{json_to_value, value_to_json};
pub(crate) use namespaces::memory::derive_program_name_with_fs;

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

pub fn install_prelude(host: &mut dyn Host) {
    for s in SYMBOL_IDENTS {
        host.insert_global((*s).to_string(), Value::String((*s).to_string()));
    }

    namespaces::install(host);
    install_top_level_agent_fns(host);
    namespaces::agent::install_messaging_fns(host);
    install_min_max(host);
    install_typeof(host);
}

fn install_typeof(host: &mut dyn Host) {
    host.register_top_fn(
        "typeof",
        Arc::new(|_host: &mut dyn Host, args: Vec<CallArgValue>| {
            Box::pin(async move {
                let val = args
                    .into_iter()
                    .next()
                    .map(|a| a.value)
                    .unwrap_or(Value::None);
                let name = match &val {
                    Value::Struct(type_name, _) => type_name.clone(),
                    Value::EnumVariant(type_name, _, _) => type_name.clone(),
                    other => other.type_name().to_string(),
                };
                Ok(Value::String(name))
            })
        }),
    );
}

fn cmp_values(a: &Value, b: &Value) -> miette::Result<std::cmp::Ordering> {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Ok(x.cmp(y)),
        (Value::Float(x), Value::Float(y)) => {
            Ok(x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal))
        }
        (Value::Integer(x), Value::Float(y)) => Ok((*x as f64)
            .partial_cmp(y)
            .unwrap_or(std::cmp::Ordering::Equal)),
        (Value::Float(x), Value::Integer(y)) => Ok(x
            .partial_cmp(&(*y as f64))
            .unwrap_or(std::cmp::Ordering::Equal)),
        (Value::String(x), Value::String(y)) => Ok(x.cmp(y)),
        _ => Err(miette::miette!(
            "cannot compare `{}` with `{}`",
            a.type_name(),
            b.type_name()
        )),
    }
}

fn install_min_max(host: &mut dyn Host) {
    use std::cmp::Ordering;

    for want_max in [false, true] {
        let name = if want_max { "max" } else { "min" };
        host.register_top_fn(
            name,
            Arc::new(move |host: &mut dyn Host, args: Vec<CallArgValue>| {
                Box::pin(async move {
                    let by_val = args
                        .iter()
                        .find(|a| a.name.as_deref() == Some("by"))
                        .map(|a| a.value.clone());
                    let positional: Vec<Value> = args
                        .into_iter()
                        .filter(|a| a.name.is_none())
                        .map(|a| a.value)
                        .collect();
                    // A single list argument is auto-spread so that
                    // `min(items, by: |x| x.score)` iterates the list elements,
                    // mirroring Python's min(iterable, key=...) convention.
                    let items: Vec<Value> = match positional.as_slice() {
                        [Value::List(v)] => v.clone(),
                        _ => positional,
                    };
                    if items.is_empty() {
                        return Ok(Value::None);
                    }
                    let target = if want_max {
                        Ordering::Greater
                    } else {
                        Ordering::Less
                    };
                    match by_val {
                        Some(by) => {
                            let (params, body) = match by {
                                Value::Closure(p, b) => (p, *b),
                                _ => {
                                    return Err(miette::miette!(
                                        "`by:` argument must be a function"
                                    ));
                                }
                            };
                            let mut best = items[0].clone();
                            let mut best_key = host
                                .call_closure(
                                    &params,
                                    &body,
                                    vec![CallArgValue {
                                        name: None,
                                        value: best.clone(),
                                    }],
                                )
                                .await?;
                            for item in items.into_iter().skip(1) {
                                let key = host
                                    .call_closure(
                                        &params,
                                        &body,
                                        vec![CallArgValue {
                                            name: None,
                                            value: item.clone(),
                                        }],
                                    )
                                    .await?;
                                if cmp_values(&key, &best_key)? == target {
                                    best = item;
                                    best_key = key;
                                }
                            }
                            Ok(best)
                        }
                        None => {
                            // Use Comparable impl if items are structs.
                            let cmp_task = host.find_impl_task(&items[0], "compare");
                            if let Some(task) = cmp_task {
                                let mut best = items[0].clone();
                                for item in items.into_iter().skip(1) {
                                    let cmp_val = host
                                        .call_task(
                                            "compare",
                                            &task,
                                            vec![
                                                CallArgValue {
                                                    name: None,
                                                    value: best.clone(),
                                                },
                                                CallArgValue {
                                                    name: None,
                                                    value: item.clone(),
                                                },
                                            ],
                                        )
                                        .await?;
                                    let wins = if want_max {
                                        matches!(cmp_val, Value::Integer(n) if n < 0)
                                    } else {
                                        matches!(cmp_val, Value::Integer(n) if n > 0)
                                    };
                                    if wins {
                                        best = item;
                                    }
                                }
                                return Ok(best);
                            }
                            let mut best = items[0].clone();
                            for item in items.into_iter().skip(1) {
                                if cmp_values(&item, &best)? == target {
                                    best = item;
                                }
                            }
                            Ok(best)
                        }
                    }
                })
            }),
        );
    }
}

fn install_top_level_agent_fns(host: &mut dyn Host) {
    host.register_top_fn(
        "run",
        Arc::new(|host: &mut dyn Host, args: Vec<CallArgValue>| {
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
                host.start_agent(&agent_name).await?;
                Ok(Value::None)
            })
        }),
    );

    host.register_top_fn(
        "stop",
        Arc::new(|host: &mut dyn Host, args: Vec<CallArgValue>| {
            Box::pin(async move {
                let agent_name = match args.first().map(|a| &a.value) {
                    Some(Value::AgentRef(name)) => name.clone(),
                    _ => return Err(miette::miette!("stop() requires an agent argument")),
                };
                host.stop_agent(&agent_name).await?;
                Ok(Value::None)
            })
        }),
    );
}
