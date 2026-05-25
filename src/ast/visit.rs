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

    /// Visits an expression.
    fn visit_expr(&mut self, expr: &Expr) {
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
    for (decl, span) in &program.declarations {
        v.visit_decl(decl, span);
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
        Decl::Extern(extern_decl) => walk_extern_decl(v, extern_decl),
        Decl::Agent(agent_decl) => walk_agent_decl(v, agent_decl),
        Decl::Use(UseDecl { kind }) => match kind {
            UseKind::File(_) | UseKind::Symbol { .. } | UseKind::Package(_) => {}
        },
        Decl::Stmt((stmt, span)) => v.visit_stmt(stmt, span),
    }
}

/// Walks an agent body item.
pub fn walk_agent_item<V: Visitor + ?Sized>(v: &mut V, item: &AgentItem) {
    match item {
        AgentItem::Attribute(attribute) => v.visit_attribute_body(&attribute.body),
        AgentItem::State(fields) => {
            for field in fields {
                v.visit_type_expr(&field.ty);
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
    for (stmt, span) in block {
        v.visit_stmt(stmt, span);
    }
}

/// Walks a statement's children.
pub fn walk_stmt<V: Visitor + ?Sized>(v: &mut V, stmt: &Stmt, _span: &Span) {
    match stmt {
        Stmt::Let { ty, value, .. } => {
            if let Some(ty) = ty {
                v.visit_type_expr(ty);
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
                v.visit_type_expr(&catch.ty);
                v.visit_block(&catch.body);
            }
        }
        Stmt::While { cond, body } => {
            v.visit_expr(cond);
            v.visit_block(body);
        }
        Stmt::AugAssign { rhs, .. } => v.visit_expr(rhs),
        Stmt::Raise(expr) => v.visit_expr(expr),
        Stmt::Break | Stmt::Continue => {}
        Stmt::Expr(expr) => v.visit_expr(expr),
    }
}

/// Walks an expression's children.
pub fn walk_expr<V: Visitor + ?Sized>(v: &mut V, expr: &Expr) {
    match expr {
        Expr::Integer(_)
        | Expr::Float(_)
        | Expr::Bool(_)
        | Expr::None_
        | Expr::Ident(_)
        | Expr::SelfAccess(_)
        | Expr::SelfRef => {}
        Expr::StringLit(parts) => {
            for part in parts {
                match part {
                    StringPart::Literal(_) => {}
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
            v.visit_type_expr(ty);
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
                if let Some(ty) = &param.ty {
                    v.visit_type_expr(ty);
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
        TypeExpr::Named(_) | TypeExpr::Dynamic => {}
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
                v.visit_type_expr(&field.ty);
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
                        v.visit_type_expr(&field.ty);
                    }
                }
            }
        }
        TypeDef::Struct(fields) => {
            for field in fields {
                v.visit_type_expr(&field.ty);
            }
        }
        TypeDef::Alias(ty) => v.visit_type_expr(ty),
    }
}

fn walk_interface_decl<V: Visitor + ?Sized>(v: &mut V, decl: &InterfaceDecl) {
    for method in &decl.methods {
        for param in &method.params {
            walk_param(v, param);
        }
        if let Some(return_type) = &method.return_type {
            v.visit_type_expr(return_type);
        }
    }
}

fn walk_extern_decl<V: Visitor + ?Sized>(v: &mut V, decl: &ExternDecl) {
    for param in &decl.params {
        walk_param(v, param);
    }
    v.visit_type_expr(&decl.return_type);
}

fn walk_task_decl<V: Visitor + ?Sized>(v: &mut V, decl: &TaskDecl) {
    for param in &decl.params {
        walk_param(v, param);
    }
    if let Some(return_type) = &decl.return_type {
        v.visit_type_expr(return_type);
    }
    v.visit_block(&decl.body);
}

fn walk_agent_decl<V: Visitor + ?Sized>(v: &mut V, decl: &AgentDecl) {
    for item in &decl.items {
        v.visit_agent_item(item);
    }
}

fn walk_param<V: Visitor + ?Sized>(v: &mut V, param: &Param) {
    v.visit_type_expr(&param.ty);
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
