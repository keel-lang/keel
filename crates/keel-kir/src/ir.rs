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
    pub span_table: SpanTable,
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
    /// plain `=` shadows in the current scope).
    Let {
        local: LocalId,
        init: Expr,
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
    /// `return expr` / bare `return`.
    Return(Option<Expr>),
    /// Expression evaluated for its side effect (e.g. a bare call).
    Expr(Expr),
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
    /// Builds a simple-enum value (`Priority.low`) — just its runtime tag,
    /// resolved at lowering time via `EnumLayout::variant_index`. No payload
    /// (rich variants aren't modeled yet).
    MakeEnum {
        enum_id: EnumId,
        variant_index: usize,
    },
}

/// What an `Expr::Call` invokes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallTarget {
    /// Direct call to another compiled Keel task.
    Fn(FuncId),
    /// Generic stdlib namespace dispatch: `io.show(...)`, `log.info(...)`.
    /// `ns_id`/`method_id` are the stable ids from
    /// `keel_catalog::specs::NAMESPACE_IDS`/`BuiltinMethod::method_id`,
    /// resolved at lowering time — `keel-codegen` (M1+) compiles this to a
    /// call into `keel_rt_call_ns(ns_id, method_id, ...)` (§2.7).
    Ns { ns_id: u16, method_id: u16 },
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
            Expr::MakeEnum { enum_id, .. } => KirType::Enum(*enum_id),
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
