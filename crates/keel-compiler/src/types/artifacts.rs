// Rust guideline compliant 2026-02-21
//! Post-check artifacts: per-expression resolved types and generic
//! instantiations.
//!
//! The checker computes a [`Ty`] for every expression it walks but
//! historically discarded them, returning only diagnostics.  This module
//! persists those results so downstream consumers (KIR lowering, IDE hover)
//! can read them without re-running inference.  Produced by
//! [`crate::types::checker::check_program_with_artifacts`] and
//! [`crate::types::checker::check_graph_with_artifacts`]; recording is pure
//! instrumentation and never changes what the checker accepts or reports.

use std::collections::HashMap;

use crate::lexer::Span;
use crate::types::ty::Ty;

/// Resolved types and generic instantiations collected during one check pass.
#[derive(Debug, Clone, Default)]
pub struct CheckArtifacts {
    /// Resolved type of every inferred expression, keyed by source span
    /// (byte-offset range).
    ///
    /// Key choice: spans, not synthetic expression IDs.  The AST carries no
    /// per-expression identity (HIR `SymbolId`s cover declarations only), so
    /// the alternatives were a parser/AST change to mint IDs or keying by
    /// the span every `SpannedExpr` already carries.  Spans win on
    /// simplicity and are unique per expression node in practice: the parser
    /// re-spans a single-element parenthesised group to cover the parens
    /// (one node, one span — the inner node is consumed, not nested) and
    /// never synthesises expression nodes that borrow another node's span.
    /// If the checker visits one expression more than once, the last
    /// inference wins; inference is deterministic, so repeated visits under
    /// the same scope agree.
    pub expr_types: HashMap<Span, Ty>,

    /// Concrete type arguments each generic task or type was instantiated
    /// with, keyed by the generic declaration's name.
    ///
    /// Each entry lists the distinct argument vectors seen, ordered
    /// positionally per the declared type parameters; exact duplicates are
    /// collapsed.  Instantiation sites are not recorded — the KIR
    /// monomorphizer needs the set of stamps, not their locations.
    pub generic_instantiations: HashMap<String, Vec<Vec<Ty>>>,
}

impl CheckArtifacts {
    /// Record the resolved type of the expression covering `span`.
    pub(crate) fn record_expr(&mut self, span: Span, ty: &Ty) {
        self.expr_types.insert(span, ty.clone());
    }

    /// Record one generic instantiation, collapsing exact duplicates.
    pub(crate) fn record_instantiation(&mut self, name: &str, type_args: Vec<Ty>) {
        let entries = self
            .generic_instantiations
            .entry(name.to_string())
            .or_default();
        if !entries.contains(&type_args) {
            entries.push(type_args);
        }
    }

    /// Resolved type of the innermost recorded expression covering byte
    /// `offset`, or `None` when no recorded expression contains it.
    ///
    /// "Innermost" is the shortest containing span — expression spans nest
    /// or are disjoint, so the shortest one is the deepest node.  The start
    /// position breaks the (theoretical) tie between equal-length spans
    /// deterministically.
    #[must_use]
    pub fn ty_at(&self, offset: usize) -> Option<&Ty> {
        self.expr_types
            .iter()
            .filter(|(span, _)| span.start <= offset && offset < span.end)
            .min_by_key(|(span, _)| (span.end - span.start, span.start))
            .map(|(_, ty)| ty)
    }
}
