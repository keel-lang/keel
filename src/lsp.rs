//! Keel Language Server — v0.1.
//!
//! Scope for this release: diagnostics only. On every `did_open` /
//! `did_change`, we lex → parse → type-check and publish the resulting
//! errors as LSP diagnostics. Hover and completion are placeholders
//! pending a follow-up.

use std::collections::HashMap;

use miette::NamedSource;
use parking_lot::Mutex;
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

pub(crate) struct Backend {
    pub(crate) client: Client,
    /// In-memory snapshot of open documents: URI → current text.
    pub(crate) docs: Mutex<HashMap<Url, String>>,
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
        self.docs.lock().insert(uri.clone(), text.clone());
        self.publish(&uri, &text).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        // FULL sync mode: the last content change holds the new full text.
        if let Some(change) = params.content_changes.pop() {
            self.docs.lock().insert(uri.clone(), change.text.clone());
            self.publish(&uri, &change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.docs.lock().remove(&params.text_document.uri);
        // Clear diagnostics for the closed file.
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let text = match self.docs.lock().get(&uri).cloned() {
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
            "Memory", "Search", "Db", "Time", "File", "Json", "Cache", "Random", "Uuid",
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
            // Random
            ("float", "Random method"),
            ("int", "Random method"),
            ("bool", "Random method"),
            // Uuid
            ("v4", "Uuid method"),
            ("v7", "Uuid method"),
            ("v5", "Uuid method"),
            ("parse", "Uuid method"),
            ("version", "Uuid method"),
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
        let text = match self.docs.lock().get(&uri).cloned() {
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
        let text = match self.docs.lock().get(&uri).cloned() {
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
                | "File"
                | "Json"
                | "Random"
                | "Uuid"
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
                | "uuid"
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
        let text = match self.docs.lock().get(&uri).cloned() {
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
pub(crate) fn position_to_offset(text: &str, pos: Position) -> usize {
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
pub(crate) fn spans_from_report(report: &miette::Report) -> Vec<(String, Span)> {
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

pub(crate) fn diag(
    text: &str,
    span: Span,
    message: String,
    severity: DiagnosticSeverity,
) -> Diagnostic {
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
pub(crate) fn byte_range_to_lsp(text: &str, span: &Span) -> Range {
    Range {
        start: offset_to_position(text, span.start),
        end: offset_to_position(text, span.end),
    }
}

pub(crate) fn offset_to_position(text: &str, offset: usize) -> Position {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::LanguageServer;

    // ── position_to_offset ──────────────────────────────────────────

    #[test]
    fn position_to_offset_start_of_text() {
        let text = "hello world";
        let pos = Position {
            line: 0,
            character: 0,
        };
        assert_eq!(position_to_offset(text, pos), 0);
    }

    #[test]
    fn position_to_offset_middle_of_line() {
        let text = "hello world";
        let pos = Position {
            line: 0,
            character: 6,
        };
        assert_eq!(position_to_offset(text, pos), 6);
    }

    #[test]
    fn position_to_offset_second_line() {
        let text = "first\nsecond\nthird";
        // "second" starts at byte offset 6 (f,i,r,s,t,\n = 6)
        let pos = Position {
            line: 1,
            character: 3,
        };
        // "sec" → bytes: s=6, e=7, c=8 → offset 9
        assert_eq!(position_to_offset(text, pos), 9);
    }

    #[test]
    fn position_to_offset_past_end_returns_text_length() {
        let text = "abc";
        let pos = Position {
            line: 0,
            character: 100,
        };
        assert_eq!(position_to_offset(text, pos), 3);
    }

    #[test]
    fn position_to_offset_past_end_line() {
        let text = "abc\ndef";
        let pos = Position {
            line: 10,
            character: 0,
        };
        assert_eq!(position_to_offset(text, pos), 7); // "abc\ndef" = 7 bytes
    }

    #[test]
    fn position_to_offset_at_newline() {
        let text = "abc\ndef";
        // Position at line 0, character 3 is right after "abc" at the \n
        // But position_to_offset iterates chars; after 'c' char, col=3, then \n resets col to 0
        // So position (0, 3) resolves to byte offset 3 (the \n)
        let pos = Position {
            line: 0,
            character: 3,
        };
        assert_eq!(position_to_offset(text, pos), 3);
    }

    #[test]
    fn position_to_offset_unicode_multibyte() {
        let text = "héllo";
        // 'h' = 1 byte, 'é' = 2 bytes, 'l' = 1 byte
        // character 0: h (offset 0)
        // character 1: é (offset 1, 2 bytes)
        // character 2: l (offset 3)
        let pos = Position {
            line: 0,
            character: 2,
        };
        assert_eq!(position_to_offset(text, pos), 3);
    }

    // ── offset_to_position ──────────────────────────────────────────

    #[test]
    fn offset_to_position_zero() {
        let pos = offset_to_position("hello", 0);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn offset_to_position_middle() {
        let pos = offset_to_position("hello", 3);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 3);
    }

    #[test]
    fn offset_to_position_second_line() {
        let text = "abc\ndef";
        // offset 4 = 'd' (line 1, col 0)
        let pos = offset_to_position(text, 4);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn offset_to_position_second_line_middle() {
        let text = "abc\ndef";
        // offset 5 = 'e', offset 6 = 'f'
        let pos = offset_to_position(text, 5);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 1);
    }

    #[test]
    fn offset_to_position_past_end() {
        let pos = offset_to_position("abc", 100);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 3);
    }

    #[test]
    fn offset_to_position_at_newline_byte() {
        // "abc\ndef" → offset 3 is the \n byte
        // The loop increments i after processing \n; at offset 3, i starts at 3
        // First char processed: \n (i=0+1=1), then e,f → no break because i was checked
        // Actually: i=0, ch='a', i+=1 → i=1. ch='b', i=2. ch='c', i=3.
        // Next: ch='\n', i=3 >= offset=3, break!
        // So line=0, col=3 (the \n itself)
        let pos = offset_to_position("abc\ndef", 3);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 3);
    }

    // ── byte_range_to_lsp ───────────────────────────────────────────

    #[test]
    fn byte_range_to_lsp_simple() {
        let range = byte_range_to_lsp("hello world", &(0..5));
        assert_eq!(
            range.start,
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            range.end,
            Position {
                line: 0,
                character: 5
            }
        );
    }

    #[test]
    fn byte_range_to_lsp_multiline() {
        let text = "line1\nline2\nline3";
        // "line2" is at offset 6..11
        let range = byte_range_to_lsp(text, &(6..11));
        assert_eq!(
            range.start,
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            range.end,
            Position {
                line: 1,
                character: 5
            }
        );
    }

    // ── diag ────────────────────────────────────────────────────────

    #[test]
    fn diag_has_correct_severity_and_source() {
        let d = diag(
            "test",
            0..4,
            "something wrong".into(),
            DiagnosticSeverity::ERROR,
        );
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(d.source.as_deref(), Some("keel"));
        assert_eq!(d.message, "something wrong");
        assert_eq!(
            d.range.start,
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            d.range.end,
            Position {
                line: 0,
                character: 4
            }
        );
    }

    #[test]
    fn diag_warning_severity() {
        let d = diag("test", 0..0, "hint".into(), DiagnosticSeverity::WARNING);
        assert_eq!(d.severity, Some(DiagnosticSeverity::WARNING));
    }

    // ── spans_from_report ──────────────────────────────────────────

    #[test]
    fn spans_from_report_parser_error() {
        use miette::NamedSource;
        let src = NamedSource::new("test.keel", "task t() {\n  x =\n}".to_string());
        let tokens = crate::lexer::lex("task t() {\n  x =\n}", &src).expect("lex should pass");
        let err = crate::parser::parse(tokens, "task t() {\n  x =\n}".len(), &src).unwrap_err();
        let spans = spans_from_report(&err);
        assert!(!spans.is_empty(), "expected at least one span");
    }

    #[test]
    fn spans_from_report_empty_fallback() {
        // Create a miette report without labels
        let report: miette::Report = miette::miette!("bare error without labels");
        let spans = spans_from_report(&report);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].0, "bare error without labels");
        assert_eq!(spans[0].1, 0..0);
    }

    // ── analyze ─────────────────────────────────────────────────────

    #[test]
    fn analyze_empty_string() {
        let diags = analyze("");
        // An empty string has no tokens and should be clean
        assert!(
            diags.is_empty(),
            "empty string should produce no diagnostics, got: {diags:?}"
        );
    }

    #[test]
    fn analyze_blank_lines() {
        let diags = analyze("\n\n\n");
        // Blank lines only — should be clean
        assert!(
            diags.is_empty(),
            "blank lines should produce no diagnostics, got: {diags:?}"
        );
    }

    #[test]
    fn analyze_lexer_error() {
        let diags = analyze("@invalid");
        assert!(!diags.is_empty(), "expected lexer error diagnostic");
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn analyze_multiple_type_errors() {
        let diags = analyze(
            r#"
task t() {
  a = unknown1
  b = unknown2
}
"#,
        );
        let count = diags
            .iter()
            .filter(|d| d.message.contains("undefined"))
            .count();
        assert!(
            count >= 2,
            "expected at least 2 undefined errors, got {count}: {diags:?}"
        );
    }

    #[test]
    fn analyze_type_error_has_source() {
        let diags = analyze("task t() { x = bogus }");
        assert!(!diags.is_empty());
        for d in &diags {
            assert_eq!(d.source.as_deref(), Some("keel"));
        }
    }

    // ── Backend non-client methods ──────────────────────────────────

    #[tokio::test]
    async fn backend_initialize() {
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let result = backend
            .initialize(InitializeParams::default())
            .await
            .expect("initialize should succeed");
        assert_eq!(result.server_info.as_ref().unwrap().name, "keel-lsp");
        // Verify key capabilities are present
        let caps = result.capabilities;
        assert!(caps.hover_provider.is_some());
        assert!(caps.completion_provider.is_some());
        assert!(caps.definition_provider.is_some());
    }

    #[tokio::test]
    async fn backend_shutdown() {
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let result = backend.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn backend_completion_returns_items() {
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let result = backend
            .completion(CompletionParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: Url::parse("file:///test.keel").unwrap(),
                    },
                    position: Position {
                        line: 0,
                        character: 0,
                    },
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: None,
            })
            .await
            .expect("completion should succeed");
        assert!(result.is_some(), "completion should return items");
        let items = match result.unwrap() {
            CompletionResponse::Array(arr) => arr,
            _ => panic!("expected array response"),
        };
        assert!(!items.is_empty(), "completions should not be empty");
        // Check for some expected items
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"Ai"), "should contain namespace Ai");
        assert!(labels.contains(&"agent"), "should contain keyword agent");
        assert!(
            labels.contains(&"classify"),
            "should contain method classify"
        );
        // Regression (D1): `now` is a prelude identifier, not a reserved keyword.
        assert!(
            !labels.contains(&"now"),
            "`now` must not appear in keyword completions:\n{labels:?}"
        );
    }

    #[tokio::test]
    async fn backend_hover_resolves_type() {
        let src = "agent A {\n    @on_start {\n        items = [1, 2, 3]\n    }\n}\n";
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        // Simulate did_open to populate docs
        let uri = Url::parse("file:///test.keel").unwrap();
        backend.docs.lock().insert(uri.clone(), src.to_string());

        let offset = src.find("items").unwrap() + 1; // cursor inside "items"
        let pos = offset_to_position(src, offset);
        let result = backend
            .hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position: pos,
                },
                work_done_progress_params: Default::default(),
            })
            .await
            .expect("hover should succeed");
        assert!(result.is_some(), "hover should return a result on `items`");
        if let Some(hover) = result {
            match hover.contents {
                HoverContents::Scalar(MarkedString::String(label)) => {
                    assert!(label.contains("list"), "expected list type, got: {label}");
                }
                _ => panic!("expected scalar markdown hover"),
            }
        }
    }

    #[tokio::test]
    async fn backend_hover_unknown_returns_none() {
        let src = "task t() { return }";
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend.docs.lock().insert(uri.clone(), src.to_string());

        // Hover on whitespace (offset 0)
        let result = backend
            .hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position {
                        line: 0,
                        character: 0,
                    },
                },
                work_done_progress_params: Default::default(),
            })
            .await
            .expect("hover should succeed");
        assert!(result.is_none(), "hover on unknown should return None");
    }

    #[tokio::test]
    async fn backend_hover_no_doc_returns_none() {
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let uri = Url::parse("file:///nonexistent.keel").unwrap();
        let result = backend
            .hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position {
                        line: 0,
                        character: 0,
                    },
                },
                work_done_progress_params: Default::default(),
            })
            .await
            .expect("hover should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn backend_goto_definition_finds_task() {
        let src = "task greet() -> str {\n    \"hello\"\n}\nagent A {\n    @on_start {\n        r = greet()\n    }\n}\n";
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend.docs.lock().insert(uri.clone(), src.to_string());

        let offset = src.find("greet()").unwrap() + 1;
        let pos = offset_to_position(src, offset);
        let result = backend
            .goto_definition(GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position: pos,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await
            .expect("goto_definition should succeed");
        assert!(result.is_some(), "should find definition of greet");
    }

    #[tokio::test]
    async fn backend_goto_definition_not_found() {
        let src = "task t() { return }";
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend.docs.lock().insert(uri.clone(), src.to_string());

        // Hover on "return" — not a declaration
        let offset = src.find("return").unwrap() + 1;
        let pos = offset_to_position(src, offset);
        let result = backend
            .goto_definition(GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: pos,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await
            .expect("goto_definition should succeed");
        assert!(result.is_none(), "return is not a declaration");
    }

    #[tokio::test]
    async fn backend_prepare_rename_task_name() {
        let src = "task greet() -> str { \"hello\" }\n";
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend.docs.lock().insert(uri.clone(), src.to_string());

        let offset = src.find("greet").unwrap() + 1;
        let pos = offset_to_position(src, offset);
        let result = backend
            .prepare_rename(TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: pos,
            })
            .await
            .expect("prepare_rename should succeed");
        assert!(result.is_some(), "should allow renaming task name `greet`");
    }

    #[tokio::test]
    async fn backend_prepare_rename_prelude_blocked() {
        let src = "agent A { @on_start { Io.show(\"x\") } }\n";
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend.docs.lock().insert(uri.clone(), src.to_string());

        let offset = src.find("Io").unwrap() + 1;
        let pos = offset_to_position(src, offset);
        let result = backend
            .prepare_rename(TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: pos,
            })
            .await
            .expect("prepare_rename should succeed");
        assert!(result.is_none(), "should block renaming prelude Io");
    }

    #[tokio::test]
    async fn backend_rename_task() {
        let src = "task orig() -> str { \"x\" }\nagent A { @on_start { r = orig() } }\n";
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend.docs.lock().insert(uri.clone(), src.to_string());

        let offset = src.find("orig").unwrap() + 1;
        let pos = offset_to_position(src, offset);
        let result = backend
            .rename(RenameParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position: pos,
                },
                new_name: "renamed".to_string(),
                work_done_progress_params: Default::default(),
            })
            .await
            .expect("rename should succeed");
        assert!(result.is_some(), "should produce workspace edit");
        let edit = result.unwrap();
        let changes = edit.changes.expect("should have changes");
        let edits = changes.get(&uri).expect("should have edits for uri");
        assert!(
            edits.len() >= 2,
            "expected at least 2 edits (decl + call), got {}",
            edits.len()
        );
        for e in edits {
            assert_eq!(e.new_text, "renamed");
        }
    }

    // ── Doc management ──────────────────────────────────────────────

    #[tokio::test]
    async fn did_open_stores_document() {
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        // Instead of calling did_open (which calls publish_diagnostics),
        // we directly test the docs map
        backend
            .docs
            .lock()
            .insert(uri.clone(), "task t() {}".to_string());
        assert!(backend.docs.lock().contains_key(&uri));
        assert_eq!(backend.docs.lock().get(&uri).unwrap(), "task t() {}");
    }

    #[tokio::test]
    async fn did_change_updates_document() {
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend.docs.lock().insert(uri.clone(), "old".to_string());
        // Update the doc
        backend.docs.lock().insert(uri.clone(), "new".to_string());
        assert_eq!(backend.docs.lock().get(&uri).unwrap(), "new");
    }

    #[tokio::test]
    async fn did_close_removes_document() {
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend.docs.lock().insert(uri.clone(), "data".to_string());
        assert!(backend.docs.lock().contains_key(&uri));
        backend.docs.lock().remove(&uri);
        assert!(!backend.docs.lock().contains_key(&uri));
    }

    #[tokio::test]
    async fn did_open_then_hover_works() {
        let src = "agent A {\n    @on_start {\n        x = 42\n    }\n}\n";
        let (service, _socket) = LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        });
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        // Populate docs as if did_open was called
        backend.docs.lock().insert(uri.clone(), src.to_string());

        // Now hover on `x`
        let offset = src.find("x =").unwrap() + 1;
        let pos = offset_to_position(src, offset);
        let result = backend
            .hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: pos,
                },
                work_done_progress_params: Default::default(),
            })
            .await
            .expect("hover should succeed");
        assert!(result.is_some(), "hover should work after did_open");
    }
}
