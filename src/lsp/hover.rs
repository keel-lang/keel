//! Hover type resolution for the language server.
//!
//! When a [`SemanticIndex`] is available (populated after every successful
//! `did_open` / `did_change`), hover is answered from the pre-built index
//! with zero reparsing.  On parse failure the index is absent and hover
//! falls back to the reparse-on-demand [`checker::type_at`] helper.

use crate::types::checker;
use crate::types::prelude::namespace_names;
use crate::types::ty::prelude_label_for;

use super::diagnostics::SemanticIndex;

/// Resolve the inferred type label for the identifier at `offset`.
///
/// Queries the semantic index when available (fast path, no reparse).
/// Falls back to [`checker::type_at`] when the index is absent (parse failure).
pub(crate) fn resolve_hover_type(
    text: &str,
    offset: usize,
    index: Option<&SemanticIndex>,
) -> Option<String> {
    if let Some(idx) = index {
        // Fast path: span-keyed lookup (scope-correct for reference sites).
        let ident_span = checker::ident_span_at_offset(text, offset)?;
        if let Some(ty_str) = idx.span_types.get(&ident_span) {
            return Some(ty_str.clone());
        }
        let name = checker::ident_at_offset(text, offset)?;
        // Tier 1: document-scoped user bindings (declaration sites).
        if let Some(ty_str) = idx.name_types.get(&name) {
            return Some(ty_str.clone());
        }
        // Tier 2: process-wide static prelude labels.
        if let Some(label) = prelude_label_for(&name) {
            return Some(label.to_string());
        }
        if namespace_names().contains(name.as_str()) {
            return Some(format!("namespace `{name}`"));
        }
        return None;
    }
    // No index (parse failure): reparse on demand.
    checker::type_at(text, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::diagnostics::analyze_document;

    /// Hovering over a primitive type name resolves via the tier-2 static
    /// path even when `name_types` no longer stores prelude entries.
    #[test]
    fn hover_primitive_type_name_resolves_via_tier2() {
        let src = "task t(x: int) { }\n";
        let (_, index) = analyze_document(src, None);
        let offset = src.find("int").unwrap() + 1;
        let label = resolve_hover_type(src, offset, index.as_ref());
        assert_eq!(label.as_deref(), Some("type `int`"));
    }

    /// A user binding named `str` shadows the prelude primitive: tier-1 wins.
    #[test]
    fn user_binding_shadows_prelude_primitive() {
        // `str` here is used as a variable name (valid — not a reserved keyword).
        let src = "task t() {\n  str = \"hello\"\n}\n";
        let (_, index) = analyze_document(src, None);
        // Hover on the `str` declaration site.
        let offset = src.find("str").unwrap() + 1;
        let label = resolve_hover_type(src, offset, index.as_ref());
        // Tier-1 user binding wins: the inferred type of "hello" is str.
        assert_eq!(
            label.as_deref(),
            Some("str"),
            "user binding should shadow prelude: got {label:?}"
        );
    }
}
