//! `KirType` — the lowered type vocabulary KIR expressions carry.
//!
//! M0 lowers the scalar subset only (see `designs/llvm-compilation.md` §4,
//! M0), so only the unboxed-scalar variants are reachable from `lower/`
//! today. The remaining variants sketched in the design doc's `KirType`
//! (§2.3) — containers, structs, enums, nullable, func, boxed `dynamic`,
//! opaque handles — are deliberately not modeled yet; adding them is M1+
//! work, done alongside the lowering support that produces them.

/// A KIR-level type. Every KIR expression carries one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KirType {
    /// `int` -> LLVM `i64`.
    I64,
    /// `float` -> LLVM `double`.
    F64,
    /// `bool` -> LLVM `i1` (i8 in memory).
    Bool,
    /// `none` / statements with no value -> LLVM `void`.
    Unit,
    /// `str` -> `ptr` to an RC'd `KeelStr` (opaque to KIR; every operation on
    /// it is a runtime call). Included in the scalar subset because string
    /// literals and `+` concatenation are simple enough to lower now without
    /// the container ABI landing first.
    Str,
}

impl KirType {
    /// Pretty name used by `dump.rs` and diagnostics.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            KirType::I64 => "int",
            KirType::F64 => "float",
            KirType::Bool => "bool",
            KirType::Unit => "none",
            KirType::Str => "str",
        }
    }

    /// `true` for the two variants `int`/`float` allow arithmetic on ( `+ - * / %` ).
    #[must_use]
    pub const fn is_numeric(self) -> bool {
        matches!(self, KirType::I64 | KirType::F64)
    }
}

impl std::fmt::Display for KirType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}
