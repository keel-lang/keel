//! Tree-walking interpreter for Keel v0.1.
//!
//! Evaluates programs against the new AST: agent bodies with
//! `@attribute` clauses, namespace-dispatched calls (`Ai.classify`,
//! `Io.notify`, `Schedule.every`, ...) resolved through the runtime
//! prelude, and structured `self.` state mutation.

pub mod environment;
pub mod host;
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
mod promote;
mod state;
mod stmt;
mod store;

pub(crate) use binary::{eval_binary, is_pascal_case};
pub(crate) use binding::bind_value;
pub use entry::{
    TestOutcome, run_tests_with_source_and_runtime, run_with_source_and_runtime, test_names,
};
pub(crate) use error::{RuntimeError, RuntimeErrorKind, runtime_error};
pub use host::Host;
#[cfg(test)]
pub use host::MockHost;
pub use state::{BuiltinFn, CallArgValue, Event, Interpreter, Namespace};
pub use stmt::StmtOutcome;
