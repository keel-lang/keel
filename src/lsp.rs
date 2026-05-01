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
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string()]),
                    ..CompletionOptions::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
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
            self.docs
                .lock()
                .unwrap()
                .insert(uri.clone(), change.text.clone());
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

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let text = match self.docs.lock().unwrap().get(&uri).cloned() {
            Some(t) => t,
            None => return Ok(None),
        };
        let offset = position_to_offset(&text, pos);
        let Some(label) = checker::type_at(&text, offset) else {
            return Ok(None);
        };
        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(label)),
            range: None,
        }))
    }

    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let mut completions = Vec::new();

        // Get prelude namespace suggestions
        let namespaces = vec![
            "Ai", "Io", "Schedule", "Email", "Http", "Env", "Log", "Agent", "Control", "Async",
            "Memory", "Search", "Db", "Time", "File", "Json", "Cache", "Str",
        ];

        for ns in namespaces {
            completions.push(CompletionItem {
                label: ns.to_string(),
                kind: Some(CompletionItemKind::MODULE),
                ..CompletionItem::default()
            });
        }

        // Get prelude methods suggestions
        let methods = vec![
            // Ai
            ("classify", "Ai method"),
            ("summarize", "Ai method"),
            ("draft", "Ai method"),
            ("extract", "Ai method"),
            ("translate", "Ai method"),
            ("decide", "Ai method"),
            ("prompt", "Ai method"),
            // Io
            ("notify", "Io method"),
            ("show", "Io method"),
            ("ask", "Io method"),
            ("confirm", "Io method"),
            // Schedule
            ("every", "Schedule method"),
            ("after", "Schedule method"),
            ("at", "Schedule method"),
            ("cron", "Schedule method"),
            ("sleep", "Schedule method"),
            // File
            ("read", "File method"),
            ("write", "File method"),
            ("exists", "File method"),
            ("list", "File method"),
            // Json
            ("parse", "Json method"),
            ("stringify", "Json method"),
            // Async
            ("spawn", "Async method"),
            ("join_all", "Async method"),
            ("select", "Async method"),
            // Control
            ("retry", "Control method"),
            ("with_timeout", "Control method"),
            ("with_deadline", "Control method"),
            // Agent
            ("run", "Agent method"),
            ("stop", "Agent method"),
            ("send", "Agent method"),
            ("delegate", "Agent method"),
            ("broadcast", "Agent method"),
            // Cache
            ("set", "Cache method"),
            ("get", "Cache method"),
            ("delete", "Cache method"),
            ("clear", "Cache method"),
            // Str
            ("match", "Str method"),
            ("extract", "Str method"),
            ("truncate", "Str method"),
            ("pad", "Str method"),
        ];

        for (method, kind) in methods {
            completions.push(CompletionItem {
                label: method.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(kind.to_string()),
                ..CompletionItem::default()
            });
        }

        // Get reserved keywords
        let keywords = vec![
            "agent",
            "task",
            "interface",
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
            "in",
            "try",
            "catch",
            "return",
            "as",
            "and",
            "or",
            "not",
            "true",
            "false",
            "none",
            "now",
            "set",
        ];

        for kw in keywords {
            completions.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..CompletionItem::default()
            });
        }

        if completions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(completions)))
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let text = match self.docs.lock().unwrap().get(&uri).cloned() {
            Some(t) => t,
            None => return Ok(None),
        };
        let offset = position_to_offset(&text, pos);
        let Some(span) = checker::definition_of(&text, offset) else {
            return Ok(None);
        };
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range: byte_range_to_lsp(&text, &span),
        })))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let text = match self.docs.lock().unwrap().get(&uri).cloned() {
            Some(t) => t,
            None => return Ok(None),
        };
        let offset = position_to_offset(&text, params.position);
        let Some(name) = checker::ident_at_offset(&text, offset) else {
            return Ok(None);
        };
        // Block renaming of prelude namespaces, built-in types, and keywords
        if matches!(
            name.as_str(),
            "Ai" | "Io"
                | "Http"
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
                | "Str"
                | "File"
                | "Json"
                | "int"
                | "float"
                | "str"
                | "bool"
                | "none"
                | "datetime"
                | "duration"
                | "true"
                | "false"
                | "run"
                | "stop"
        ) {
            return Ok(None);
        }
        // v0.1 rename is scope-unaware: only allow renaming top-level
        // declarations (task / agent / type names). Local variables would
        // require AST-level scope tracking to rename safely; until that
        // lands, decline rather than risk renaming the wrong scope.
        if checker::definition_of(&text, offset).is_none() {
            return Ok(None);
        }
        let Some(span) = checker::ident_span_at_offset(&text, offset) else {
            return Ok(None);
        };
        Ok(Some(PrepareRenameResponse::Range(byte_range_to_lsp(
            &text, &span,
        ))))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let text = match self.docs.lock().unwrap().get(&uri).cloned() {
            Some(t) => t,
            None => return Ok(None),
        };
        let offset = position_to_offset(&text, pos);
        let Some(name) = checker::ident_at_offset(&text, offset) else {
            return Ok(None);
        };
        // v0.1 rename is scope-unaware: only allow renaming top-level
        // declarations (task / agent / type names), where a file-wide
        // rename is correct. For local variables (let bindings, params,
        // etc.) `definition_of` returns `None` — decline the rename
        // rather than risk clobbering an identically-named binding in
        // another scope.
        if checker::definition_of(&text, offset).is_none() {
            return Ok(None);
        }
        let spans = checker::usages_of(&text, &name);
        if spans.is_empty() {
            return Ok(None);
        }
        let edits: Vec<TextEdit> = spans
            .iter()
            .map(|s| TextEdit {
                range: byte_range_to_lsp(&text, s),
                new_text: params.new_name.clone(),
            })
            .collect();
        let mut changes = HashMap::new();
        changes.insert(uri, edits);
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }))
    }
}

/// Convert an LSP `Position` (0-based line + UTF-8 column approximation)
/// into a UTF-8 byte offset into `text`.
fn position_to_offset(text: &str, pos: Position) -> usize {
    let mut line: u32 = 0;
    let mut col: u32 = 0;
    let mut offset: usize = 0;
    for ch in text.chars() {
        if line == pos.line && col == pos.character {
            return offset;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        offset += ch.len_utf8();
    }
    offset
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
    Position {
        line,
        character: col,
    }
}
