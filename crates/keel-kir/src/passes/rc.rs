//! Inserts `retain`/`release` around heap-value lifetimes — the single
//! owner of RC placement policy (`designs/llvm-compilation.md` §2.3 "rc").
//!
//! No-op in M0: the scalar subset's only heap-shaped value is `str`
//! (`KirType::Str`, opaque RC'd `KeelStr`), and no `Retain`/`Release`
//! statement variant exists in `ir::Stmt` yet — string values in M0 KIR are
//! produced and consumed structurally (literals, `+` concat) without the
//! runtime container ABI in the picture. Becomes real once `keel-codegen`
//! needs actual RC bookkeeping (M1+, when strings cross function/call
//! boundaries against a real runtime).

use crate::ir::KirProgram;

#[must_use]
pub fn insert_rc(program: KirProgram) -> KirProgram {
    program
}
