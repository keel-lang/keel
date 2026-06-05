//! Go-to-definition for the language server.
//!
//! When a [`SemanticIndex`] is available, the pre-built definitions map is
//! queried with zero reparsing.  On parse failure the handler falls back to
//! [`checker::definition_of`].

use crate::lexer::Span;
use crate::types::checker;

use super::diagnostics::SemanticIndex;

/// Return the declaration span of the identifier at `offset`, or `None` if
/// the cursor is not on an identifier or no matching declaration exists.
///
/// Queries the semantic index when available (fast path, no reparse).
pub(crate) fn find_definition(
    text: &str,
    offset: usize,
    index: Option<&SemanticIndex>,
) -> Option<Span> {
    if let Some(idx) = index {
        let ident_span = checker::ident_span_at_offset(text, offset)?;
        if let Some(span) = idx.definitions.get(&ident_span) {
            return Some(span.clone());
        }
        // Index present but span not found — fall through to reparse.
        // This covers reference spans that the HIR records without a resolved
        // symbol (e.g. prelude namespace names where definition_of also returns
        // None, but future partially-resolved constructs benefit from the try).
    }
    checker::definition_of(text, offset)
}
