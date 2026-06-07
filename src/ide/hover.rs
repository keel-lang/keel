//! Hover and type-inference IDE helpers used by the language server.
//!
//! These functions run a lightweight subset of the type checker to resolve
//! the type of the identifier under the cursor and to collect all visible
//! bindings for completion proposals.

use std::collections::HashMap;

use crate::ast::*;
use crate::hir::{self, Hir, SymbolId};
use crate::ide::symbols::ident_at_offset;
use crate::lexer::Span;
use crate::types::checker::Checker;
use crate::types::prelude::namespace_names;
use crate::types::scope::Scope;
use crate::types::ty::{Ty, UnknownReason, describe_ty, prelude_label_for};

/// Resolve the inferred type for the identifier at `offset` (UTF-8 byte
/// offset into `text`). Returns `None` if the cursor is not on an
/// identifier or the identifier cannot be resolved.
pub fn type_at(text: &str, offset: usize) -> Option<String> {
    let name = ident_at_offset(text, offset)?;

    if let Some(label) = prelude_label_for(&name) {
        return Some(label.to_string());
    }
    if namespace_names().contains(name.as_str()) {
        return Some(format!("namespace `{name}`"));
    }

    let named = miette::NamedSource::new("file", text.to_string());
    let tokens = crate::lexer::lex(text, &named).ok()?;
    let program = crate::parser::parse(tokens, text.len(), &named).ok()?;
    let hir = hir::lower_ast(&program);

    let mut bindings: HashMap<String, Ty> = HashMap::new();
    let mut checker = Checker::new(&hir);
    checker.collect(&program);
    collect_decl_bindings(&program, &mut checker, &mut bindings);

    bindings.get(&name).map(describe_ty)
}

// ---------------------------------------------------------------------------
// Combined type maps for the LSP semantic index
// ---------------------------------------------------------------------------

/// Build both the [`SymbolId`]-keyed and name-keyed type maps in a single
/// Checker pass.
///
/// Returns `(symbol_types, name_types)`:
/// - `symbol_types`: SymbolId → type-description (scope-correct for reference
///   sites in the semantic index fast path).
/// - `name_types`: name → type-description (scope-unaware fallback for
///   declaration sites where the HIR has no reference entry).
pub(crate) fn build_types(
    program: &Program,
    hir: &Hir<'_>,
) -> (HashMap<SymbolId, String>, HashMap<String, String>) {
    let mut checker = Checker::new(hir);
    checker.collect(program);

    // Build a (name, span) → SymbolId index once so insert_symbol can do
    // an O(1) lookup instead of a linear scan through hir.symbols().
    let decl_ids: HashMap<(String, Span), SymbolId> = hir
        .symbols()
        .iter()
        .map(|s| ((s.name.clone(), s.span.clone()), s.id))
        .collect();

    let mut symbol_types: HashMap<SymbolId, String> = HashMap::new();
    let mut name_bindings: HashMap<String, Ty> = HashMap::new();

    collect_symbol_decl_types(program, &decl_ids, &mut checker, &mut symbol_types);
    collect_decl_bindings(program, &mut checker, &mut name_bindings);

    let name_types = name_bindings
        .into_iter()
        .map(|(name, ty)| (name, describe_ty(&ty)))
        .collect();

    (symbol_types, name_types)
}

// ---------------------------------------------------------------------------
// Binding collectors (used by type_at and build_name_types)
// ---------------------------------------------------------------------------

pub(crate) fn insert_binding(
    binding: &Binding,
    ty: Ty,
    _c: &mut Checker<'_, '_>,
    out: &mut HashMap<String, Ty>,
) {
    match binding {
        Binding::Ident(name) => {
            out.insert(name.clone(), ty);
        }
        Binding::Destruct(DestructPat::Struct(fields)) => {
            let struct_fields = match &ty {
                Ty::Struct { fields: f, .. } => f.clone(),
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
    c: &mut Checker<'_, '_>,
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

pub(crate) fn collect_stmt_bindings(
    stmt: &Stmt,
    c: &mut Checker<'_, '_>,
    out: &mut HashMap<String, Ty>,
) {
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

// ---------------------------------------------------------------------------
// SymbolId-keyed type collection (internal helpers for build_types)
// ---------------------------------------------------------------------------

fn collect_symbol_decl_types(
    program: &Program,
    decl_ids: &HashMap<(String, Span), SymbolId>,
    c: &mut Checker<'_, '_>,
    out: &mut HashMap<SymbolId, String>,
) {
    for node in &program.declarations {
        match &node.kind {
            Decl::Stmt(stmt_node) => {
                collect_stmt_symbol_types(&stmt_node.kind, &stmt_node.span, decl_ids, c, out);
            }
            Decl::Task(t) => {
                for p in &t.params {
                    let ty = c.resolve_type(&p.ty.kind);
                    insert_binding_symbol(&p.name, &p.name_span, ty, decl_ids, out);
                }
                for s_node in &t.body {
                    collect_stmt_symbol_types(&s_node.kind, &s_node.span, decl_ids, c, out);
                }
            }
            Decl::Agent(decl) => {
                for it in &decl.items {
                    match it {
                        AgentItem::State(fields) => {
                            for sf in fields {
                                let ty = c.resolve_type(&sf.ty.kind);
                                insert_symbol(&sf.name, &sf.name_span, ty, decl_ids, out);
                            }
                        }
                        AgentItem::Task(t) => {
                            for p in &t.params {
                                let ty = c.resolve_type(&p.ty.kind);
                                insert_binding_symbol(&p.name, &p.name_span, ty, decl_ids, out);
                            }
                            for s_node in &t.body {
                                collect_stmt_symbol_types(
                                    &s_node.kind,
                                    &s_node.span,
                                    decl_ids,
                                    c,
                                    out,
                                );
                            }
                        }
                        AgentItem::On(h) => {
                            if let Some(p) = &h.param {
                                let ty = c.resolve_type(&p.ty.kind);
                                insert_binding_symbol(&p.name, &p.name_span, ty, decl_ids, out);
                            }
                            for s_node in &h.body {
                                collect_stmt_symbol_types(
                                    &s_node.kind,
                                    &s_node.span,
                                    decl_ids,
                                    c,
                                    out,
                                );
                            }
                        }
                        AgentItem::Attribute(attr) => {
                            if let AttributeBody::Block(block) = &attr.body {
                                for s_node in block {
                                    collect_stmt_symbol_types(
                                        &s_node.kind,
                                        &s_node.span,
                                        decl_ids,
                                        c,
                                        out,
                                    );
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

fn collect_stmt_symbol_types(
    stmt: &Stmt,
    stmt_span: &Span,
    decl_ids: &HashMap<(String, Span), SymbolId>,
    c: &mut Checker<'_, '_>,
    out: &mut HashMap<SymbolId, String>,
) {
    match stmt {
        Stmt::Let { binding, ty, value } => {
            let mut scope = Scope::new();
            let inferred = c.infer_expr(value, &mut scope);
            let bound = ty
                .as_ref()
                .map(|t| c.resolve_type(&t.kind))
                .unwrap_or(inferred);
            // HIR stores let-binding symbols with span = stmt_span.
            insert_binding_symbol(binding, stmt_span, bound, decl_ids, out);
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
            // HIR stores for-binding symbols with span = for-stmt span.
            insert_binding_symbol(binding, stmt_span, elem, decl_ids, out);
            for s_node in body {
                collect_stmt_symbol_types(&s_node.kind, &s_node.span, decl_ids, c, out);
            }
        }
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s_node in then_body {
                collect_stmt_symbol_types(&s_node.kind, &s_node.span, decl_ids, c, out);
            }
            if let Some(eb) = else_body {
                for s_node in eb {
                    collect_stmt_symbol_types(&s_node.kind, &s_node.span, decl_ids, c, out);
                }
            }
        }
        Stmt::When { arms, .. } => {
            for arm in arms {
                for s_node in &arm.body {
                    collect_stmt_symbol_types(&s_node.kind, &s_node.span, decl_ids, c, out);
                }
            }
        }
        Stmt::While { body, .. } => {
            for s_node in body {
                collect_stmt_symbol_types(&s_node.kind, &s_node.span, decl_ids, c, out);
            }
        }
        Stmt::TryCatch { body, catches } => {
            for s_node in body {
                collect_stmt_symbol_types(&s_node.kind, &s_node.span, decl_ids, c, out);
            }
            for catch in catches {
                let ty = c.resolve_type(&catch.ty.kind);
                // HIR stores catch-binding symbols with span = catch.ty.span.
                insert_symbol(&catch.name, &catch.ty.span, ty, decl_ids, out);
                for s_node in &catch.body {
                    collect_stmt_symbol_types(&s_node.kind, &s_node.span, decl_ids, c, out);
                }
            }
        }
        _ => {}
    }
}

/// Insert a single named binding's type, keyed by its HIR SymbolId.
///
/// Uses the pre-built `decl_ids` map for O(1) lookup by (name, span).
fn insert_symbol(
    name: &str,
    span: &Span,
    ty: Ty,
    decl_ids: &HashMap<(String, Span), SymbolId>,
    out: &mut HashMap<SymbolId, String>,
) {
    if let Some(&id) = decl_ids.get(&(name.to_string(), span.clone())) {
        out.insert(id, describe_ty(&ty));
    }
}

/// Insert all names in a `Binding` pattern, each with their SymbolId.
///
/// For `Binding::Ident`, the HIR stored the symbol at `span`.
/// For destructure patterns, every destructured name shares the same `span`.
fn insert_binding_symbol(
    binding: &Binding,
    span: &Span,
    ty: Ty,
    decl_ids: &HashMap<(String, Span), SymbolId>,
    out: &mut HashMap<SymbolId, String>,
) {
    match binding {
        Binding::Ident(name) => insert_symbol(name, span, ty, decl_ids, out),
        Binding::Destruct(DestructPat::Struct(fields)) => {
            let struct_fields = match &ty {
                Ty::Struct { fields: f, .. } => f.clone(),
                _ => vec![],
            };
            for (source, local) in fields {
                let field_ty = struct_fields
                    .iter()
                    .find(|(n, _)| n == source)
                    .map(|(_, t)| t.clone())
                    .unwrap_or(Ty::Unknown(UnknownReason::InferenceLimitation));
                insert_symbol(local, span, field_ty, decl_ids, out);
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
                insert_symbol(name, span, t, decl_ids, out);
            }
        }
    }
}
