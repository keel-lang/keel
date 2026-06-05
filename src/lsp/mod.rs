//! Keel Language Server — v0.1.
//!
//! Scope for this release: diagnostics only. On every `did_open` /
//! `did_change`, we lex → parse → lower HIR → type-check and publish the
//! resulting errors as LSP diagnostics. Hover and completion are backed by
//! a per-document semantic index built from the HIR and prelude catalog.

mod backend;
mod completion;
mod definition;
mod diagnostics;
mod hover;
mod position;
mod rename;

pub(crate) use backend::Backend;

pub async fn start() {
    use tower_lsp::{LspService, Server};

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        docs: parking_lot::Mutex::new(std::collections::HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
