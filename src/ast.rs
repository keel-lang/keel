use crate::lexer::Span;

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Program {
    pub declarations: Vec<Spanned<Decl>>,
}

pub type Spanned<T> = (T, Span);

// ---------------------------------------------------------------------------
// Top-level declarations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Decl {
    Type(TypeDecl),
    Interface(InterfaceDecl),
    Task(TaskDecl),
    Extern(ExternDecl),
    Agent(AgentDecl),
    Use(UseDecl),
    /// Top-level statement, e.g. `run(MyAgent)` at the end of a file.
    Stmt(Spanned<Stmt>),
}

// ---------------------------------------------------------------------------
// Type declarations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    pub def: TypeDef,
}

#[derive(Debug, Clone)]
pub enum TypeDef {
    /// `type Urgency = low | medium | high | critical`
    SimpleEnum(Vec<String>),
    /// `type Action = | reply { to: str } | archive`
    RichEnum(Vec<EnumVariant>),
    /// `type EmailInfo { sender: str, subject: str }`
    Struct(Vec<Field>),
    /// `type Timestamp = datetime`
    Alias(TypeExpr),
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Option<Vec<Field>>,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: TypeExpr,
}

// ---------------------------------------------------------------------------
// Interface declaration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub name: String,
    pub methods: Vec<TaskSig>,
}

/// `task name(params) -> ReturnType` — method signature inside an interface.
#[derive(Debug, Clone)]
pub struct TaskSig {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
}

// ---------------------------------------------------------------------------
// Extern declaration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ExternDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub source: String,
}

// ---------------------------------------------------------------------------
// Use declaration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UseDecl {
    pub kind: UseKind,
}

#[derive(Debug, Clone)]
pub enum UseKind {
    /// `use "./path.keel"`
    File(String),
    /// `use Symbol from "./path.keel"`
    Symbol { name: String, source: String },
    /// `use keel/slack` — package path
    Package(Vec<String>),
}

// ---------------------------------------------------------------------------
// Type expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum TypeExpr {
    /// Named type: `str`, `int`, `Urgency`
    Named(String),
    /// Nullable: `str?`
    Nullable(Box<TypeExpr>),
    /// List: `list[str]`
    List(Box<TypeExpr>),
    /// Map: `map[str, int]`
    Map(Box<TypeExpr>, Box<TypeExpr>),
    /// Set: `set[int]`
    Set(Box<TypeExpr>),
    /// Inline struct: `{body: str, from: str}`
    Struct(Vec<Field>),
    /// Tuple: `(str, int)`
    Tuple(Vec<TypeExpr>),
    /// Function type: `(str) -> bool`
    Func(Vec<TypeExpr>, Box<TypeExpr>),
    /// Generic application: `Result[T, E]`
    Generic(String, Vec<TypeExpr>),
    /// Dynamic (FFI escape hatch)
    Dynamic,
}

// ---------------------------------------------------------------------------
// Task declaration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TaskDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub default: Option<Expr>,
}

pub type Block = Vec<Spanned<Stmt>>;

// ---------------------------------------------------------------------------
// Agent declaration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AgentDecl {
    pub name: String,
    pub items: Vec<AgentItem>,
}

#[derive(Debug, Clone)]
pub enum AgentItem {
    Attribute(AttributeDecl),
    State(Vec<StateField>),
    Task(TaskDecl),
    On(OnHandler),
}

/// `@name <body>` — attribute clause inside an agent body.
///
/// Only `@role` and `@model` are core-defined. Every other attribute is
/// interpreted by a stdlib-registered handler.
#[derive(Debug, Clone)]
pub struct AttributeDecl {
    pub name: String,
    pub body: AttributeBody,
}

#[derive(Debug, Clone)]
pub enum AttributeBody {
    /// `@role "..."`, `@tools [...]`, `@memory persistent`, `@limits { ... }`, etc.
    Expr(Expr),
    /// `@on_start { ... }` — block of statements executed in the agent context.
    Block(Block),
}

/// Names of attributes whose body is a block of statements (not an expression).
/// All other attributes parse their body as an expression.
pub const BLOCK_BODY_ATTRIBUTES: &[&str] = &["on_start", "on_stop"];

#[derive(Debug, Clone)]
pub struct StateField {
    pub name: String,
    pub ty: TypeExpr,
    pub default: Expr,
}

#[derive(Debug, Clone)]
pub struct OnHandler {
    pub event: String,
    pub param: Option<Param>,
    pub body: Block,
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `x = expr` or `x: Type = expr`
    Let {
        name: String,
        ty: Option<TypeExpr>,
        value: Expr,
    },
    /// `self.field = expr`
    SelfAssign { field: String, value: Expr },
    /// `return expr`
    Return(Option<Expr>),
    /// `for x in expr { ... }` or `for x in expr where pred { ... }`
    For {
        binding: String,
        iter: Expr,
        filter: Option<Expr>,
        body: Block,
    },
    /// `if cond { ... } else { ... }` — statement form, used when the value
    /// isn't consumed (for branching side effects).
    If {
        cond: Expr,
        then_body: Block,
        else_body: Option<Block>,
    },
    /// `when expr { arms }` — statement form.
    When {
        subject: Expr,
        arms: Vec<WhenArm>,
    },
    /// `try { ... } catch err: Type { ... }`
    TryCatch {
        body: Block,
        catches: Vec<CatchClause>,
    },
    /// Expression used as a statement — covers `Io.notify(...)`,
    /// `Email.send(...)`, `run(MyAgent)`, bare calls, etc.
    Expr(Expr),
}

// ---------------------------------------------------------------------------
// When (pattern matching)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WhenArm {
    pub patterns: Vec<Pattern>,
    pub guard: Option<Expr>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    /// Identifier: matches an enum variant by name or binds a variable.
    Ident(String),
    /// Wildcard: `_`
    Wildcard,
    /// Literal value.
    Literal(Expr),
    /// Rich enum variant destructure: `reply { to, tone }`.
    Variant {
        name: String,
        bindings: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// Catch clause
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CatchClause {
    pub name: String,
    pub ty: TypeExpr,
    pub body: Block,
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Expr {
    // ── Literals ─────────────────────────────────────────────────────
    Integer(i64),
    Float(f64),
    /// String with possible interpolation segments.
    StringLit(Vec<StringPart>),
    Bool(bool),
    None_,
    Now,

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
    /// `when expr { arms }` (expression form)
    WhenExpr {
        subject: Box<Expr>,
        arms: Vec<WhenArm>,
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

// ---------------------------------------------------------------------------
// Sub-types
// ---------------------------------------------------------------------------

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
            DurationUnit::Seconds => "seconds",
            DurationUnit::Minutes => "minutes",
            DurationUnit::Hours => "hours",
            DurationUnit::Days => "days",
            DurationUnit::Weeks => "weeks",
        }
    }
}
