//! LSP — placeholder for v0.1.
//!
//! The language server will return once the type checker and interpreter
//! migrations land. For now, `keel lsp` prints a message and exits.

pub async fn start() {
    eprintln!(
        "Keel LSP is unavailable in v0.1 alpha — the language server will \
         return once the type checker is rewritten (see ROADMAP.md)."
    );
}
