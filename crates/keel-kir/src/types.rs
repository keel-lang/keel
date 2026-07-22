//! `KirType` — the lowered type vocabulary KIR expressions carry.
//!
//! M0/M1 lower the scalar subset only (see `designs/llvm-compilation.md`
//! §4), so only the unboxed-scalar variants are reachable from `lower/`
//! today. The remaining variants sketched in the design doc's `KirType`
//! (§2.3) — containers, structs, enums, nullable, func, boxed `dynamic`,
//! opaque handles — are deliberately not modeled yet; adding them is M2+
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

    /// Maps a catalog [`keel_catalog::builtins::TySpec`] to the equivalent
    /// `KirType`, for the scalar subset a stdlib namespace method can take
    /// or return in M1. `None` for anything needing boxing, containers, or
    /// nullable (M2+) — callers reject those with a `LowerError`/
    /// `VerifyError` naming the construct, not a panic.
    #[must_use]
    pub fn from_tyspec(spec: keel_catalog::builtins::TySpec) -> Option<KirType> {
        use keel_catalog::builtins::TySpec;
        match spec {
            TySpec::Int => Some(KirType::I64),
            TySpec::Float => Some(KirType::F64),
            TySpec::Bool => Some(KirType::Bool),
            TySpec::Str => Some(KirType::Str),
            TySpec::None_ => Some(KirType::Unit),
            TySpec::Datetime
            | TySpec::Duration
            | TySpec::Uuid
            | TySpec::Dynamic
            | TySpec::DbConnection
            | TySpec::NullableStr
            | TySpec::NullableInt
            | TySpec::NullableFloat
            | TySpec::NullableUuid
            | TySpec::NullableDatetime
            | TySpec::NullableDynamic
            | TySpec::ListOfStr
            | TySpec::ListOfInt
            | TySpec::ListOfListOfStr
            | TySpec::ListOfMapStrStr
            | TySpec::ListOfMapStrDynamic
            | TySpec::Unknown => None,
        }
    }

    /// Maps the checker's resolved [`keel_compiler::types::ty::Ty`] (from
    /// `CheckArtifacts::expr_types`) to the equivalent `KirType`, for the
    /// scalar subset this crate lowers today. `None` for anything needing
    /// containers, structs, enums, nullable, or boxing — callers reject
    /// those with a `LowerError`/`VerifyError` naming the construct, same
    /// policy as [`Self::from_tyspec`].
    #[must_use]
    pub fn from_ty(ty: &keel_compiler::types::ty::Ty) -> Option<KirType> {
        use keel_compiler::types::ty::Ty;
        match ty {
            Ty::Int => Some(KirType::I64),
            Ty::Float => Some(KirType::F64),
            Ty::Bool => Some(KirType::Bool),
            Ty::Str => Some(KirType::Str),
            Ty::None_ => Some(KirType::Unit),
            Ty::Duration
            | Ty::Datetime
            | Ty::Uuid
            | Ty::List(_)
            | Ty::Map(_, _)
            | Ty::Set(_)
            | Ty::Struct { .. }
            | Ty::Tuple(_)
            | Ty::Func(_, _)
            | Ty::Enum(_, _)
            | Ty::DbConnection
            | Ty::Dynamic
            | Ty::Error
            | Ty::Unresolved(_)
            | Ty::Unknown(_)
            | Ty::Nullable(_) => None,
        }
    }
}

impl std::fmt::Display for KirType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}
