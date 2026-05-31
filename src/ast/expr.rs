//! Expression nodes and expression subtypes.

use super::{Block, Node, TypeExpr};
use crate::lexer::Span;

/// A key in a struct/map literal, carrying the original syntactic form.
#[derive(Debug, Clone, PartialEq)]
pub enum MapLitKey {
    /// Bareword key: `{foo: 1}`.
    Ident(String),
    /// Quoted string key: `{"foo": 1}`.
    Str(String),
    /// Integer key: `{1: "one"}`.
    Int(i64),
    /// Boolean key: `{true: "on"}`.
    Bool(bool),
}

impl MapLitKey {
    /// Returns the string value for `Ident` and `Str` keys; `None` for `Int`/`Bool`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            MapLitKey::Ident(s) | MapLitKey::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Display string for error messages and formatting.
    pub fn display(&self) -> String {
        match self {
            MapLitKey::Ident(s) | MapLitKey::Str(s) => s.clone(),
            MapLitKey::Int(n) => n.to_string(),
            MapLitKey::Bool(b) => b.to_string(),
        }
    }
}

/// A spanned expression: the expression node paired with its source byte range.
///
/// Every expression produced by the parser carries a span so that type-checker
/// diagnostics and IDE features can point at the precise sub-expression.
pub type SpannedExpr = Node<Expr>;

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
    FieldAccess(Box<SpannedExpr>, String),
    /// `expr?.field`
    NullFieldAccess(Box<SpannedExpr>, String),
    /// `expr!`
    NullAssert(Box<SpannedExpr>),
    /// `self.field`
    SelfAccess {
        field: String,
        field_span: Span,
    },
    /// bare `self` — resolves to an AgentRef for the current agent
    SelfRef,

    // ── Compound literals ────────────────────────────────────────────
    /// `{key: value, ...}` — keys carry their syntactic form via `MapLitKey`.
    StructLit(Vec<(MapLitKey, SpannedExpr)>),
    /// `{ ...base, field: val, ... }` — copy all fields from base, override specified ones.
    StructSpreadUpdate {
        base: Box<SpannedExpr>,
        overrides: Vec<(String, SpannedExpr)>,
    },
    /// `[expr, ...]`
    ListLit(Vec<SpannedExpr>),
    /// `set[expr, ...]`
    SetLit(Vec<SpannedExpr>),
    /// `(expr, expr, ...)` — tuple with 2+ elements
    TupleLit(Vec<SpannedExpr>),

    // ── Operators ────────────────────────────────────────────────────
    BinaryOp {
        left: Box<SpannedExpr>,
        op: BinOp,
        right: Box<SpannedExpr>,
    },
    UnaryOp {
        op: UnOp,
        expr: Box<SpannedExpr>,
    },
    /// `expr ?? default`
    NullCoalesce(Box<SpannedExpr>, Box<SpannedExpr>),
    /// `expr |> func`
    Pipeline(Box<SpannedExpr>, Box<SpannedExpr>),
    /// `start..end` — inclusive integer range, evaluates to `list[int]`.
    Range(Box<SpannedExpr>, Box<SpannedExpr>),

    // ── Calls ────────────────────────────────────────────────────────
    /// `func(args)` or `func(name: value)` — also covers
    /// `Ai.classify(...)` after method-call desugaring below.
    Call {
        callee: Box<SpannedExpr>,
        args: Vec<CallArg>,
    },
    /// `expr.method(args)` — keeps the method name available for lookup.
    MethodCall {
        object: Box<SpannedExpr>,
        method: String,
        args: Vec<CallArg>,
    },

    // ── Cast ─────────────────────────────────────────────────────────
    /// `expr as Type` — used with `Ai.prompt(...)` and `dynamic` narrowing.
    Cast {
        expr: Box<SpannedExpr>,
        /// Target type annotation together with its source span.
        ty: Node<TypeExpr>,
    },

    // ── Control flow as expressions ──────────────────────────────────
    /// `if cond { ... } else { ... }` (expression form)
    IfExpr {
        cond: Box<SpannedExpr>,
        then_body: Block,
        else_body: Block,
    },
    /// `when subject { pattern => expr ... }` (expression form — evaluates to the matched arm's value)
    WhenExpr {
        subject: Box<SpannedExpr>,
        arms: Vec<super::WhenArm>,
    },
    // ── Lambda ───────────────────────────────────────────────────────
    /// `(params) => expr` or `x => expr` or `(params) => { block }`
    Lambda {
        params: Vec<LambdaParam>,
        body: LambdaBody,
    },

    // ── Subscript ────────────────────────────────────────────────────
    /// `expr[index]` — list element access (returns `T`) or string char
    /// access (returns `str`). Out-of-bounds is a runtime error.
    Index {
        object: Box<SpannedExpr>,
        index: Box<SpannedExpr>,
    },

    // ── Duration ─────────────────────────────────────────────────────
    /// `5.minutes`, `2.hours` — parsed at postfix `INT "." Ident(unit)`.
    Duration {
        value: Box<SpannedExpr>,
        unit: DurationUnit,
    },

    // ── Enum variant ─────────────────────────────────────────────────
    /// `Urgency.high` (simple) or `Action.reply { to: "...", tone: "..." }`
    /// (rich). `fields` is empty for simple variants.
    EnumVariant {
        ty: String,
        variant: String,
        fields: Vec<(String, SpannedExpr)>,
    },
}

#[derive(Debug, Clone)]
pub enum StringPart {
    Literal(String),
    /// Expression plus an optional raw format spec (the part after `:` inside `{}`).
    /// e.g. `{pi:.2f}` → spec = `Some(".2f")`, `{x}` → spec = `None`.
    Interpolation(Box<SpannedExpr>, Option<String>),
    /// The interpolation slot could not be parsed.
    ///
    /// Carries the raw slot text so the formatter can round-trip broken
    /// source without data loss.  The type checker reports a diagnostic
    /// whenever this variant appears.
    ParseError(String),
}

#[derive(Debug, Clone)]
pub struct CallArg {
    pub name: Option<String>,
    pub value: SpannedExpr,
    /// If true, `value` is expanded (spread) into individual variadic slots.
    pub spread: bool,
}

#[derive(Debug, Clone)]
pub struct LambdaParam {
    pub name: String,
    /// Optional type annotation together with its source span.
    pub ty: Option<Node<TypeExpr>>,
}

#[derive(Debug, Clone)]
pub enum LambdaBody {
    Expr(Box<SpannedExpr>),
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
