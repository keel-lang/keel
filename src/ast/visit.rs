//! Read-only AST traversal utilities.
//!
//! This module provides a Rust-style visitor for analysis passes. Each
//! `visit_*` method has a default implementation that delegates to a matching
//! `walk_*` function, so visitors can override only the nodes they care about.

use crate::ast::*;
use crate::lexer::Span;

/// Visits Keel AST nodes without mutating them.
///
/// Override a method to observe a node, then call the corresponding `walk_*`
/// function when traversal should continue into that node's children.
pub trait Visitor {
    /// Visits a complete program.
    fn visit_program(&mut self, program: &Program) {
        walk_program(self, program);
    }

    /// Visits a top-level declaration and its span.
    fn visit_decl(&mut self, decl: &Decl, span: &Span) {
        walk_decl(self, decl, span);
    }

    /// Visits an agent body item.
    fn visit_agent_item(&mut self, item: &AgentItem) {
        walk_agent_item(self, item);
    }

    /// Visits an attribute body.
    fn visit_attribute_body(&mut self, body: &AttributeBody) {
        walk_attribute_body(self, body);
    }

    /// Visits a block of statements.
    fn visit_block(&mut self, block: &Block) {
        walk_block(self, block);
    }

    /// Visits a statement and its span.
    fn visit_stmt(&mut self, stmt: &Stmt, span: &Span) {
        walk_stmt(self, stmt, span);
    }

    /// Visits a spanned expression (the expression node together with its source span).
    fn visit_expr(&mut self, expr: &SpannedExpr) {
        walk_expr(self, expr);
    }

    /// Visits a `when` pattern.
    fn visit_pattern(&mut self, pattern: &Pattern) {
        walk_pattern(self, pattern);
    }

    /// Visits a type expression.
    fn visit_type_expr(&mut self, ty: &TypeExpr) {
        walk_type_expr(self, ty);
    }
}

/// Walks every declaration in a program.
pub fn walk_program<V: Visitor + ?Sized>(v: &mut V, program: &Program) {
    for node in &program.declarations {
        v.visit_decl(&node.kind, &node.span);
    }
}

/// Walks a top-level declaration's children.
pub fn walk_decl<V: Visitor + ?Sized>(v: &mut V, decl: &Decl, _span: &Span) {
    match decl {
        Decl::Type(type_decl) => walk_type_decl(v, type_decl),
        Decl::Interface(interface_decl) => walk_interface_decl(v, interface_decl),
        Decl::Impl(impl_decl) => {
            for method in &impl_decl.methods {
                walk_task_decl(v, method);
            }
        }
        Decl::Task(task_decl) => walk_task_decl(v, task_decl),
        Decl::Test(test_decl) => walk_test_decl(v, test_decl),
        Decl::Extern(extern_decl) => walk_extern_decl(v, extern_decl),
        Decl::Agent(agent_decl) => walk_agent_decl(v, agent_decl),
        Decl::Use(UseDecl { kind }) => match kind {
            UseKind::File(_) | UseKind::Symbol { .. } | UseKind::Package(_) => {}
        },
        Decl::Stmt(node) => v.visit_stmt(&node.kind, &node.span),
    }
}

/// Walks an agent body item.
pub fn walk_agent_item<V: Visitor + ?Sized>(v: &mut V, item: &AgentItem) {
    match item {
        AgentItem::Attribute(attribute) => v.visit_attribute_body(&attribute.body),
        AgentItem::State(fields) => {
            for field in fields {
                v.visit_type_expr(&field.ty.kind);
                v.visit_expr(&field.default);
            }
        }
        AgentItem::Task(task) => walk_task_decl(v, task),
        AgentItem::On(handler) => {
            if let Some(param) = &handler.param {
                walk_param(v, param);
            }
            v.visit_block(&handler.body);
        }
    }
}

/// Walks an attribute body's children.
pub fn walk_attribute_body<V: Visitor + ?Sized>(v: &mut V, body: &AttributeBody) {
    match body {
        AttributeBody::Expr(expr) => v.visit_expr(expr),
        AttributeBody::Block(block) => v.visit_block(block),
        AttributeBody::Tools(entries) => {
            for entry in entries {
                if let Some(condition) = &entry.condition {
                    v.visit_expr(condition);
                }
            }
        }
    }
}

/// Walks each statement in a block.
pub fn walk_block<V: Visitor + ?Sized>(v: &mut V, block: &Block) {
    for node in block {
        v.visit_stmt(&node.kind, &node.span);
    }
}

/// Walks a statement's children.
pub fn walk_stmt<V: Visitor + ?Sized>(v: &mut V, stmt: &Stmt, _span: &Span) {
    match stmt {
        Stmt::Let { ty, value, .. } => {
            if let Some(ty_node) = ty {
                v.visit_type_expr(&ty_node.kind);
            }
            v.visit_expr(value);
        }
        Stmt::SelfAssign { value, .. } => v.visit_expr(value),
        Stmt::Return(Some(expr)) => v.visit_expr(expr),
        Stmt::Return(None) => {}
        Stmt::For {
            iter, filter, body, ..
        } => {
            v.visit_expr(iter);
            if let Some(filter) = filter {
                v.visit_expr(filter);
            }
            v.visit_block(body);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            v.visit_expr(cond);
            v.visit_block(then_body);
            if let Some(else_body) = else_body {
                v.visit_block(else_body);
            }
        }
        Stmt::When { subject, arms } => {
            v.visit_expr(subject);
            walk_when_arms(v, arms);
        }
        Stmt::TryCatch { body, catches } => {
            v.visit_block(body);
            for catch in catches {
                v.visit_type_expr(&catch.ty.kind);
                v.visit_block(&catch.body);
            }
        }
        Stmt::While { cond, body } => {
            v.visit_expr(cond);
            v.visit_block(body);
        }
        Stmt::AugAssign { rhs, .. } => v.visit_expr(rhs),
        Stmt::Raise(expr) => v.visit_expr(expr),
        Stmt::Assert { cond, message } => {
            v.visit_expr(cond);
            if let Some(message) = message {
                v.visit_expr(message);
            }
        }
        Stmt::Break | Stmt::Continue => {}
        Stmt::Expr(expr) => v.visit_expr(expr),
    }
}

/// Walks an expression's children.
///
/// Accepts a [`SpannedExpr`] (= `Node<Expr>`) so that overriding visitors
/// can access the expression's source span via `expr.span`.
pub fn walk_expr<V: Visitor + ?Sized>(v: &mut V, expr: &SpannedExpr) {
    match &expr.kind {
        Expr::Integer(_)
        | Expr::Float(_)
        | Expr::Bool(_)
        | Expr::None_
        | Expr::Ident(_)
        | Expr::SelfAccess { .. }
        | Expr::SelfRef => {}
        Expr::StringLit(parts) => {
            for part in parts {
                match part {
                    StringPart::Literal(_) | StringPart::ParseError(_) => {}
                    StringPart::Interpolation(expr, _spec) => v.visit_expr(expr),
                }
            }
        }
        Expr::FieldAccess(object, _) | Expr::NullFieldAccess(object, _) => {
            v.visit_expr(object);
        }
        Expr::NullAssert(expr) => v.visit_expr(expr),
        Expr::StructLit(fields) => {
            for (_, value) in fields {
                v.visit_expr(value);
            }
        }
        Expr::StructSpreadUpdate { base, overrides } => {
            v.visit_expr(base);
            for (_, value) in overrides {
                v.visit_expr(value);
            }
        }
        Expr::ListLit(items) | Expr::SetLit(items) | Expr::TupleLit(items) => {
            for item in items {
                v.visit_expr(item);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            v.visit_expr(left);
            v.visit_expr(right);
        }
        Expr::UnaryOp { expr, .. } => v.visit_expr(expr),
        Expr::NullCoalesce(left, right)
        | Expr::Pipeline(left, right)
        | Expr::Range(left, right) => {
            v.visit_expr(left);
            v.visit_expr(right);
        }
        Expr::Call { callee, args } => {
            v.visit_expr(callee);
            walk_call_args(v, args);
        }
        Expr::MethodCall { object, args, .. } => {
            v.visit_expr(object);
            walk_call_args(v, args);
        }
        Expr::Cast { expr, ty } => {
            v.visit_expr(expr);
            v.visit_type_expr(&ty.kind);
        }
        Expr::IfExpr {
            cond,
            then_body,
            else_body,
        } => {
            v.visit_expr(cond);
            v.visit_block(then_body);
            v.visit_block(else_body);
        }
        Expr::WhenExpr { subject, arms } => {
            v.visit_expr(subject);
            walk_when_arms(v, arms);
        }
        Expr::Lambda { params, body } => {
            for param in params {
                if let Some(ty_node) = &param.ty {
                    v.visit_type_expr(&ty_node.kind);
                }
            }
            match body {
                LambdaBody::Expr(expr) => v.visit_expr(expr),
                LambdaBody::Block(block) => v.visit_block(block),
            }
        }
        Expr::Index { object, index } => {
            v.visit_expr(object);
            v.visit_expr(index);
        }
        Expr::Duration { value, .. } => v.visit_expr(value),
        Expr::EnumVariant { fields, .. } => {
            for (_, value) in fields {
                v.visit_expr(value);
            }
        }
    }
}

/// Walks expressions inside a pattern.
pub fn walk_pattern<V: Visitor + ?Sized>(v: &mut V, pattern: &Pattern) {
    match pattern {
        Pattern::Ident(_) | Pattern::Wildcard | Pattern::Variant { .. } => {}
        Pattern::Literal(expr) => v.visit_expr(expr),
    }
}

/// Walks nested type expressions.
pub fn walk_type_expr<V: Visitor + ?Sized>(v: &mut V, ty: &TypeExpr) {
    match ty {
        TypeExpr::Named(_) | TypeExpr::Dynamic | TypeExpr::SelfType => {}
        TypeExpr::Nullable(inner) | TypeExpr::List(inner) | TypeExpr::Set(inner) => {
            v.visit_type_expr(inner);
        }
        TypeExpr::Generic(_, args) => {
            for arg in args {
                v.visit_type_expr(arg);
            }
        }
        TypeExpr::Map(key, value) => {
            v.visit_type_expr(key);
            v.visit_type_expr(value);
        }
        TypeExpr::Struct(fields) => {
            for field in fields {
                v.visit_type_expr(&field.ty.kind);
            }
        }
        TypeExpr::Tuple(items) => {
            for item in items {
                v.visit_type_expr(item);
            }
        }
        TypeExpr::Func(params, ret) => {
            for param in params {
                v.visit_type_expr(param);
            }
            v.visit_type_expr(ret);
        }
    }
}

fn walk_type_decl<V: Visitor + ?Sized>(v: &mut V, decl: &TypeDecl) {
    match &decl.def {
        TypeDef::SimpleEnum(_) => {}
        TypeDef::RichEnum(variants) => {
            for variant in variants {
                if let Some(fields) = &variant.fields {
                    for field in fields {
                        v.visit_type_expr(&field.ty.kind);
                    }
                }
            }
        }
        TypeDef::Struct(fields) => {
            for field in fields {
                v.visit_type_expr(&field.ty.kind);
            }
        }
        TypeDef::Alias(ty_node) => v.visit_type_expr(&ty_node.kind),
    }
}

fn walk_interface_decl<V: Visitor + ?Sized>(v: &mut V, decl: &InterfaceDecl) {
    for method in &decl.methods {
        for param in &method.params {
            walk_param(v, param);
        }
        if let Some(return_type) = &method.return_type {
            v.visit_type_expr(&return_type.kind);
        }
    }
}

fn walk_extern_decl<V: Visitor + ?Sized>(v: &mut V, decl: &ExternDecl) {
    for param in &decl.params {
        walk_param(v, param);
    }
    v.visit_type_expr(&decl.return_type.kind);
}

fn walk_task_decl<V: Visitor + ?Sized>(v: &mut V, decl: &TaskDecl) {
    for param in &decl.params {
        walk_param(v, param);
    }
    if let Some(return_type) = &decl.return_type {
        v.visit_type_expr(&return_type.kind);
    }
    v.visit_block(&decl.body);
}

fn walk_test_decl<V: Visitor + ?Sized>(v: &mut V, decl: &TestDecl) {
    if let Some(param) = &decl.param {
        v.visit_expr(&param.cases);
    }
    v.visit_block(&decl.setup);
    v.visit_block(&decl.body);
}

fn walk_agent_decl<V: Visitor + ?Sized>(v: &mut V, decl: &AgentDecl) {
    for item in &decl.items {
        v.visit_agent_item(item);
    }
}

fn walk_param<V: Visitor + ?Sized>(v: &mut V, param: &Param) {
    v.visit_type_expr(&param.ty.kind);
    if let Some(default) = &param.default {
        v.visit_expr(default);
    }
}

fn walk_call_args<V: Visitor + ?Sized>(v: &mut V, args: &[CallArg]) {
    for arg in args {
        v.visit_expr(&arg.value);
    }
}

fn walk_when_arms<V: Visitor + ?Sized>(v: &mut V, arms: &[WhenArm]) {
    for arm in arms {
        for pattern in &arm.patterns {
            v.visit_pattern(pattern);
        }
        if let Some(guard) = &arm.guard {
            v.visit_expr(guard);
        }
        v.visit_block(&arm.body);
    }
}

#[cfg(test)]
mod tests {
    use super::{self as visit, Visitor};
    use crate::ast::*;
    use crate::lexer::{self, Span};
    use crate::parser;
    use miette::NamedSource;

    fn parse_ok(source: &str) -> Program {
        let named = NamedSource::new("ast_visit.keel", source.to_string());
        let tokens = lexer::lex(source, &named).expect("lexer failed");
        parser::parse(tokens, source.len(), &named).expect("parser failed")
    }

    #[derive(Default)]
    struct Counts {
        decls: usize,
        agent_items: usize,
        attribute_bodies: usize,
        blocks: usize,
        stmts: usize,
        exprs: usize,
        patterns: usize,
        type_exprs: usize,
        saw_tool_condition: bool,
        saw_string_interpolation: bool,
        saw_self_ref: bool,
        saw_self_access: bool,
        saw_null_field_access: bool,
        saw_null_assert: bool,
        saw_struct_lit: bool,
        saw_set_lit: bool,
        saw_tuple_lit: bool,
        saw_pipeline: bool,
        saw_range: bool,
        saw_cast: bool,
        saw_if_expr: bool,
        saw_lambda_expr: bool,
        saw_duration: bool,
        saw_enum_variant: bool,
    }

    impl Visitor for Counts {
        fn visit_decl(&mut self, decl: &Decl, span: &Span) {
            self.decls += 1;
            visit::walk_decl(self, decl, span);
        }

        fn visit_agent_item(&mut self, item: &AgentItem) {
            self.agent_items += 1;
            visit::walk_agent_item(self, item);
        }

        fn visit_attribute_body(&mut self, body: &AttributeBody) {
            self.attribute_bodies += 1;
            if let AttributeBody::Tools(entries) = body {
                self.saw_tool_condition = entries.iter().any(|entry| entry.condition.is_some());
            }
            visit::walk_attribute_body(self, body);
        }

        fn visit_block(&mut self, block: &Block) {
            self.blocks += 1;
            visit::walk_block(self, block);
        }

        fn visit_stmt(&mut self, stmt: &Stmt, span: &Span) {
            self.stmts += 1;
            visit::walk_stmt(self, stmt, span);
        }

        fn visit_expr(&mut self, spanned: &SpannedExpr) {
            self.exprs += 1;
            let expr = &spanned.kind;
            match expr {
                Expr::StringLit(parts) => {
                    self.saw_string_interpolation |= parts
                        .iter()
                        .any(|part| matches!(part, StringPart::Interpolation(..)));
                }
                Expr::NullFieldAccess(_, _) => self.saw_null_field_access = true,
                Expr::NullAssert(_) => self.saw_null_assert = true,
                Expr::SelfAccess { .. } => self.saw_self_access = true,
                Expr::SelfRef => self.saw_self_ref = true,
                Expr::StructLit(_) => self.saw_struct_lit = true,
                Expr::SetLit(_) => self.saw_set_lit = true,
                Expr::TupleLit(_) => self.saw_tuple_lit = true,
                Expr::Pipeline(_, _) => self.saw_pipeline = true,
                Expr::Range(_, _) => self.saw_range = true,
                Expr::Cast { .. } => self.saw_cast = true,
                Expr::IfExpr { .. } => self.saw_if_expr = true,
                Expr::Lambda { .. } => self.saw_lambda_expr = true,
                Expr::Duration { .. } => self.saw_duration = true,
                Expr::EnumVariant { .. } => self.saw_enum_variant = true,
                Expr::Integer(_)
                | Expr::Float(_)
                | Expr::Bool(_)
                | Expr::None_
                | Expr::Ident(_)
                | Expr::FieldAccess(_, _)
                | Expr::ListLit(_)
                | Expr::BinaryOp { .. }
                | Expr::UnaryOp { .. }
                | Expr::NullCoalesce(_, _)
                | Expr::Call { .. }
                | Expr::MethodCall { .. }
                | Expr::WhenExpr { .. }
                | Expr::Index { .. }
                | Expr::StructSpreadUpdate { .. } => {}
            }
            visit::walk_expr(self, spanned);
        }

        fn visit_pattern(&mut self, pattern: &Pattern) {
            self.patterns += 1;
            visit::walk_pattern(self, pattern);
        }

        fn visit_type_expr(&mut self, ty: &TypeExpr) {
            self.type_exprs += 1;
            visit::walk_type_expr(self, ty);
        }
    }

    #[test]
    fn visitor_reaches_major_ast_shapes_from_parsed_program() {
        let source = r#"
use "./shared.keel"
use Helper from "./helper.keel"
use keel/slack

type Severity = low | medium | high | critical
type Action =
  | reply { to: str, tone: str }
  | archive
type Ticket { subject: str, count: int }
type MaybeText = str?
type Pair = (str, int)
type Bag = dynamic

interface Handler {
  task handle(ticket: Ticket) -> Action
}

extern task risky(input: str) -> str from "native"

task helper() -> str {
  return "helper"
}

task inspect(ticket: Ticket, guidance: str? = none) -> str {
  maybe = ticket?.subject ?? "none"
  forced = maybe!
  pair = (1, 2)
  values = [1, 2, 3]
  known = set["a", "b"]
  shaped = { name: "Ada", score: 42 }
  math = -1 + 2 * 3
  flag = not false
  piped = values |> helper
  rng = 1..3
  casted = Json.parse("{}") as dynamic
  later = 5.minutes
  action = Action.reply { to: "ops", tone: "direct" }
  f1 = x => x + 1
  f2 = () => { return "ok" }
  choice = if flag { "yes" } else { "no" }
  label = "done"
  for x in 1..3 if x > 1 {
    Io.show("{x}")
  }
  if maybe != none {
    Io.show(ticket.subject)
  } else {
    Io.show("missing")
  }
  when action {
    reply { to, tone } where tone == "direct" => {
      Io.show(to)
    }
    archive => {
      Io.show("archive")
    }
  }
  try {
    risky(maybe)
  } catch err: Error {
    Io.show(err.message)
  }
  return label
}

agent Bot {
  @role "bot"
  @limits { max_cost_per_request: 0.50, timeout: 30.seconds }
  state {
    ready: bool = false
    last: str? = none
  }
  @tools [Email.fetch, Email.send if self.ready, Io]
  @on_start {
    self.ready = true
    Agent.stop(self)
  }
  on message(msg: Ticket) {
    self.last = msg.subject
    Io.show(self.last)
  }
  task run_one(ticket: Ticket) -> str {
    return inspect(ticket)
  }
}

run(Bot)
"#;

        let program = parse_ok(source);
        let mut visitor = Counts::default();
        visitor.visit_program(&program);

        assert!(visitor.decls >= 13, "decls: {}", visitor.decls);
        assert!(
            visitor.agent_items >= 7,
            "agent_items: {}",
            visitor.agent_items
        );
        assert!(
            visitor.attribute_bodies >= 4,
            "attribute_bodies: {}",
            visitor.attribute_bodies
        );
        assert!(visitor.blocks >= 10, "blocks: {}", visitor.blocks);
        assert!(visitor.stmts >= 35, "stmts: {}", visitor.stmts);
        assert!(visitor.exprs >= 90, "exprs: {}", visitor.exprs);
        assert!(visitor.patterns >= 2, "patterns: {}", visitor.patterns);
        assert!(
            visitor.type_exprs >= 25,
            "type_exprs: {}",
            visitor.type_exprs
        );
        assert!(visitor.saw_tool_condition);
        assert!(visitor.saw_string_interpolation);
        assert!(visitor.saw_self_ref);
        assert!(visitor.saw_self_access);
        assert!(visitor.saw_null_field_access);
        assert!(visitor.saw_null_assert);
        assert!(visitor.saw_struct_lit);
        assert!(visitor.saw_set_lit);
        assert!(visitor.saw_tuple_lit);
        assert!(visitor.saw_pipeline);
        assert!(visitor.saw_range);
        assert!(visitor.saw_cast);
        assert!(visitor.saw_if_expr);
        assert!(visitor.saw_lambda_expr);
        assert!(visitor.saw_duration);
        assert!(visitor.saw_enum_variant);
    }

    #[test]
    fn visitor_walks_function_and_generic_type_expressions() {
        let ty = TypeExpr::Func(
            vec![
                TypeExpr::Generic(
                    "Result".to_string(),
                    vec![TypeExpr::Named("str".to_string()), TypeExpr::Dynamic],
                ),
                TypeExpr::Map(
                    Box::new(TypeExpr::Named("str".to_string())),
                    Box::new(TypeExpr::Set(Box::new(TypeExpr::Named("int".to_string())))),
                ),
            ],
            Box::new(TypeExpr::List(Box::new(TypeExpr::Named(
                "bool".to_string(),
            )))),
        );

        let mut visitor = Counts::default();
        visitor.visit_type_expr(&ty);

        assert_eq!(visitor.type_exprs, 10);
    }
}
