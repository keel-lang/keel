//! `KirType` — the lowered type vocabulary KIR expressions carry.
//!
//! M0/M1 lower the scalar subset only (see `designs/llvm-compilation.md`
//! §4); M2 adds named structs. The remaining variants sketched in the
//! design doc's `KirType` (§2.3) — containers, anonymous struct shapes,
//! enums, nullable, func, boxed `dynamic`, opaque handles — are
//! deliberately not modeled yet; adding them is later-M2+ work, done
//! alongside the lowering support that produces them.

use crate::ir::{EnumId, KirProgram, StructId};

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
    /// A named struct type (`type X { .. }`) — see `KirProgram::structs` for
    /// the field layout this id indexes. Anonymous shapes have no `KirType`
    /// yet (deferred, see `ir.rs`'s `StructLayout` doc).
    Struct(StructId),
    /// A simple (unit-variant) enum type (`type Priority = low | medium |
    /// high`) — see `KirProgram::enums` for the variant list this id
    /// indexes. Rich (payload-carrying) variants have no `KirType` yet
    /// (deferred, see `ir.rs`'s `EnumLayout` doc). By-value `i32` tag — no
    /// heap allocation or RC, unlike `Struct`.
    Enum(EnumId),
}

impl KirType {
    /// Pretty name used by `dump.rs` and diagnostics. Doesn't have access to
    /// `KirProgram::structs`, so a struct type prints as the generic
    /// `"struct"` here — call sites that can name the actual struct (lowering
    /// error messages, which already have the name in hand; `dump.rs`, which
    /// carries the whole program) do so directly instead of through this.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            KirType::I64 => "int",
            KirType::F64 => "float",
            KirType::Bool => "bool",
            KirType::Unit => "none",
            KirType::Str => "str",
            KirType::Struct(_) => "struct",
            KirType::Enum(_) => "enum",
        }
    }

    /// `true` for the two variants `int`/`float` allow arithmetic on ( `+ - * / %` ).
    #[must_use]
    pub const fn is_numeric(self) -> bool {
        matches!(self, KirType::I64 | KirType::F64)
    }

    /// `true` for types the value ABI represents as a `ptr` to RC'd heap
    /// data — `str` today, plus a struct whose layout contains a heap field
    /// anywhere (recursively, via `StructLayout::is_heap`). Everything else
    /// is a plain by-value scalar/aggregate.
    #[must_use]
    pub fn is_heap(self, program: &KirProgram) -> bool {
        match self {
            KirType::Str => true,
            KirType::Struct(id) => program.structs[id].is_heap(program),
            KirType::I64 | KirType::F64 | KirType::Bool | KirType::Unit | KirType::Enum(_) => false,
        }
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
