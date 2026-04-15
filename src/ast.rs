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
    Connect(ConnectDecl),
    Task(TaskDecl),
    Agent(AgentDecl),
    Run(RunStmt),
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
}

// ---------------------------------------------------------------------------
// Connect declaration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ConnectDecl {
    pub name: String,
    pub protocol: String,
    pub config: Vec<(String, Expr)>,
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
    Role(String),
    Model(String),
    Tools(Vec<String>),
    Memory(MemoryMode),
    State(Vec<StateField>),
    Config(Vec<(String, Expr)>),
    Task(TaskDecl),
    Every(EveryBlock),
    On(OnHandler),
    Team(Vec<String>),
    Rules(Vec<Expr>),
}

#[derive(Debug, Clone)]
pub enum MemoryMode {
    None_,
    Session,
    Persistent,
}

#[derive(Debug, Clone)]
pub struct StateField {
    pub name: String,
    pub ty: TypeExpr,
    pub default: Expr,
}

#[derive(Debug, Clone)]
pub struct EveryBlock {
    pub interval: Expr,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct OnHandler {
    pub event: String,
    pub param: Option<Param>,
    pub body: Block,
}

// ---------------------------------------------------------------------------
// Run statement
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RunStmt {
    pub agent: String,
    pub background: bool,
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
    /// Expression used as a statement
    Expr(Expr),
    /// `return expr`
    Return(Option<Expr>),
    /// `for x in expr { ... }` or `for x in expr where pred { ... }`
    For {
        binding: String,
        iter: Expr,
        filter: Option<Expr>,
        body: Block,
    },
    /// `if cond { ... } else { ... }`
    If {
        cond: Expr,
        then_body: Block,
        else_body: Option<Block>,
    },
    /// `when expr { arms }`
    When {
        subject: Expr,
        arms: Vec<WhenArm>,
    },
    /// `try { ... } catch err: Type { ... }`
    TryCatch {
        body: Block,
        catches: Vec<CatchClause>,
    },
    /// `notify user expr`
    Notify { message: Expr },
    /// `show user expr`
    Show { value: Expr },
    /// `send expr to target`
    Send { value: Expr, target: Expr },
    /// `archive expr`
    Archive { value: Expr },
    /// `confirm user expr then stmt`
    ConfirmThen {
        message: Expr,
        then_body: Block,
    },
    /// `remember expr`
    Remember { value: Expr },
    /// `after duration { body }`
    After { delay: Expr, body: Block },
    /// `retry N times [with backoff] { body }`
    Retry {
        count: Expr,
        backoff: bool,
        body: Block,
    },
    /// `wait duration` or `wait until condition`
    Wait { duration: Option<Expr>, condition: Option<Expr> },
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
    /// Identifier: matches an enum variant by name or binds a variable
    Ident(String),
    /// Wildcard: `_`
    Wildcard,
    /// Literal value
    Literal(Expr),
    /// Rich enum variant: `reply { to, tone }`
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
    /// String with possible interpolation segments
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
    /// `env.VAR`
    EnvAccess(String),
    /// `self.field`
    SelfAccess(String),

    // ── Compound literals ────────────────────────────────────────────
    /// `{key: value, ...}`
    StructLit(Vec<(String, Expr)>),
    /// `[expr, ...]`
    ListLit(Vec<Expr>),

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
    /// `func(args)` or `func(name: value)`
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
    },
    /// `expr.method(args)`
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<CallArg>,
    },

    // ── AI primitives ────────────────────────────────────────────────
    Classify {
        input: Box<Expr>,
        target: ClassifyTarget,
        criteria: Option<Vec<(String, String)>>,
        fallback: Option<Box<Expr>>,
        model: Option<String>,
    },
    Summarize {
        input: Box<Expr>,
        length: Option<(i64, String)>,
        format: Option<String>,
        fallback: Option<Box<Expr>>,
        model: Option<String>,
    },
    Draft {
        description: Box<Expr>,
        options: Vec<(String, Expr)>,
        model: Option<String>,
    },
    Extract {
        schema: Vec<Field>,
        source: Box<Expr>,
        model: Option<String>,
    },
    Translate {
        input: Box<Expr>,
        target_lang: Vec<String>,
        model: Option<String>,
    },
    Decide {
        input: Box<Expr>,
        options: Vec<(String, Expr)>,
        model: Option<String>,
    },

    /// `prompt { system: "...", user: "..." } as Type`
    Prompt {
        config: Vec<(String, Expr)>,
        target_type: String,
    },

    // ── Human interaction ────────────────────────────────────────────
    /// `ask user "prompt"` or `ask user "prompt" options [...]`
    Ask {
        prompt: Box<Expr>,
        options: Option<Box<Expr>>,
    },
    /// `confirm user expr` (expression form returning bool)
    Confirm { message: Box<Expr> },

    // ── Data access ──────────────────────────────────────────────────
    /// `fetch "url"` or `fetch source where pred`
    Fetch {
        source: Box<Expr>,
        filter: Option<Box<Expr>>,
    },

    // ── Memory ───────────────────────────────────────────────────────
    /// `recall "query"` with optional `limit N`
    Recall {
        query: Box<Expr>,
        limit: Option<i64>,
    },

    // ── Delegate ─────────────────────────────────────────────────────
    /// `delegate task(args) to Agent`
    Delegate {
        task_call: Box<Expr>,
        agent: String,
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
    /// `(params) => expr` or `x => expr`
    Lambda {
        params: Vec<LambdaParam>,
        body: Box<Expr>,
    },

    // ── Duration ─────────────────────────────────────────────────────
    /// `5.minutes`, `2.hours` etc.
    Duration {
        value: Box<Expr>,
        unit: DurationUnit,
    },

    // ── Enum variant ─────────────────────────────────────────────────
    /// `Type.variant`
    EnumVariant(String, String),
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
pub enum ClassifyTarget {
    /// Named type: `as Urgency`
    Named(String),
    /// Inline options: `as [low, medium, high]`
    Inline(Vec<String>),
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
}
