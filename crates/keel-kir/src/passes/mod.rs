//! Fixed-order KIR pass manager: `mono -> boxing -> rc -> verify`
//! (`lower` runs before this, in `lower::lower_program`; see the pipeline
//! doc comment on `crate::lower`). Each pass re-runs `verify` in debug
//! builds in the full design; M0 only has the boxing/rc no-ops plus one real
//! verify at the end — see `designs/llvm-compilation.md` §2.3 "Fixed pass
//! order".

pub mod boxing;
pub mod rc;
pub mod verify;
