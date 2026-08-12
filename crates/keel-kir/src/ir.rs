//! KIR data model: `KirProgram`, `KirFunction`, `Block`, `Stmt`, `Expr`.
//!
//! This is a trimmed instantiation of the sketch in
//! `designs/llvm-compilation.md` §2.3, scoped to what M0's scalar-subset
//! lowering actually produces. Fields the design doc lists for later
//! milestones (structs, enums, agents, monomorphization stamps, RC
//! retain/release statements, `Box`/`Unbox`, result-ABI `can_raise`) are
//! intentionally omitted rather than stubbed with dead fields — they get
//! added in the milestone that lowers to them (see module docs on
//! `types.rs` for the same policy applied to `KirType`).
//!
//! KIR is structured (tree-shaped), not SSA/CFG — see §2.3 "Rationale".

use crate::span_table::{SpanId, SpanTable};
use crate::types::KirType;

/// Index into `KirProgram::functions`.
pub type FuncId = usize;

/// Index into a `KirFunction`'s `locals` (declaration order; shadowing
/// copies get distinct ids, mirroring the "plain assignment always declares"
/// scoping rule — see `AGENTS.md` / `feedback_keel_assignment_scoping`).
pub type LocalId = usize;

/// Index into `KirProgram::structs`.
pub type StructId = usize;

/// Index into `KirProgram::enums`.
pub type EnumId = usize;

/// Index into `KirProgram::tuples` — a *structural* intern id, like `ListId`
/// rather than `StructId`: `(str, int)` written anywhere in the program is
/// the same `TupleId`, since `SPEC.md` §2.8 makes tuples structural.
pub type TupleId = usize;

/// Index into `KirProgram::lists` — a *structural* intern id (unlike
/// `StructId`/`EnumId`, which are nominal: two declarations with identical
/// shape are still distinct types). `list[int]` written anywhere in the
/// program is the same `ListId` — see `lower/mod.rs`'s list-interning doc.
pub type ListId = usize;

/// Index into `KirProgram::nullables` — a *structural* intern id, same
/// rationale as `ListId`: `int?` written anywhere in the program is the same
/// `NullableId`.
pub type NullableId = usize;

/// Index into `KirProgram::maps` — a *structural* intern id over the
/// *value* type only (the key is always `str` — see `KirType::Map`'s doc),
/// same rationale as `ListId`: `map[str, int]` written anywhere in the
/// program is the same `MapId`.
pub type MapId = usize;

/// Index into `KirProgram::sets` — a *structural* intern id over the
/// element type, same rationale as `ListId`.
pub type SetId = usize;

/// A whole lowered program (currently: a single file's tasks + its
/// top-level statements — multi-module lowering is deferred until the
/// `keel-compiler` `ModuleGraph`/`CheckArtifacts` seam lands, see `lib.rs`).
#[derive(Debug, Clone)]
pub struct KirProgram {
    /// Every lowered task, in lowering order. Includes the synthetic
    /// top-level function referenced by `toplevel`.
    pub functions: Vec<KirFunction>,
    /// The function compiling the file's top-level statements (mirrors
    /// `Interpreter::execute`'s treatment of top-level code).
    pub toplevel: FuncId,
    /// Every named struct type (`type X { .. }`) declared in the file, in
    /// declaration order. Anonymous struct shapes (`{x: 1, y: 2}` with no
    /// resolvable named-type context) aren't interned yet — deferred until
    /// an M2 fixture actually needs one; see `lower/mod.rs`'s struct-
    /// resolution doc.
    pub structs: Vec<StructLayout>,
    /// Every simple (unit-variant) enum type (`type Priority = low | medium |
    /// high`) declared in the file, in declaration order. Rich (payload-
    /// carrying) variants aren't modeled yet — deferred to a follow-up
    /// issue; see `lower/mod.rs`'s enum-resolution doc.
    pub enums: Vec<EnumLayout>,
    /// Every distinct list element type interned so far (structural, not
    /// declaration order — see `ListId`'s doc). Only int/float/bool/str
    /// elements are modeled; a struct/enum element needs `Value`
    /// marshaling that doesn't exist yet (deferred, see `lower/expr.rs`'s
    /// list-literal lowering doc).
    pub lists: Vec<KirType>,
    /// Every distinct `map[str, V]` *value* type interned so far (structural,
    /// not declaration order — see `MapId`'s doc). Only int/float/bool/str
    /// values are modeled, same restriction as `lists`; the key is always
    /// `str` (non-`str` keys are a later issue, see `KirType::Map`'s doc).
    /// `.insert(k, v)` is the one mutation op, keyed by `str` like the rest.
    pub maps: Vec<KirType>,
    /// Every distinct `set[T]` element type interned so far (structural, not
    /// declaration order — see `SetId`'s doc). Only int/float/bool/str
    /// elements are modeled, same restriction as `lists` — narrower than the
    /// `set[T]` the checker accepts (which admits structs and nested
    /// containers), because those elements would need the `Value` marshaling
    /// `lists` also defers.
    pub sets: Vec<KirType>,
    /// Every distinct nullable inner type interned so far (structural, not
    /// declaration order — see `NullableId`'s doc). Only int/float/bool/str/
    /// list/struct inner types are modeled — see
    /// `lower::is_nullable_inner_ty`.
    pub nullables: Vec<KirType>,
    /// Every distinct tuple *shape* interned so far (structural, not
    /// declaration order — see `TupleId`'s doc). Element types are restricted
    /// to int/float/bool/str and nested tuples (`lower::is_tuple_element_ty`);
    /// containers, structs, enums, and nullables inside a tuple need `Value`
    /// marshaling the by-value representation deliberately avoids.
    pub tuples: Vec<TupleLayout>,
    pub span_table: SpanTable,
}

/// A tuple shape's compiled layout: element types in positional order,
/// fixed at KIR-lowering time. Unlike [`StructLayout`] this is *structural*
/// and unnamed — `(str, int)` written anywhere in the program is one
/// `TupleId` (`SPEC.md` §2.8 makes tuples structural), so there is no
/// declaration to carry a name from.
///
/// A tuple is a **by-value** LLVM aggregate: no heap allocation, no `ptr`
/// indirection, and no RC — deliberately not the container ABI
/// (`designs/llvm-compilation.md` §1.1). A `str` element is still a boxed
/// `Value*`, so copying a tuple duplicates that pointer without a retain.
/// That is consistent with the rest of the backend today — `passes::rc`'s
/// `insert_rc` is a no-op, so nothing releases either and there is no
/// double-free — and becomes real RC bookkeeping when that pass does.
#[derive(Debug, Clone)]
pub struct TupleLayout {
    pub id: TupleId,
    /// Positional order — `pair.0` indexes this directly.
    pub elems: Vec<KirType>,
}

impl TupleLayout {
    /// `true` when any element is heap-shaped, mirroring
    /// [`StructLayout::is_heap`]. Note this does *not* make the tuple itself
    /// a `ptr` the way a heap struct is — the aggregate stays by value; it
    /// only reports that it *contains* RC'd data.
    #[must_use]
    pub fn is_heap(&self, program: &KirProgram) -> bool {
        self.elems.iter().any(|ty| ty.is_heap(program))
    }
}

/// A named struct type's compiled layout: field order + `KirType` per
/// field, fixed at KIR-lowering time (`designs/llvm-compilation.md` §2.3 —
/// tag/layout values are decided here, not left to codegen).
#[derive(Debug, Clone)]
pub struct StructLayout {
    pub id: StructId,
    pub name: String,
    /// Declaration order — struct literals are matched against this by
    /// field *name* (not literal-source order, matching the checker's
    /// structural assignability rule) and rebuilt in this order.
    pub fields: Vec<(String, KirType)>,
}

impl StructLayout {
    /// Index of `name` in `fields`, if this struct has such a field.
    #[must_use]
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|(n, _)| n == name)
    }

    /// A struct needs heap allocation + RC (a `ptr` in the value ABI) if any
    /// field is itself heap-typed — recursively, so a struct-of-a-struct-
    /// with-a-string-field is heap too. Everything else (all-scalar fields)
    /// is a plain by-value LLVM aggregate, same treatment as tuples (§1.1).
    #[must_use]
    pub fn is_heap(&self, program: &KirProgram) -> bool {
        self.fields.iter().any(|(_, ty)| ty.is_heap(program))
    }
}

/// A simple enum type's compiled layout: variant names in declaration order,
/// where a variant's position *is* its runtime tag (fixed at KIR-lowering
/// time, same rationale as `StructLayout`). Values are a plain by-value
/// `i32` — no payload, no heap allocation, no RC.
#[derive(Debug, Clone)]
pub struct EnumLayout {
    pub id: EnumId,
    pub name: String,
    pub variants: Vec<String>,
}

impl EnumLayout {
    /// Index of `name` in `variants` (its runtime tag), if this enum has
    /// such a variant.
    #[must_use]
    pub fn variant_index(&self, name: &str) -> Option<usize> {
        self.variants.iter().position(|v| v == name)
    }
}

/// One lowered task (or the synthetic top-level function).
#[derive(Debug, Clone)]
pub struct KirFunction {
    pub id: FuncId,
    /// Source name, for dumps and diagnostics. `"<toplevel>"` for the
    /// synthetic entry function.
    pub name: String,
    pub params: Vec<Param>,
    pub ret: KirType,
    /// Whether this function's compiled signature returns the result-ABI
    /// wrapper (`{ i1 is_err, T success, UserRaised error }`) instead of a
    /// plain `ret`-typed value — set by the whole-program `can_raise`
    /// fixpoint (`lower/mod.rs`'s `compute_can_raise`) over `CallTarget::Fn`
    /// call sites only (`designs/llvm-compilation.md` §2.5). A function is
    /// `can_raise` iff it directly executes `Stmt::Raise`, or makes an
    /// uncaught call (not inside a matching `Stmt::TryCatch`) to another
    /// `can_raise` function. `CallTarget::Ns`/`CallTarget::Rt` calls don't
    /// participate — no M1/M2 namespace method actually produces `is_err`
    /// yet, so propagating through them would mark nearly every task
    /// `can_raise` and cascade to the toplevel entry point's fixed `-> i32`
    /// ABI; a later M2/M3 concern once a namespace method needs it.
    pub can_raise: bool,
    /// Every local this function declares, including params (params occupy
    /// the first `params.len()` slots, in order) and every `let`-introduced
    /// shadow. Declaration order = `LocalId` order.
    pub locals: Vec<Local>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub local: LocalId,
    pub ty: KirType,
}

#[derive(Debug, Clone)]
pub struct Local {
    pub id: LocalId,
    /// Source identifier, for dumps. Not unique across `locals` (shadows
    /// reuse the source name with a fresh `LocalId`).
    pub name: String,
    pub ty: KirType,
}

pub type Block = Vec<Stmt>;

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `x = expr` — always declares a fresh local (Keel assignment scoping:
    /// plain `=` shadows in the current scope). `init: None` declares the
    /// local without an initial value (`keel-codegen`'s `Stmt::Let` arm
    /// still allocates storage but skips the initial store) — used only by
    /// `when`-as-expression's let-position lowering (issue #160), where the
    /// value depends on which arm runs: a subsequent `Stmt::Assign` in each
    /// arm's branch supplies the real value before the local is ever read.
    Let {
        local: LocalId,
        init: Option<Expr>,
    },
    /// `x += expr` (and other augmented-assign ops) — desugared to a plain
    /// store against the *existing* local it resolved to; the RHS already
    /// embeds the arithmetic (`x + expr`) so this variant doesn't need its
    /// own `BinOp` field.
    Assign {
        local: LocalId,
        value: Expr,
    },
    If {
        cond: Expr,
        then_branch: Block,
        else_branch: Block,
    },
    While {
        cond: Expr,
        body: Block,
    },
    /// `for x in a..b { ... }` lowered to an indexed loop — the only `for`
    /// shape M1 lowers (see `designs/llvm-compilation.md` §2.3, §4 M1).
    /// `var` is a fresh `LocalId` of type `I64`, redeclared (per Keel's
    /// always-declares assignment scoping) and rebound each iteration;
    /// `low`/`high` are evaluated once, before the loop starts, in the
    /// enclosing scope (they cannot see `var`). Both bounds are inclusive,
    /// matching the interpreter's `Value::Range(lo, hi)` (`lo..=hi`).
    /// Non-range iterables (lists, etc.) are out of scope until the
    /// container ABI lands (M2).
    ForIndex {
        var: LocalId,
        low: Expr,
        high: Expr,
        body: Block,
    },
    /// `for x in xs { ... }` over a `list[T]` — indexed internally
    /// (`keel_list_len`/`keel_list_get`, see `keel-codegen`'s `emit_for_each`)
    /// rather than a real iterator, so this is a distinct shape from
    /// `ForIndex` (whose `var` is always `I64`, a range bound). `var` is
    /// rebound each iteration to the *unboxed* element value (`elem_ty`).
    ForEach {
        var: LocalId,
        elem_ty: KirType,
        list: Expr,
        body: Block,
    },
    /// `return expr` / bare `return`.
    Return(Option<Expr>),
    /// Expression evaluated for its side effect (e.g. a bare call).
    Expr(Expr),
    /// `raise expr` — `error` is already the constructed synthetic
    /// `UserRaised { message: str }` value (an `Expr::MakeStruct`, reusing
    /// that lowering/codegen wholesale rather than inventing a parallel
    /// error-construction path; see `lower/stmt.rs`). `expr` itself must
    /// already be `Str`-typed (the interpreter's non-`str` `Display`-
    /// coercion path is a later M2/M3 concern). Always terminates the
    /// current function with the result-ABI's error branch — the enclosing
    /// function is therefore always `can_raise`.
    Raise {
        error: Expr,
        span: SpanId,
    },
    /// `try { body } catch binder: Error|UserRaised { handler }` — only a
    /// single catch clause of type `Error` or `UserRaised` is supported
    /// (both bind the same synthetic `UserRaised { message: str }` shape,
    /// since `raise` only ever produces `UserRaised`); this collapses
    /// "caught" to "lexically inside this try's body" with no type lattice
    /// to evaluate. `binder_ty` is always `KirType::Struct(id)` for the
    /// synthetic `UserRaised` layout — carried directly (same convention as
    /// `ForEach`'s `elem_ty`) so codegen never needs a `func.locals` lookup
    /// to allocate the binder. Multiple catch clauses, and clauses over any
    /// other error type name, are rejected at lowering.
    TryCatch {
        body: Block,
        binder: LocalId,
        binder_ty: KirType,
        handler: Block,
    },
}

#[derive(Debug, Clone)]
pub enum Expr {
    ConstInt(i64),
    ConstFloat(f64),
    ConstBool(bool),
    ConstStr(String),
    Local {
        id: LocalId,
        ty: KirType,
    },
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        ty: KirType,
    },
    UnOp {
        op: UnOp,
        operand: Box<Expr>,
        ty: KirType,
    },
    /// A call to a compiled Keel function or a stdlib namespace method (see
    /// [`CallTarget`]). Value methods and indirect lambda calls are not
    /// lowered yet — see `designs/llvm-compilation.md` §2.3 `CallTarget`.
    Call {
        target: CallTarget,
        args: Vec<Expr>,
        ty: KirType,
        span: SpanId,
    },
    /// Builds a named struct value. `fields` are in the struct's declared
    /// field order (`StructLayout::fields`), already matched/reordered from
    /// the literal's (possibly different) source order and, for a spread-
    /// update, already resolved to each field's final value (overridden or
    /// copied from the base) — see `lower/expr.rs`'s struct-literal lowering
    /// doc for why this needs an expected-type context to build at all.
    MakeStruct {
        struct_id: StructId,
        fields: Vec<Expr>,
    },
    /// `base.field` on a struct-typed `base` — `field_index` is resolved at
    /// lowering time via `StructLayout::field_index`.
    FieldGet {
        base: Box<Expr>,
        field_index: usize,
        ty: KirType,
    },
    /// Builds a tuple value (`("hello", 42)`) as a by-value aggregate — one
    /// element expression per position in `TupleLayout::elems`, same order.
    /// Unlike `MakeStruct` there is no heap allocation and no field-name
    /// matching: tuple positions *are* the layout.
    MakeTuple {
        tuple_id: TupleId,
        elems: Vec<Expr>,
    },
    /// `pair.0` on a tuple-typed `base` — `index` is the positional index,
    /// bounds-checked by the type checker before lowering ever sees it
    /// (`SPEC.md` §2.8), so codegen can `extractvalue` unconditionally.
    TupleGet {
        base: Box<Expr>,
        index: usize,
        ty: KirType,
    },
    /// Builds a simple-enum value (`Priority.low`) — just its runtime tag,
    /// resolved at lowering time via `EnumLayout::variant_index`. No payload
    /// (rich variants aren't modeled yet).
    MakeEnum {
        enum_id: EnumId,
        variant_index: usize,
    },
    /// `xs[i]` on a `list[T]`-typed `xs` — bounds-checked at runtime
    /// (`keel_list_get`); `ty` is the already-unboxed element type. See
    /// `keel-rt-ffi`'s `keel_list_get` doc for the current (pre-#150)
    /// out-of-bounds behavior.
    Index {
        list: Box<Expr>,
        index: Box<Expr>,
        ty: KirType,
    },
    /// The `none` value of a nullable type (`ty` is always `KirType::
    /// Nullable(_)`) — only reachable via a `none` literal lowered against an
    /// already-known expected nullable type (`lower_expr_expecting`; `none`
    /// has no type of its own to infer bottom-up). Codegen builds the
    /// representation matching `ty`'s inner type (§1.1: null pointer for a
    /// nullable struct, a boxed `Value::None` for a nullable str/list, an
    /// `{ i1 false, T }` pair for a nullable scalar).
    NullLit {
        ty: KirType,
    },
    /// Widens a plain, known-present `inner`-typed value into `ty`'s
    /// (`KirType::Nullable(inner)`) "some" representation — the checker
    /// allows passing a non-nullable `T` wherever `T?` is expected (`SPEC.md`
    /// §4's nullable-safety rule is one-directional: unwrapping the *other*
    /// way needs `?.`/`??`/an assert). A pointer-typed inner is already the
    /// right bits (no wrapping, just a relabeled `KirType`); a scalar inner
    /// becomes an `{ i1 true, T }` pair.
    NullSome {
        value: Box<Expr>,
        ty: KirType,
    },
    /// `nullable ?? fallback` — short-circuits: `fallback` is only evaluated
    /// when `nullable` is `none` (§2.3: "`?.`/`??` → explicit branches").
    /// `ty` is `nullable`'s unwrapped inner type (== `fallback`'s type).
    NullCoalesce {
        nullable: Box<Expr>,
        fallback: Box<Expr>,
        ty: KirType,
    },
    /// `base?.field` — `base` is a nullable-struct-typed expression;
    /// short-circuits to `none` without touching the field when `base` is
    /// `none`. `ty` is `KirType::Nullable(field_ty)` (the field's own type,
    /// re-wrapped) — resolved at lowering time via `StructLayout::
    /// field_index`, same as `FieldGet`.
    NullFieldGet {
        base: Box<Expr>,
        field_index: usize,
        ty: KirType,
    },
    /// Tests whether a nullable-typed expression is `none` — `ty` is always
    /// `KirType::Bool`. Exposes `keel-codegen`'s existing `emit_is_none` as a
    /// first-class KIR expression (issue #230) so lowering can build a
    /// boolean condition for a hoisted `if`-chain — the `??`-fallback
    /// analogue of `and`/`or` using its own left operand as the condition
    /// (#228), which a nullable's plain non-boolean value can't do directly.
    IsNone {
        nullable: Box<Expr>,
        ty: KirType,
    },
    /// Unwraps a nullable-typed expression to its inner value (`ty`),
    /// assuming — by construction, not a runtime check — that it is `Some`.
    /// The `??`-fallback-hoisting desugaring's non-`none` branch (#230):
    /// only ever constructed where an `Expr::IsNone` test on the same
    /// `nullable` has already gated the branch this appears in. Using it
    /// anywhere else is a lowering bug, not a user-reachable error —
    /// `keel-codegen` emits it as a raw unwrap with no null check.
    /// `passes/verify.rs` checks that `nullable`'s type and the claimed
    /// `ty` agree, but cannot check the gating invariant itself (that would
    /// require tracing back to an enclosing branch condition) — this is a
    /// lowering-side invariant only, not something verify enforces.
    UnwrapSome {
        nullable: Box<Expr>,
        ty: KirType,
    },
}

/// What an `Expr::Call` invokes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallTarget {
    /// Direct call to another compiled Keel task.
    Fn(FuncId),
    /// Generic stdlib namespace dispatch: `io.show(...)`, `log.info(...)`.
    /// `ns_id`/`method_id` are the stable ids from
    /// `keel_catalog::specs::NAMESPACE_IDS`/`BuiltinMethod::method_id`,
    /// resolved at lowering time — `keel-codegen` (M1+) compiles this to a
    /// call into `keel_rt_call_ns(ns_id, method_id, ...)` (§2.7).
    Ns { ns_id: u16, method_id: u16 },
    /// A typed runtime-ABI call — container primitives today (§2.7: "opaque
    /// to codegen... synchronous `CallTarget::Rt` calls into the container
    /// ABI from day one"), never inline LLVM logic.
    Rt(RtFn),
    /// A value-method dispatch (`s.upper()`, `s.contains(x)`, …) — issue
    /// #214. Unlike `Ns`, there's no numeric id: value methods aren't in
    /// the `keel-catalog` namespace registry, they're a hardcoded match in
    /// `keel_runtime::interpreter::call_method_on_value`, so `method` is
    /// carried as a name and resolved by `keel_rt_call_value_method` at
    /// runtime. The associated `Expr::Call`'s `args[0]` is the receiver;
    /// `args[1..]` are the method's own arguments (`lower_str_method_call`).
    /// Only a `Str` receiver lowers today — see its doc for the closure-free
    /// method subset.
    ValueMethod { method: String },
}

/// One `keel-rt-ffi` container-ABI entry point. Each maps to exactly one
/// `#[no_mangle]` symbol in `keel-rt-ffi`'s `abi/mod.rs` — see
/// `keel-codegen`'s `rt_call.rs` for the symbol/signature each one declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtFn {
    /// `keel_list_new() -> list` — builds an empty list.
    ListNew,
    /// `keel_list_push(list, elem) -> list'` — always clones (see
    /// `keel-rt-ffi`'s doc on why this isn't real copy-on-write yet).
    ListPush,
    /// `keel_list_len(list) -> int`.
    ListLen,
    /// `keel_int_to_str(int) -> str` — one interpolation slot's to-string
    /// conversion (`designs/llvm-compilation.md` §2.3).
    IntToStr,
    /// `keel_float_to_str(float) -> str`. See [`RtFn::IntToStr`].
    FloatToStr,
    /// `keel_bool_to_str(bool) -> str`. See [`RtFn::IntToStr`].
    BoolToStr,
    /// `keel_map_new() -> map` — builds an empty `map[str, V]`.
    MapNew,
    /// `keel_map_insert(map, key, val) -> map'` — always clones, same
    /// convention as [`RtFn::ListPush`]. `key` is always `str`-typed
    /// (`map[str, V]` only — see `KirType::Map`'s doc).
    MapInsert,
    /// `keel_map_get(map, key) -> V?` — returns a boxed `none` on a missing
    /// key rather than exiting (unlike `Expr::Index`'s list-indexing
    /// out-of-bounds exit, `keel-rt-ffi`'s `keel_list_get`): a missing key is
    /// an ordinary, checker-required-nullable outcome the interpreter's own
    /// `map.get` also just returns `none` for.
    MapGet,
    /// `keel_map_len(map) -> int`.
    MapLen,
    /// `keel_map_contains(map, key) -> bool`.
    MapContains,
    /// `keel_map_keys(map) -> list[str]`, sorted (see `keel-rt-ffi`'s doc on
    /// why: `HashMap` iteration order isn't deterministic).
    MapKeys,
    /// `keel_map_values(map) -> list[V]`, ordered by sorted key. See
    /// [`RtFn::MapKeys`].
    MapValues,
    /// `keel_set_new() -> set` — builds an empty `set[T]`.
    SetNew,
    /// `keel_set_insert(set, elem) -> set'` — adds `elem` unless an equal
    /// element is already present, always cloning (same convention as
    /// [`RtFn::ListPush`]). Backs both the `set[...]` literal fold and the
    /// `.add(v)` method, which are the same operation.
    SetInsert,
    /// `keel_set_len(set) -> int` — post-dedup count.
    SetLen,
    /// `keel_set_contains(set, elem) -> bool` — membership by the runtime's
    /// value equality, the same rule [`RtFn::SetInsert`] dedups on.
    SetContains,
}

impl Expr {
    /// The `KirType` this expression evaluates to.
    #[must_use]
    pub fn ty(&self) -> KirType {
        match self {
            Expr::ConstInt(_) => KirType::I64,
            Expr::ConstFloat(_) => KirType::F64,
            Expr::ConstBool(_) => KirType::Bool,
            Expr::ConstStr(_) => KirType::Str,
            Expr::Local { ty, .. }
            | Expr::BinOp { ty, .. }
            | Expr::UnOp { ty, .. }
            | Expr::Call { ty, .. }
            | Expr::FieldGet { ty, .. } => *ty,
            Expr::MakeStruct { struct_id, .. } => KirType::Struct(*struct_id),
            Expr::MakeTuple { tuple_id, .. } => KirType::Tuple(*tuple_id),
            Expr::TupleGet { ty, .. } => *ty,
            Expr::MakeEnum { enum_id, .. } => KirType::Enum(*enum_id),
            Expr::Index { ty, .. }
            | Expr::NullLit { ty }
            | Expr::NullSome { ty, .. }
            | Expr::NullCoalesce { ty, .. }
            | Expr::NullFieldGet { ty, .. }
            | Expr::IsNone { ty, .. }
            | Expr::UnwrapSome { ty, .. } => *ty,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}
