//! Keel Language Server — v0.1.
//!
//! Scope for this release: diagnostics only. On every `did_open` /
//! `did_change`, we lex → parse → type-check and publish the resulting
//! errors as LSP diagnostics. Hover and completion are placeholders
//! pending a follow-up.

use std::collections::HashMap;
use std::sync::Mutex;

use miette::NamedSource;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::lexer::{self, Span};
use crate::parser;
use crate::types::checker;

pub async fn start() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        docs: Mutex::new(HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

struct Backend {
    client: Client,
    /// In-memory snapshot of open documents: URI → current text.
    docs: Mutex<HashMap<Url, String>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "keel-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Keel LSP v0.1 ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text;
        self.docs.lock().unwrap().insert(uri.clone(), text.clone());
        self.publish(&uri, &text).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        // FULL sync mode: the last content change holds the new full text.
        if let Some(change) = params.content_changes.pop() {
            self.docs.lock().unwrap().insert(uri.clone(), change.text.clone());
            self.publish(&uri, &change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.lock().unwrap().remove(&params.text_document.uri);
        // Clear diagnostics for the closed file.
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn hover(&self, _params: HoverParams) -> Result<Option<Hover>> {
        // v0.1: no hover info yet. Returning `None` keeps VS Code happy.
        Ok(None)
    }
}

impl Backend {
    async fn publish(&self, uri: &Url, text: &str) {
        let diagnostics = analyze(text);
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }
}

/// Run lex/parse/type-check and convert every failure into an LSP
/// diagnostic. Empty vec means a clean file.
pub fn analyze(text: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    let named = NamedSource::new("file", text.to_string());

    let tokens = match lexer::lex(text, &named) {
        Ok(t) => t,
        Err(report) => {
            for (message, span) in spans_from_report(&report) {
                out.push(diag(text, span, message, DiagnosticSeverity::ERROR));
            }
            return out;
        }
    };

    let program = match parser::parse(tokens, text.len(), &named) {
        Ok(p) => p,
        Err(report) => {
            for (message, span) in spans_from_report(&report) {
                out.push(diag(text, span, message, DiagnosticSeverity::ERROR));
            }
            return out;
        }
    };

    for err in checker::check(&program) {
        let span = err.span.unwrap_or(0..0);
        out.push(diag(text, span, err.message, DiagnosticSeverity::ERROR));
    }
    out
}

/// Extract `(label, span)` pairs from a miette::Report. Keel's lexer
/// and parser both emit LabeledSpans attached to their errors.
fn spans_from_report(report: &miette::Report) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    if let Some(labels) = report.labels() {
        for label in labels {
            let span = label.inner();
            let range = span.offset()..span.offset() + span.len();
            let msg = label
                .label()
                .map(str::to_string)
                .unwrap_or_else(|| report.to_string());
            out.push((msg, range));
        }
    }
    if out.is_empty() {
        out.push((report.to_string(), 0..0));
    }
    out
}

fn diag(text: &str, span: Span, message: String, severity: DiagnosticSeverity) -> Diagnostic {
    Diagnostic {
        range: byte_range_to_lsp(text, &span),
        severity: Some(severity),
        source: Some("keel".into()),
        message,
        ..Diagnostic::default()
    }
}

/// Convert a byte-offset range to LSP `Range` (0-based line + UTF-16
/// column). v0.1 approximates column as UTF-8 character count — fine
/// for ASCII sources; a follow-up can add true UTF-16 code-unit
/// counting for emoji-dense files.
fn byte_range_to_lsp(text: &str, span: &Span) -> Range {
    Range {
        start: offset_to_position(text, span.start),
        end: offset_to_position(text, span.end),
    }
}

fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line: u32 = 0;
    let mut col: u32 = 0;
    let mut i = 0;
    for ch in text.chars() {
        if i >= offset {
            break;
        }
        i += ch.len_utf8();
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position { line, character: col }
}
