//! Inserts explicit `Box`/`Unbox` instructions at every typed <-> `dynamic`
//! boundary (`designs/llvm-compilation.md` §2.3 "boxing").
//!
//! No-op in M0: the scalar subset has no `dynamic` values and no `Box`/
//! `Unbox` variant exists in `ir::Expr` yet. Becomes real in M2, alongside
//! the `dynamic` type and `as` casts.

use crate::ir::KirProgram;

#[must_use]
pub fn insert_boxing(program: KirProgram) -> KirProgram {
    program
}
