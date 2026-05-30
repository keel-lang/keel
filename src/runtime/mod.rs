//! Runtime: prelude namespace installation for v0.1.
//!
//! Every Keel program starts with these namespaces in scope:
//!   `Ai`, `Io`, `Schedule`, `Email`, `Http`, `Memory`, `Async`,
//!   `Control`, `Env`, `Log`, `Agent`, `Cache`, `File`, `Json`,
//!   `Random`, `Uuid`.
//!
//! Top-level convenience bindings (`run`, `stop`) wrap `Agent.*`.

use std::sync::Arc;

use crate::interpreter::value::Value;
use crate::interpreter::{CallArgValue, Interpreter};

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

pub fn install_prelude(interp: &mut Interpreter) {
    for s in SYMBOL_IDENTS {
        interp
            .globals
            .insert((*s).to_string(), Value::String((*s).to_string()));
    }

    namespaces::install(interp);
    install_top_level_agent_fns(interp);
    install_min_max(interp);
    install_uuid_alias(interp);
    install_typeof(interp);
}

fn install_typeof(interp: &mut Interpreter) {
    interp.register_top_fn(
        "typeof",
        Arc::new(|_interp: &mut Interpreter, args: Vec<CallArgValue>| {
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

fn install_min_max(interp: &mut Interpreter) {
    use std::cmp::Ordering;

    for want_max in [false, true] {
        let name = if want_max { "max" } else { "min" };
        interp.register_top_fn(
            name,
            Arc::new(move |interp: &mut Interpreter, args: Vec<CallArgValue>| {
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
                            let mut best_key = interp
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
                                let key = interp
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
                            let cmp_task = interp.find_impl_task(&items[0], "compare");
                            if let Some(task) = cmp_task {
                                let mut best = items[0].clone();
                                for item in items.into_iter().skip(1) {
                                    let cmp_val = interp
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

fn install_top_level_agent_fns(interp: &mut Interpreter) {
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

fn install_uuid_alias(interp: &mut Interpreter) {
    interp.register_top_fn(
        "uuid",
        Arc::new(|interp: &mut Interpreter, _args: Vec<CallArgValue>| {
            Box::pin(async move { interp.call_namespace_method("Uuid", "v4", vec![]).await })
        }),
    );
}
