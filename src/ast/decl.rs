//! Declaration nodes.

use super::{Binding, Block, Expr, Spanned, Stmt, TypeDef, TypeExpr};

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

#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    /// Type parameter names for generic types, e.g. `["T"]` for `type Paginated[T]`.
    pub type_params: Vec<String>,
    pub def: TypeDef,
}

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

#[derive(Debug, Clone)]
pub struct ExternDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub source: String,
}

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

#[derive(Debug, Clone)]
pub struct TaskDecl {
    pub name: String,
    /// Type parameter names for generic tasks, e.g. `["T"]` for `task f[T](x: T)`.
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: Binding,
    pub ty: TypeExpr,
    pub default: Option<Expr>,
}

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
    /// `@role "..."`, `@memory persistent`, `@limits { ... }`, etc.
    Expr(Expr),
    /// `@on_start { ... }` — block of statements executed in the agent context.
    Block(Block),
    /// `@tools [Email, Email.send when self.confirmed, Http]`
    Tools(Vec<ToolEntry>),
}

/// One entry inside `@tools [...]`.
#[derive(Debug, Clone)]
pub struct ToolEntry {
    /// Namespace name, e.g. `"Email"`.
    pub namespace: String,
    /// Optional method name. `None` = gate the whole namespace.
    pub method: Option<String>,
    /// Optional guard expression. `None` = always allowed.
    pub condition: Option<Expr>,
}

/// Names of attributes whose body is a block of statements (not an expression).
/// All other attributes parse their body as an expression.
pub const BLOCK_BODY_ATTRIBUTES: &[&str] = &["on_start", "on_stop"];

#[derive(Debug, Clone)]
pub struct StateField {
    pub name: String,
    pub ty: TypeExpr,
    pub default: Expr,
    /// If true, assignment via `self.field = ...` is a compile-time error.
    pub readonly: bool,
}

#[derive(Debug, Clone)]
pub struct OnHandler {
    pub event: String,
    pub param: Option<Param>,
    pub body: Block,
}
