//! Hover and type-inference IDE helpers used by the language server.
//!
//! These functions run a lightweight subset of the type checker to resolve
//! the type of the identifier under the cursor and to collect all visible
//! bindings for completion proposals.

use std::collections::HashMap;

use crate::ast::*;
use crate::ide::symbols::ident_at_offset;
use crate::types::checker::Checker;
use crate::types::scope::Scope;
use crate::types::ty::{Ty, UnknownReason, describe_ty};

/// Resolve the inferred type for the identifier at `offset` (UTF-8 byte
/// offset into `text`). Returns `None` if the cursor is not on an
/// identifier or the identifier cannot be resolved.
pub fn type_at(text: &str, offset: usize) -> Option<String> {
    let name = ident_at_offset(text, offset)?;

    if matches!(
        name.as_str(),
        "Ai" | "Io"
            | "Http"
            | "Shell"
            | "Email"
            | "Search"
            | "Db"
            | "Memory"
            | "Schedule"
            | "Async"
            | "Control"
            | "Env"
            | "Time"
            | "Log"
            | "Agent"
            | "Cache"
            | "File"
            | "Json"
            | "Random"
            | "Uuid"
            | "Crypto"
            | "Math"
            | "Csv"
    ) {
        return Some(format!("namespace `{name}`"));
    }
    if matches!(
        name.as_str(),
        "int"
            | "float"
            | "str"
            | "bool"
            | "none"
            | "datetime"
            | "duration"
            | "Uuid"
            | "list"
            | "map"
            | "set"
            | "dynamic"
    ) {
        return Some(format!("type `{name}`"));
    }

    let named = miette::NamedSource::new("file", text.to_string());
    let tokens = crate::lexer::lex(text, &named).ok()?;
    let program = crate::parser::parse(tokens, text.len(), &named).ok()?;

    let mut bindings: HashMap<String, Ty> = HashMap::new();
    let mut checker = Checker::new();
    checker.collect(&program);
    collect_decl_bindings(&program, &mut checker, &mut bindings);

    bindings.get(&name).map(describe_ty)
}

// ---------------------------------------------------------------------------
// Binding collectors (used by type_at)
// ---------------------------------------------------------------------------

pub(crate) fn insert_binding(
    binding: &Binding,
    ty: Ty,
    _c: &mut Checker,
    out: &mut HashMap<String, Ty>,
) {
    match binding {
        Binding::Ident(name) => {
            out.insert(name.clone(), ty);
        }
        Binding::Destruct(DestructPat::Struct(fields)) => {
            let struct_fields = match &ty {
                Ty::Struct(f) => f.clone(),
                _ => vec![],
            };
            for (source, local) in fields {
                let field_ty = struct_fields
                    .iter()
                    .find(|(n, _)| n == source)
                    .map(|(_, t)| t.clone())
                    .unwrap_or(Ty::Unknown(UnknownReason::InferenceLimitation));
                out.insert(local.clone(), field_ty);
            }
        }
        Binding::Destruct(DestructPat::Tuple(names)) => {
            let elem_tys = match ty {
                Ty::Tuple(items) => items,
                _ => vec![],
            };
            for (i, name) in names.iter().enumerate() {
                let t = elem_tys
                    .get(i)
                    .cloned()
                    .unwrap_or(Ty::Unknown(UnknownReason::InferenceLimitation));
                out.insert(name.clone(), t);
            }
        }
    }
}

pub(crate) fn collect_decl_bindings(
    program: &Program,
    c: &mut Checker,
    out: &mut HashMap<String, Ty>,
) {
    for node in &program.declarations {
        match &node.kind {
            Decl::Stmt(stmt_node) => collect_stmt_bindings(&stmt_node.kind, c, out),
            Decl::Task(t) => {
                for p in &t.params {
                    insert_binding(&p.name, c.resolve_type(&p.ty.kind), c, out);
                }
                for s_node in &t.body {
                    collect_stmt_bindings(&s_node.kind, c, out);
                }
            }
            Decl::Agent(decl) => {
                for it in &decl.items {
                    match it {
                        AgentItem::State(fields) => {
                            for sf in fields {
                                out.insert(sf.name.clone(), c.resolve_type(&sf.ty.kind));
                            }
                        }
                        AgentItem::Task(t) => {
                            for p in &t.params {
                                insert_binding(&p.name, c.resolve_type(&p.ty.kind), c, out);
                            }
                            for s_node in &t.body {
                                collect_stmt_bindings(&s_node.kind, c, out);
                            }
                        }
                        AgentItem::On(h) => {
                            if let Some(p) = &h.param {
                                insert_binding(&p.name, c.resolve_type(&p.ty.kind), c, out);
                            }
                            for s_node in &h.body {
                                collect_stmt_bindings(&s_node.kind, c, out);
                            }
                        }
                        AgentItem::Attribute(attr) => {
                            if let AttributeBody::Block(block) = &attr.body {
                                for s_node in block {
                                    collect_stmt_bindings(&s_node.kind, c, out);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn collect_stmt_bindings(stmt: &Stmt, c: &mut Checker, out: &mut HashMap<String, Ty>) {
    match stmt {
        Stmt::Let { binding, ty, value } => {
            let mut scope = Scope::new();
            let inferred = c.infer_expr(value, &mut scope);
            let bound = ty
                .as_ref()
                .map(|t| c.resolve_type(&t.kind))
                .unwrap_or(inferred);
            insert_binding(binding, bound, c, out);
        }
        Stmt::For {
            binding,
            iter,
            body,
            ..
        } => {
            let mut scope = Scope::new();
            let iter_ty = c.infer_expr(iter, &mut scope);
            let elem = match iter_ty.strip_nullable() {
                Ty::List(inner) => *inner.clone(),
                _ => Ty::Unknown(UnknownReason::InferenceLimitation),
            };
            insert_binding(binding, elem, c, out);
            for s_node in body {
                collect_stmt_bindings(&s_node.kind, c, out);
            }
        }
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s_node in then_body {
                collect_stmt_bindings(&s_node.kind, c, out);
            }
            if let Some(eb) = else_body {
                for s_node in eb {
                    collect_stmt_bindings(&s_node.kind, c, out);
                }
            }
        }
        Stmt::When { arms, .. } => {
            for arm in arms {
                for s_node in &arm.body {
                    collect_stmt_bindings(&s_node.kind, c, out);
                }
            }
        }
        Stmt::While { body, .. } => {
            for s_node in body {
                collect_stmt_bindings(&s_node.kind, c, out);
            }
        }
        Stmt::TryCatch { body, catches } => {
            for s_node in body {
                collect_stmt_bindings(&s_node.kind, c, out);
            }
            for catch in catches {
                out.insert(catch.name.clone(), c.resolve_type(&catch.ty.kind));
                for s_node in &catch.body {
                    collect_stmt_bindings(&s_node.kind, c, out);
                }
            }
        }
        _ => {}
    }
}
