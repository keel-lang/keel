//! LSP completion items, derived from the prelude catalog and reserved keywords.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Documentation};

use crate::types::prelude;

/// Build the full list of completion items: namespace names, method names,
/// and reserved keywords.  All items are derived from [`prelude::catalog()`]
/// or the static keyword list so completions stay in sync with the runtime.
pub(crate) fn completion_items() -> Vec<CompletionItem> {
    let mut completions = Vec::new();

    // Namespace completions — one MODULE item per unique namespace name.
    let mut seen_ns = std::collections::HashSet::new();
    for entry in prelude::catalog() {
        if seen_ns.insert(entry.namespace) {
            completions.push(CompletionItem {
                label: entry.namespace.to_string(),
                kind: Some(CompletionItemKind::MODULE),
                ..CompletionItem::default()
            });
        }
    }

    // Method completions — one FUNCTION item per catalog entry.
    for entry in prelude::catalog() {
        completions.push(CompletionItem {
            label: entry.name.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(format!("{} method", entry.namespace)),
            documentation: Some(Documentation::String(entry.doc.to_string())),
            ..CompletionItem::default()
        });
    }

    // Reserved keywords — must match the keyword list in AGENTS.md / SPEC.md.
    let keywords = [
        "agent",
        "task",
        "interface",
        "impl",
        "type",
        "extern",
        "use",
        "from",
        "state",
        "on",
        "self",
        "if",
        "else",
        "when",
        "where",
        "for",
        "while",
        "in",
        "break",
        "continue",
        "try",
        "catch",
        "return",
        "raise",
        "as",
        "and",
        "or",
        "not",
        "true",
        "false",
        "none",
        "set",
        "test",
        "mock",
        "setup",
        "assert",
    ];
    for kw in keywords {
        completions.push(CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..CompletionItem::default()
        });
    }

    completions
}
