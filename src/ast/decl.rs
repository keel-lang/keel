//! Declaration nodes.

use crate::lexer::Span;

use super::{Binding, Block, Node, SpannedExpr, Stmt, TypeDef, TypeExpr};

#[derive(Debug, Clone)]
pub enum Decl {
    Type(TypeDecl),
    Interface(InterfaceDecl),
    Impl(ImplDecl),
    Task(TaskDecl),
    Extern(ExternDecl),
    Agent(AgentDecl),
    Use(UseDecl),
    /// Top-level statement, e.g. `run(MyAgent)` at the end of a file.
    Stmt(Node<Stmt>),
}

#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    /// Byte span of the declaration name token, e.g. the `Color` in `type Color = ...`.
    pub name_span: Span,
    /// Type parameter names for generic types, e.g. `["T"]` for `type Paginated[T]`.
    pub type_params: Vec<String>,
    pub def: TypeDef,
}

#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub name: String,
    /// Byte span of the declaration name token.
    pub name_span: Span,
    pub methods: Vec<TaskSig>,
}

/// `impl InterfaceName for TypeName { task method(self) -> ReturnType { ... } }`
#[derive(Debug, Clone)]
pub struct ImplDecl {
    pub interface_name: String,
    pub type_name: String,
    pub methods: Vec<TaskDecl>,
}

/// `task name(params) -> ReturnType` — method signature inside an interface.
#[derive(Debug, Clone)]
pub struct TaskSig {
    pub name: String,
    /// Byte span of the method name token.
    pub name_span: Span,
    pub params: Vec<Param>,
    /// Return-type annotation together with its source span, if present.
    pub return_type: Option<Node<TypeExpr>>,
}

#[derive(Debug, Clone)]
pub struct ExternDecl {
    pub name: String,
    /// Byte span of the declaration name token.
    pub name_span: Span,
    pub params: Vec<Param>,
    /// Return-type annotation together with its source span.
    pub return_type: Node<TypeExpr>,
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
    /// Byte span of the declaration name token, e.g. the `greet` in `task greet(...)`.
    pub name_span: Span,
    /// Type parameter names for generic tasks, e.g. `["T"]` for `task f[T](x: T)`.
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    /// Return-type annotation together with its source span, if present.
    pub return_type: Option<Node<TypeExpr>>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: Binding,
    /// Byte span of the parameter name token (or the full destructure pattern for `{a, b}` params).
    pub name_span: Span,
    /// Type annotation together with its source span.
    pub ty: Node<TypeExpr>,
    pub default: Option<SpannedExpr>,
    /// If true, this is a variadic rest-parameter (`...name: T`).
    /// Inside the task body it binds as `list[T]`.
    /// Must be the last positional parameter.
    pub variadic: bool,
}

#[derive(Debug, Clone)]
pub struct AgentDecl {
    pub name: String,
    /// Byte span of the declaration name token.
    pub name_span: Span,
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
    Expr(SpannedExpr),
    /// `@on_start { ... }` — block of statements executed in the agent context.
    Block(Block),
    /// `@tools [Email, Email.send if self.confirmed, Http]`
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
    pub condition: Option<SpannedExpr>,
}

/// Names of attributes whose body is a block of statements (not an expression).
/// All other attributes parse their body as an expression.
pub const BLOCK_BODY_ATTRIBUTES: &[&str] = &["on_start", "on_stop"];

#[derive(Debug, Clone)]
pub struct StateField {
    pub name: String,
    /// Byte span of the state-field name token.
    pub name_span: Span,
    /// Type annotation together with its source span.
    pub ty: Node<TypeExpr>,
    pub default: SpannedExpr,
    /// If true, assignment via `self.field = ...` is a compile-time error.
    pub readonly: bool,
}

#[derive(Debug, Clone)]
pub struct OnHandler {
    pub event: String,
    pub param: Option<Param>,
    pub body: Block,
}
