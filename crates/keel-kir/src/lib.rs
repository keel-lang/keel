//! `keel-kir` — the typed mid-level IR between the type checker and the
//! native backend (`designs/llvm-compilation.md` §2.2, §2.3).
//!
//! M0 scope: a data model (`ir`, `types`), a span table (`span_table`), a
//! textual dump (`dump`, driving `keel build --emit=kir`), a well-formedness
//! verifier (`passes::verify`), and AST -> KIR lowering for the scalar
//! subset only (`lower`). No LLVM dependency anywhere in this crate — only
//! `keel-codegen` (M1+) links LLVM, per the dependency rule in §2.2.
//!
//! Entry point: [`lower`].

pub mod dump;
pub mod ir;
pub mod lower;
pub mod mono;
pub mod passes;
pub mod span_table;
pub mod types;

use keel_syntax::ast::Program;

use ir::KirProgram;

/// Either stage of the KIR pipeline can fail: lowering (AST construct
/// outside the M0 scalar subset, or a local scalar-inference mismatch) or
/// the final verifier (an internal consistency bug in a lowering/pass — not
/// expected to trigger on any input that made it out of `lower_program`
/// cleanly, but checked anyway since it is cheap and catches lowering bugs
/// before they reach codegen).
#[derive(Debug, Clone)]
pub enum KirError {
    Lower(lower::LowerError),
    Verify(passes::verify::VerifyError),
}

impl std::fmt::Display for KirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KirError::Lower(e) => write!(f, "{e}"),
            KirError::Verify(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for KirError {}

impl From<lower::LowerError> for KirError {
    fn from(e: lower::LowerError) -> Self {
        KirError::Lower(e)
    }
}

impl From<passes::verify::VerifyError> for KirError {
    fn from(e: passes::verify::VerifyError) -> Self {
        KirError::Verify(e)
    }
}

/// Runs the full KIR pipeline on one already type-checked file: `lower ->
/// mono -> boxing -> rc -> verify` (fixed pass order, §2.3). `keel-codegen`
/// (M1+) consumes the result; M0 stops here — `--emit=kir` is the only
/// consumer today.
///
/// `file_name` is used only for the span table (diagnostics, dumps) — it
/// does not need to be a real path.
///
/// # Errors
///
/// See [`KirError`].
pub fn lower(program: &Program, file_name: &str) -> Result<KirProgram, KirError> {
    let program = lower::lower_program(program, file_name)?;
    let program = mono::monomorphize(program);
    let program = passes::boxing::insert_boxing(program);
    let program = passes::rc::insert_rc(program);
    passes::verify::verify(&program)?;
    Ok(program)
}
