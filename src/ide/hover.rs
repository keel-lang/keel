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

    // Single traversal populates both maps; avoids double infer_expr calls.
    visit_decl_bindings(program, &mut checker, |binding, span, ty| {
        insert_binding_symbol(binding, span, ty.clone(), &decl_ids, &mut symbol_types);
        insert_binding(binding, ty, &mut name_bindings);
    });

    let name_types = name_bindings
        .into_iter()
        .map(|(name, ty)| (name, describe_ty(&ty)))
        .collect();

    (symbol_types, name_types)
}

// ---------------------------------------------------------------------------
// Binding collectors
// ---------------------------------------------------------------------------

pub(crate) fn insert_binding(binding: &Binding, ty: Ty, out: &mut HashMap<String, Ty>) {
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

fn collect_decl_bindings(
    program: &Program,
    c: &mut Checker<'_, '_>,
    out: &mut HashMap<String, Ty>,
) {
    visit_decl_bindings(program, c, |binding, _span, ty| {
        insert_binding(binding, ty, out);
    });
}

// ---------------------------------------------------------------------------
// Generic AST binding visitor
// ---------------------------------------------------------------------------

/// Walk every binding site in `program`, calling `visitor` with `(binding, span, ty)`.
///
/// Covers: top-level `task` params and bodies, `agent` state fields, task/on
/// params and bodies, `impl` method params and bodies, `when` arm
/// `Pattern::Variant` destructures (span = `0..0`, type = `Unknown`), `let`,
/// `for`, `if`/`else`, `while`, `when` bodies, and `try/catch` clauses.
///
/// Spans match what the HIR records for each symbol: `stmt_span` for `let`/`for`,
/// `name_span` for params and state fields, `ty.span` for catch-clause names.
///
/// Any new binding site in the AST requires only one edit here.
fn visit_decl_bindings<F: FnMut(&Binding, &Span, Ty)>(
    program: &Program,
    c: &mut Checker<'_, '_>,
    mut visitor: F,
) {
    for node in &program.declarations {
        match &node.kind {
            Decl::Stmt(stmt_node) => {
                visit_stmt_bindings(&stmt_node.kind, &stmt_node.span, c, &mut visitor);
            }
            Decl::Task(t) => {
                for p in &t.params {
                    visitor(&p.name, &p.name_span, c.resolve_type(&p.ty.kind));
                }
                recurse_body(&t.body, c, &mut visitor);
            }
            Decl::Test(t) => {
                recurse_body(&t.body, c, &mut visitor);
            }
            Decl::Agent(decl) => {
                for it in &decl.items {
                    match it {
                        AgentItem::State(fields) => {
                            for sf in fields {
                                visitor(
                                    &Binding::Ident(sf.name.clone()),
                                    &sf.name_span,
                                    c.resolve_type(&sf.ty.kind),
                                );
                            }
                        }
                        AgentItem::Task(t) => {
                            for p in &t.params {
                                visitor(&p.name, &p.name_span, c.resolve_type(&p.ty.kind));
                            }
                            recurse_body(&t.body, c, &mut visitor);
                        }
                        AgentItem::On(h) => {
                            if let Some(p) = &h.param {
                                visitor(&p.name, &p.name_span, c.resolve_type(&p.ty.kind));
                            }
                            recurse_body(&h.body, c, &mut visitor);
                        }
                        AgentItem::Attribute(attr) => {
                            if let AttributeBody::Block(block) = &attr.body {
                                recurse_body(block, c, &mut visitor);
                            }
                        }
                    }
                }
            }
            Decl::Impl(decl) => {
                for method in &decl.methods {
                    for p in &method.params {
                        visitor(&p.name, &p.name_span, c.resolve_type(&p.ty.kind));
                    }
                    recurse_body(&method.body, c, &mut visitor);
                }
            }
            // Decl::Type / Decl::Interface / Decl::Extern / Decl::Use / Decl::Stmt
            // introduce no locally-scoped let/for/param bindings visible to hover.
            _ => {}
        }
    }
}

fn recurse_body<F: FnMut(&Binding, &Span, Ty)>(
    body: &[Node<Stmt>],
    c: &mut Checker<'_, '_>,
    visitor: &mut F,
) {
    for s_node in body {
        visit_stmt_bindings(&s_node.kind, &s_node.span, c, visitor);
    }
}

fn visit_stmt_bindings<F: FnMut(&Binding, &Span, Ty)>(
    stmt: &Stmt,
    stmt_span: &Span,
    c: &mut Checker<'_, '_>,
    visitor: &mut F,
) {
    match stmt {
        Stmt::Let { binding, ty, value } => {
            let mut scope = Scope::new();
            let inferred = c.infer_expr(value, &mut scope);
            let bound = ty
                .as_ref()
                .map(|t| c.resolve_type(&t.kind))
                .unwrap_or(inferred);
            visitor(binding, stmt_span, bound);
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
            visitor(binding, stmt_span, elem);
            recurse_body(body, c, visitor);
        }
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            recurse_body(then_body, c, visitor);
            if let Some(eb) = else_body {
                recurse_body(eb, c, visitor);
            }
        }
        Stmt::When { arms, .. } => {
            // HIR records variant-destructure names with exprless_span (0..0);
            // their type is not inferrable without subject-type analysis.
            for arm in arms {
                for pattern in &arm.patterns {
                    for name in pattern.destructured_names() {
                        visitor(
                            &Binding::Ident(name.to_string()),
                            &(0..0),
                            Ty::Unknown(UnknownReason::InferenceLimitation),
                        );
                    }
                }
                recurse_body(&arm.body, c, visitor);
            }
        }
        Stmt::While { body, .. } => {
            recurse_body(body, c, visitor);
        }
        Stmt::TryCatch { body, catches } => {
            recurse_body(body, c, visitor);
            for catch in catches {
                let ty = c.resolve_type(&catch.ty.kind);
                // HIR stores catch-clause symbols with span = catch.ty.span.
                visitor(&Binding::Ident(catch.name.clone()), &catch.ty.span, ty);
                recurse_body(&catch.body, c, visitor);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// SymbolId-keyed insert helpers (used by the symbol closure in build_types)
// ---------------------------------------------------------------------------

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
