//! Tree-walking interpreter for Keel v0.1.
//!
//! Evaluates programs against the new AST: agent bodies with
//! `@attribute` clauses, namespace-dispatched calls (`Ai.classify`,
//! `Io.notify`, `Schedule.every`, ...) resolved through the runtime
//! prelude, and structured `self.` state mutation.

pub mod environment;
pub mod value;

mod agent;
mod binary;
mod binding;
mod call;
mod decl;
mod entry;
mod error;
mod expr;
mod methods;
mod state;
mod stmt;

pub(crate) use binary::{eval_binary, is_pascal_case};
pub(crate) use binding::bind_value;
pub use entry::{run_with_source, run_with_source_and_runtime};
pub(crate) use error::{RuntimeError, runtime_error};
pub use state::{
    AgentDef, AgentInstance, BuiltinFn, CallArgValue, Event, Interpreter, Namespace,
    ScheduledClosure,
};
pub use stmt::StmtOutcome;
