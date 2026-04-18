//! Bytecode compiler — deferred post-v0.1.
//!
//! The tree-walking interpreter is the supported execution path for
//! v0.1. A real bytecode VM has to re-solve async dispatch, closure
//! capture across event-loop boundaries, and runtime-pluggable
//! namespaces — serious engineering without matching user payoff.
//! The `keel build` verb is kept as a stub so the CLI surface is
//! stable; revisit when there's a concrete motivator (LLVM/WASM
//! backend, embeddable runtime).

use crate::ast::Program;
use super::bytecode::CompiledProgram;

pub fn compile(_program: &Program) -> Result<CompiledProgram, String> {
    Err("keel build is deferred post-v0.1 — use `keel run` (the tree-walking \
         interpreter is the supported execution path for v0.1). See ROADMAP.md."
        .to_string())
}
