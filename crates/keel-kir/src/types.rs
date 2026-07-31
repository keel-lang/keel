//! `KirType` — the lowered type vocabulary KIR expressions carry.
//!
//! M0/M1 lower the scalar subset only (see `designs/llvm-compilation.md`
//! §4); M2 adds named structs, simple enums, `list[T]`, `map[str, V]`,
//! `set[T]` (all restricted to int/float/bool/str elements), and nullable.
//! The remaining variants sketched in the design doc's `KirType` (§2.3) —
//! anonymous struct shapes, rich enum variants, non-`str` map keys, func,
//! boxed `dynamic`, opaque handles — are deliberately not modeled yet;
//! adding them is later-M2+ work, done alongside the lowering support that
//! produces them.

use crate::ir::{EnumId, KirProgram, ListId, MapId, NullableId, SetId, StructId, TupleId};

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
    /// `list[T]` (`T` restricted to int/float/bool/str — see
    /// `KirProgram::lists`'s doc) — a `ptr` to an RC'd, always-clone-on-
    /// mutation `Value::List` (opaque to codegen; every operation is a
    /// `CallTarget::Rt` runtime call, `designs/llvm-compilation.md` §2.7).
    List(ListId),
    /// `map[str, V]` (`V` restricted to int/float/bool/str, and the key is
    /// always `str` — see `KirProgram::maps`'s doc; non-`str` keys are a
    /// later issue) — a `ptr` to an RC'd, always-clone-on-mutation
    /// `Value::Map` (opaque to codegen, same `CallTarget::Rt` treatment as
    /// `List`). `.insert(k, v)` is the one mutation op, and like every Keel
    /// mutation it returns a fresh map rather than updating in place.
    Map(MapId),
    /// `set[T]` (`T` restricted to int/float/bool/str, same as `List`) — a
    /// `ptr` to an RC'd, always-clone-on-mutation `Value::Set`, opaque to
    /// codegen exactly like `List` and `Map`. Distinct from `List` at runtime
    /// as well as statically: a set deduplicates on insert (by the runtime's
    /// value equality) and preserves insertion order. Dedup lives in
    /// `keel-runtime`'s `value::set_insert`, which both `keel-rt-ffi`'s
    /// `keel_set_insert` and the interpreter call — see `KirProgram::sets`.
    Set(SetId),
    /// `T?` (inner restricted to int/float/bool/str/list/struct — see
    /// `KirProgram::nullables`'s doc and `lower::is_nullable_inner_ty`). Per
    /// §1.1's representation split: a nullable *struct* is the same `ptr` as
    /// the non-nullable struct, with a null pointer meaning `none` (a native
    /// struct record is never `Value`-boxed, so null costs nothing extra); a
    /// nullable str/list is also the same `ptr`, but `none` is a boxed
    /// `Value::None` instead (str/list are already boxed `*const Value`
    /// pointers with no null-pointer bit to spare — see `keel-rt-ffi`'s
    /// `keel_box_none`/`keel_is_none`); a nullable scalar (int/float/bool)
    /// has no pointer to repurpose at all, so it's an explicit
    /// `{ i1 has_value, T }` pair, by value.
    Nullable(NullableId),
    /// A tuple shape (`(str, int)`) — see `KirProgram::tuples` for the
    /// positional element types this id indexes. A **by-value** LLVM
    /// aggregate: no heap allocation, no `ptr`, no RC, deliberately not the
    /// container ABI (`designs/llvm-compilation.md` §1.1). Element types are
    /// restricted to int/float/bool/str and nested tuples — see
    /// `TupleLayout`'s doc for the `str`-element RC caveat.
    Tuple(TupleId),
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
            KirType::List(_) => "list",
            KirType::Map(_) => "map",
            KirType::Set(_) => "set",
            KirType::Nullable(_) => "nullable",
            KirType::Tuple(_) => "tuple",
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
            KirType::List(_) | KirType::Map(_) | KirType::Set(_) => true,
            KirType::I64 | KirType::F64 | KirType::Bool | KirType::Unit | KirType::Enum(_) => false,
            // The aggregate itself is by value, but it *contains* RC'd data
            // when any element does (a `str` element is a boxed `Value*`).
            KirType::Tuple(id) => program.tuples[id].is_heap(program),
            // A nullable scalar is a by-value `{ i1, T }` pair, not a
            // pointer — everything else nullable wraps a `ptr` (see
            // `KirType::Nullable`'s doc). `is_nullable_inner_ty` guarantees
            // no other inner type is ever interned.
            KirType::Nullable(id) => !matches!(
                program.nullables[id],
                KirType::I64 | KirType::F64 | KirType::Bool
            ),
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
