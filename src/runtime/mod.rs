//! Runtime: prelude namespace installation for v0.1.
//!
//! Every Keel program starts with these namespaces in scope:
//!   `Ai`, `Io`, `Schedule`, `Email`, `Http`, `Memory`, `Async`,
//!   `Control`, `Env`, `Log`, `Agent`, `Cache`, `Str`, `File`, `Json`.
//!
//! Top-level convenience bindings (`run`, `stop`) wrap `Agent.*`.

use std::sync::Arc;

use crate::interpreter::value::Value;
use crate::interpreter::{CallArgValue, Interpreter};

pub mod context;
pub mod convert;
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
