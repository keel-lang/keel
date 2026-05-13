//! Expression nodes and expression subtypes.

use super::{Block, TypeExpr};

#[derive(Debug, Clone)]
pub enum Expr {
    // ── Literals ─────────────────────────────────────────────────────
    Integer(i64),
    Float(f64),
    /// String with possible interpolation segments.
    StringLit(Vec<StringPart>),
    Bool(bool),
    None_,

    // ── Identifiers & access ─────────────────────────────────────────
    Ident(String),
    /// `expr.field`
    FieldAccess(Box<Expr>, String),
    /// `expr?.field`
    NullFieldAccess(Box<Expr>, String),
    /// `expr!`
    NullAssert(Box<Expr>),
    /// `self.field`
    SelfAccess(String),
    /// bare `self` — resolves to an AgentRef for the current agent
    SelfRef,

    // ── Compound literals ────────────────────────────────────────────
    /// `{key: value, ...}`
    StructLit(Vec<(String, Expr)>),
    /// `[expr, ...]`
    ListLit(Vec<Expr>),
    /// `set[expr, ...]`
    SetLit(Vec<Expr>),
    /// `(expr, expr, ...)` — tuple with 2+ elements
    TupleLit(Vec<Expr>),

    // ── Operators ────────────────────────────────────────────────────
    BinaryOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnOp,
        expr: Box<Expr>,
    },
    /// `expr ?? default`
    NullCoalesce(Box<Expr>, Box<Expr>),
    /// `expr |> func`
    Pipeline(Box<Expr>, Box<Expr>),
    /// `start..end` — inclusive integer range, evaluates to `list[int]`.
    Range(Box<Expr>, Box<Expr>),

    // ── Calls ────────────────────────────────────────────────────────
    /// `func(args)` or `func(name: value)` — also covers
    /// `Ai.classify(...)` after method-call desugaring below.
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
    },
    /// `expr.method(args)` — keeps the method name available for lookup.
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<CallArg>,
    },

    // ── Cast ─────────────────────────────────────────────────────────
    /// `expr as Type` — used with `Ai.prompt(...)` and `dynamic` narrowing.
    Cast {
        expr: Box<Expr>,
        ty: TypeExpr,
    },

    // ── Control flow as expressions ──────────────────────────────────
    /// `if cond { ... } else { ... }` (expression form)
    IfExpr {
        cond: Box<Expr>,
        then_body: Block,
        else_body: Block,
    },
    // ── Lambda ───────────────────────────────────────────────────────
    /// `(params) => expr` or `x => expr` or `(params) => { block }`
    Lambda {
        params: Vec<LambdaParam>,
        body: LambdaBody,
    },

    // ── Duration ─────────────────────────────────────────────────────
    /// `5.minutes`, `2.hours` — parsed at postfix `INT "." Ident(unit)`.
    Duration {
        value: Box<Expr>,
        unit: DurationUnit,
    },

    // ── Enum variant ─────────────────────────────────────────────────
    /// `Urgency.high` (simple) or `Action.reply { to: "...", tone: "..." }`
    /// (rich). `fields` is empty for simple variants.
    EnumVariant {
        ty: String,
        variant: String,
        fields: Vec<(String, Expr)>,
    },
}

#[derive(Debug, Clone)]
pub enum StringPart {
    Literal(String),
    Interpolation(Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct CallArg {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct LambdaParam {
    pub name: String,
    pub ty: Option<TypeExpr>,
}

#[derive(Debug, Clone)]
pub enum LambdaBody {
    Expr(Box<Expr>),
    Block(Block),
}

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone, Copy)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy)]
pub enum DurationUnit {
    Milliseconds,
    Seconds,
    Minutes,
    Hours,
    Days,
    Weeks,
}

impl DurationUnit {
    /// Canonical lower-case unit name for error messages and the formatter.
    pub fn canonical_name(self) -> &'static str {
        match self {
            DurationUnit::Milliseconds => "ms",
            DurationUnit::Seconds => "seconds",
            DurationUnit::Minutes => "minutes",
            DurationUnit::Hours => "hours",
            DurationUnit::Days => "days",
            DurationUnit::Weeks => "weeks",
        }
    }
}
