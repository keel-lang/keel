use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::lexer;
use crate::parser;
use crate::types::checker;

// ---------------------------------------------------------------------------
// LSP Backend
// ---------------------------------------------------------------------------

struct KeelLsp {
    client: Client,
}

impl KeelLsp {
    fn new(client: Client) -> Self {
        KeelLsp { client }
    }

    /// Run diagnostics on a document and publish them.
    async fn diagnose(&self, uri: Url, text: &str) {
        let named = miette::NamedSource::new(uri.as_str(), text.to_string());
        let mut diagnostics = Vec::new();

        // Lex
        let tokens = match lexer::lex(text, &named) {
            Ok(t) => t,
            Err(e) => {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("Lexer error: {e}"),
                    source: Some("keel".into()),
                    ..Default::default()
                });
                self.client
                    .publish_diagnostics(uri, diagnostics, None)
                    .await;
                return;
            }
        };

        // Parse
        let program = match parser::parse(tokens, text.len(), &named) {
            Ok(p) => p,
            Err(e) => {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("Parse error: {e}"),
                    source: Some("keel".into()),
                    ..Default::default()
                });
                self.client
                    .publish_diagnostics(uri, diagnostics, None)
                    .await;
                return;
            }
        };

        // Type check
        let type_errors = checker::check(&program);
        for err in type_errors {
            let range = if let Some(span) = err.span {
                span_to_range(text, &span)
            } else {
                Range::new(Position::new(0, 0), Position::new(0, 1))
            };

            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                message: err.message,
                source: Some("keel".into()),
                ..Default::default()
            });
        }

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

// ---------------------------------------------------------------------------
// LanguageServer trait implementation
// ---------------------------------------------------------------------------

#[tower_lsp::async_trait]
impl LanguageServer for KeelLsp {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".into(), " ".into()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "keel-lsp".into(),
                version: Some("0.1.0".into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Keel LSP initialized")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.diagnose(params.text_document.uri, &params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.first() {
            self.diagnose(params.text_document.uri, &change.text)
                .await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(text) = params.text {
            self.diagnose(params.text_document.uri, &text).await;
        }
    }

    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let items = COMPLETIONS
            .iter()
            .map(|(label, detail, kind)| CompletionItem {
                label: label.to_string(),
                detail: Some(detail.to_string()),
                kind: Some(*kind),
                insert_text: Some(label.to_string()),
                ..Default::default()
            })
            .collect();

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        // Read the word under cursor from the document
        // For now, provide keyword documentation based on common keywords
        let pos = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        // We don't have document contents cached, so provide generic hover
        // A full implementation would cache documents and look up the token
        let _ = (pos, uri);

        Ok(None)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Completions
// ---------------------------------------------------------------------------

const COMPLETIONS: &[(&str, &str, CompletionItemKind)] = &[
    // Declarations
    ("agent", "Declare an agent", CompletionItemKind::KEYWORD),
    ("task", "Declare a task", CompletionItemKind::KEYWORD),
    ("type", "Declare a type", CompletionItemKind::KEYWORD),
    ("connect", "Declare a connection", CompletionItemKind::KEYWORD),
    ("run", "Run an agent", CompletionItemKind::KEYWORD),
    // Agent fields
    ("role", "Agent role description", CompletionItemKind::KEYWORD),
    ("model", "AI model to use", CompletionItemKind::KEYWORD),
    ("tools", "Agent tools list", CompletionItemKind::KEYWORD),
    ("memory", "Memory mode: none | session | persistent", CompletionItemKind::KEYWORD),
    ("state", "Mutable agent state", CompletionItemKind::KEYWORD),
    ("config", "Agent configuration", CompletionItemKind::KEYWORD),
    ("team", "Agent team members", CompletionItemKind::KEYWORD),
    // AI primitives
    ("classify", "Classify input into an enum type", CompletionItemKind::FUNCTION),
    ("extract", "Extract structured data from text", CompletionItemKind::FUNCTION),
    ("summarize", "Summarize text content", CompletionItemKind::FUNCTION),
    ("draft", "Draft text content", CompletionItemKind::FUNCTION),
    ("translate", "Translate text to another language", CompletionItemKind::FUNCTION),
    ("decide", "Make a structured decision", CompletionItemKind::FUNCTION),
    ("prompt", "Raw LLM access", CompletionItemKind::FUNCTION),
    // AI modifiers
    ("fallback", "Default value if AI operation fails", CompletionItemKind::KEYWORD),
    ("considering", "Classification criteria hints", CompletionItemKind::KEYWORD),
    ("using", "Override model for this operation", CompletionItemKind::KEYWORD),
    // Human interaction
    ("ask", "Ask user for input (blocking)", CompletionItemKind::FUNCTION),
    ("confirm", "Ask user for yes/no confirmation", CompletionItemKind::FUNCTION),
    ("notify", "Send notification to user", CompletionItemKind::FUNCTION),
    ("show", "Display structured data to user", CompletionItemKind::FUNCTION),
    // Scheduling
    ("every", "Recurring scheduled execution", CompletionItemKind::KEYWORD),
    ("after", "Delayed one-time execution", CompletionItemKind::KEYWORD),
    ("wait", "Pause execution", CompletionItemKind::KEYWORD),
    // Control flow
    ("if", "Conditional execution", CompletionItemKind::KEYWORD),
    ("else", "Alternative branch", CompletionItemKind::KEYWORD),
    ("when", "Exhaustive pattern matching", CompletionItemKind::KEYWORD),
    ("for", "Loop over a collection", CompletionItemKind::KEYWORD),
    ("return", "Return from task", CompletionItemKind::KEYWORD),
    ("try", "Error handling block", CompletionItemKind::KEYWORD),
    ("catch", "Handle errors", CompletionItemKind::KEYWORD),
    ("retry", "Retry with backoff", CompletionItemKind::KEYWORD),
    // Data
    ("fetch", "Fetch data from URL or connection", CompletionItemKind::FUNCTION),
    ("send", "Send data to a target", CompletionItemKind::FUNCTION),
    ("archive", "Archive an item", CompletionItemKind::FUNCTION),
    ("remember", "Store in persistent memory", CompletionItemKind::FUNCTION),
    ("recall", "Retrieve from persistent memory", CompletionItemKind::FUNCTION),
    // Types
    ("str", "String type", CompletionItemKind::TYPE_PARAMETER),
    ("int", "Integer type", CompletionItemKind::TYPE_PARAMETER),
    ("float", "Float type", CompletionItemKind::TYPE_PARAMETER),
    ("bool", "Boolean type", CompletionItemKind::TYPE_PARAMETER),
    ("list", "List collection type", CompletionItemKind::TYPE_PARAMETER),
    ("map", "Map/dictionary type", CompletionItemKind::TYPE_PARAMETER),
    ("none", "Absence value", CompletionItemKind::CONSTANT),
    ("true", "Boolean true", CompletionItemKind::CONSTANT),
    ("false", "Boolean false", CompletionItemKind::CONSTANT),
    ("now", "Current timestamp", CompletionItemKind::CONSTANT),
    ("self", "Agent state access", CompletionItemKind::VARIABLE),
    ("env", "Environment variable access", CompletionItemKind::VARIABLE),
    // Duration units
    ("seconds", "Duration unit", CompletionItemKind::UNIT),
    ("minutes", "Duration unit", CompletionItemKind::UNIT),
    ("hours", "Duration unit", CompletionItemKind::UNIT),
    ("days", "Duration unit", CompletionItemKind::UNIT),
    // Memory modes
    ("persistent", "Persistent memory mode", CompletionItemKind::ENUM_MEMBER),
    ("session", "Session memory mode", CompletionItemKind::ENUM_MEMBER),
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn span_to_range(text: &str, span: &std::ops::Range<usize>) -> Range {
    let start = offset_to_position(text, span.start);
    let end = offset_to_position(text, span.end);
    Range::new(start, end)
}

fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position::new(line, col)
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub async fn start() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| KeelLsp::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
