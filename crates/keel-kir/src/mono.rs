//! Monomorphization: collects generic instantiations reachable from
//! `toplevel` + agent handlers and stamps one copy per instantiation
//! (`designs/llvm-compilation.md` §2.3 "Monomorphized").
//!
//! No-op in M0: `lower::decl::signature_of` already rejects generic tasks
//! (`task.type_params` must be empty), so every `KirProgram` reaching this
//! pass is already monomorphic. Becomes real in M1+, once generics are
//! accepted by lowering.

use crate::ir::KirProgram;

#[must_use]
pub fn monomorphize(program: KirProgram) -> KirProgram {
    program
}
