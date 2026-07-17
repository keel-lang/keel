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
    pub span_table: SpanTable,
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
    /// Direct call to a compiled Keel function — the only `CallTarget` M0
    /// lowers to (no namespace dispatch, no value methods, no indirect
    /// lambda calls yet; see `designs/llvm-compilation.md` §2.3
    /// `CallTarget`).
    Call {
        target: FuncId,
        args: Vec<Expr>,
        ty: KirType,
        span: SpanId,
    },
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
            | Expr::Call { ty, .. } => *ty,
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
