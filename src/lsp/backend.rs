//! LSP backend — [`Backend`] state and all [`LanguageServer`] protocol handlers.

use std::collections::HashMap;

use parking_lot::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::types::checker;

use super::completion::completion_items;
use super::definition::find_definition;
use super::diagnostics::{SemanticIndex, analyze_document};
use super::hover::resolve_hover_type;
use super::position::{byte_range_to_lsp, position_to_offset};
use super::rename::{get_usages, is_rename_blocked, is_top_level_at_offset};

// ---------------------------------------------------------------------------
// Document state
// ---------------------------------------------------------------------------

/// Per-document state: the current text, its version counter, cached LSP
/// diagnostics, and the semantic index built at parse/check time.
#[derive(Clone)]
pub(crate) struct DocumentState {
    pub(crate) text: String,
    /// Monotonically increasing version counter from the LSP client.
    /// Reserved for incremental-sync support in a future release.
    #[allow(dead_code)]
    pub(crate) version: i32,
    /// LSP diagnostics from the last analysis.
    /// Cached here so they can be re-published without re-running analysis.
    #[allow(dead_code)]
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// Semantic index for zero-reparse hover / definition / rename.
    /// `None` when the last parse failed; handlers fall back to reparsing.
    pub(crate) semantic_index: Option<SemanticIndex>,
}

#[cfg(test)]
impl DocumentState {
    /// Analyse `text` synchronously and build the complete document state.
    /// Used in tests to populate the docs map with a pre-built semantic index.
    pub(crate) fn analyzed(text: String) -> Self {
        let (diagnostics, semantic_index) = analyze_document(&text);
        Self {
            text,
            version: 0,
            diagnostics,
            semantic_index,
        }
    }

    /// Construct a text-only document state with no pre-built index.
    /// Handlers fall back to reparse-on-demand when the semantic index is absent.
    pub(crate) fn text_only(text: String) -> Self {
        Self {
            text,
            version: 0,
            diagnostics: vec![],
            semantic_index: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

pub(crate) struct Backend {
    pub(crate) client: Client,
    /// Open documents: URI → current document state.
    pub(crate) docs: Mutex<HashMap<Url, DocumentState>>,
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
        let version = params.text_document.version;
        let (diagnostics, semantic_index) = analyze_document(&text);
        self.docs.lock().insert(
            uri.clone(),
            DocumentState {
                text,
                version,
                diagnostics: diagnostics.clone(),
                semantic_index,
            },
        );
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let version = params.text_document.version;
        // FULL sync mode: the last content change holds the new full text.
        if let Some(change) = params.content_changes.pop() {
            let text = change.text;
            let (diagnostics, semantic_index) = analyze_document(&text);
            self.docs.lock().insert(
                uri.clone(),
                DocumentState {
                    text,
                    version,
                    diagnostics: diagnostics.clone(),
                    semantic_index,
                },
            );
            self.client
                .publish_diagnostics(uri, diagnostics, None)
                .await;
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
        let doc = match self.docs.lock().get(&uri).cloned() {
            Some(d) => d,
            None => return Ok(None),
        };
        let offset = position_to_offset(&doc.text, pos);
        let Some(label) = resolve_hover_type(&doc.text, offset, doc.semantic_index.as_ref()) else {
            return Ok(None);
        };
        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(label)),
            range: None,
        }))
    }

    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let completions = completion_items();
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
        let doc = match self.docs.lock().get(&uri).cloned() {
            Some(d) => d,
            None => return Ok(None),
        };
        let offset = position_to_offset(&doc.text, pos);
        let Some(span) = find_definition(&doc.text, offset, doc.semantic_index.as_ref()) else {
            return Ok(None);
        };
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range: byte_range_to_lsp(&doc.text, &span),
        })))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let doc = match self.docs.lock().get(&uri).cloned() {
            Some(d) => d,
            None => return Ok(None),
        };
        let offset = position_to_offset(&doc.text, params.position);
        let Some(name) = checker::ident_at_offset(&doc.text, offset) else {
            return Ok(None);
        };
        if is_rename_blocked(&name) {
            return Ok(None);
        }
        // v0.1 rename is scope-unaware: only allow renaming top-level
        // declarations (task / agent / type names). Local variables would
        // require scope-aware HIR edits to rename safely; until that
        // lands, decline rather than risk renaming the wrong scope.
        if !is_top_level_at_offset(&doc.text, offset, doc.semantic_index.as_ref()) {
            return Ok(None);
        }
        let Some(span) = checker::ident_span_at_offset(&doc.text, offset) else {
            return Ok(None);
        };
        Ok(Some(PrepareRenameResponse::Range(byte_range_to_lsp(
            &doc.text, &span,
        ))))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let doc = match self.docs.lock().get(&uri).cloned() {
            Some(d) => d,
            None => return Ok(None),
        };
        let offset = position_to_offset(&doc.text, pos);
        let Some(name) = checker::ident_at_offset(&doc.text, offset) else {
            return Ok(None);
        };
        // v0.1 rename is scope-unaware: only allow renaming top-level
        // declarations (task / agent / type names), where a file-wide
        // rename is correct.
        if !is_top_level_at_offset(&doc.text, offset, doc.semantic_index.as_ref()) {
            return Ok(None);
        }
        let spans = get_usages(&doc.text, &name, doc.semantic_index.as_ref());
        if spans.is_empty() {
            return Ok(None);
        }
        let edits: Vec<TextEdit> = spans
            .iter()
            .map(|s| TextEdit {
                range: byte_range_to_lsp(&doc.text, s),
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

#[cfg(test)]
mod tests {
    use super::super::position::offset_to_position;
    use super::*;
    use tower_lsp::{LanguageServer, LspService};

    fn make_backend() -> (LspService<Backend>, tower_lsp::ClientSocket) {
        LspService::new(|client| Backend {
            client,
            docs: Mutex::new(HashMap::new()),
        })
    }

    // ── Backend non-client methods ──────────────────────────────────

    #[tokio::test]
    async fn backend_initialize() {
        let (service, _socket) = make_backend();
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
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let result = backend.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn backend_completion_returns_items() {
        let (service, _socket) = make_backend();
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
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"Ai"), "should contain namespace Ai");
        assert!(labels.contains(&"agent"), "should contain keyword agent");
        assert!(
            labels.contains(&"classify"),
            "should contain method classify"
        );
        // Regression (D1): `now` is a method (Time.now), not a reserved keyword.
        let now_as_keyword = items
            .iter()
            .any(|i| i.label == "now" && i.kind == Some(CompletionItemKind::KEYWORD));
        assert!(
            !now_as_keyword,
            "`now` must not appear as a keyword completion:\n{labels:?}"
        );
        let now_as_method = items
            .iter()
            .any(|i| i.label == "now" && i.kind == Some(CompletionItemKind::FUNCTION));
        assert!(
            now_as_method,
            "`now` should appear as a method completion (Time.now)"
        );
    }

    #[tokio::test]
    async fn backend_hover_resolves_type() {
        let src = "agent A {\n    @on_start {\n        items = [1, 2, 3]\n    }\n}\n";
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

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
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

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
        let (service, _socket) = make_backend();
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
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

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
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

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
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

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
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

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
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

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
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        // Instead of calling did_open (which calls publish_diagnostics),
        // we directly test the docs map.
        backend.docs.lock().insert(
            uri.clone(),
            DocumentState::text_only("task t() {}".to_string()),
        );
        assert!(backend.docs.lock().contains_key(&uri));
        assert_eq!(backend.docs.lock().get(&uri).unwrap().text, "task t() {}");
    }

    #[tokio::test]
    async fn did_change_updates_document() {
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::text_only("old".to_string()));
        // Update the doc
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::text_only("new".to_string()));
        assert_eq!(backend.docs.lock().get(&uri).unwrap().text, "new");
    }

    #[tokio::test]
    async fn did_close_removes_document() {
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::text_only("data".to_string()));
        assert!(backend.docs.lock().contains_key(&uri));
        backend.docs.lock().remove(&uri);
        assert!(!backend.docs.lock().contains_key(&uri));
    }

    // ── Span-keyed semantic index tests ────────────────────────────

    /// Hover on a reference site (RHS of a let) exercises the span_types
    /// fast path — not the name_types declaration-site fallback.
    #[tokio::test]
    async fn hover_reference_uses_span_keyed_index() {
        // `xs` on the RHS of `result = xs` is an identifier reference that
        // the HIR records in `references`.  The span_types path resolves it
        // scope-correctly through the SymbolId.
        let src =
            "agent A {\n    @on_start {\n        xs = [1, 2, 3]\n        result = xs\n    }\n}\n";
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

        // `rfind("xs")` locates the last occurrence — the reference in `result = xs`.
        let ref_offset = src.rfind("xs").unwrap() + 1;
        let pos = offset_to_position(src, ref_offset);
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
        assert!(
            result.is_some(),
            "hover on reference `xs` should return a type"
        );
        let label = match result.unwrap().contents {
            HoverContents::Scalar(MarkedString::String(l)) => l,
            _ => panic!("expected scalar hover"),
        };
        assert!(
            label.contains("list"),
            "expected list type for xs reference, got: {label}"
        );
    }

    /// Hover on shadowed references is scope-correct: the inner shadow and
    /// the outer binding produce different types at the right reference sites.
    #[tokio::test]
    async fn hover_shadowed_reference_is_scope_correct() {
        // `s = "hi"` (outer, str) then `s = 42` inside an `if` (inner, int).
        // Hovering the `s` in `inner = s` must show int; the `s` in `outer = s`
        // must show str.  A name-keyed implementation would give the same
        // answer for both (last-write-wins); only the span-keyed path is correct.
        let src = concat!(
            "task t() {\n",
            "  s = \"hi\"\n",
            "  if true {\n",
            "    s = 42\n",
            "    inner = s\n",
            "  }\n",
            "  outer = s\n",
            "}\n",
        );
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

        let hover_at = |label: &str| {
            // Each `inner = s` and `outer = s` are unique — find the `s` reference.
            let needle = format!("{label} = s");
            let base = src.find(needle.as_str()).unwrap();
            base + needle.len() - 1 // offset of the `s`
        };

        let hover = |offset: usize| {
            let pos = offset_to_position(src, offset);
            backend.hover(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: Url::parse("file:///test.keel").unwrap(),
                    },
                    position: pos,
                },
                work_done_progress_params: Default::default(),
            })
        };

        let inner_result = hover(hover_at("inner")).await.expect("hover succeeded");
        let outer_result = hover(hover_at("outer")).await.expect("hover succeeded");

        let label_of = |r: Option<Hover>| match r?.contents {
            HoverContents::Scalar(MarkedString::String(l)) => Some(l),
            _ => None,
        };

        let inner_label = label_of(inner_result).expect("inner s should have a type");
        let outer_label = label_of(outer_result).expect("outer s should have a type");

        assert!(
            inner_label.contains("int"),
            "inner `s` (= 42) should be int, got: {inner_label}"
        );
        assert!(
            outer_label.contains("str"),
            "outer `s` (= \"hi\") should be str, got: {outer_label}"
        );
    }

    #[tokio::test]
    async fn did_open_then_hover_works() {
        let src = "agent A {\n    @on_start {\n        x = 42\n    }\n}\n";
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

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

    // ── Regression: issues 1 + 2 (namespace_names vs prelude_names) ──

    /// Hovering a primitive type annotation must return "type `X`", not
    /// "namespace `X`".  Regressed when `prelude_names()` (which includes
    /// primitives) was used in `type_at` before the specific primitive check.
    #[tokio::test]
    async fn hover_primitive_type_annotation_returns_type_label() {
        // `n: int` — cursor on the type annotation token `int`.
        let src = "task t(n: int) -> str { \"ok\" }\n";
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

        let offset = src.find("int").unwrap() + 1;
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
        let label = match result {
            Some(h) => match h.contents {
                HoverContents::Scalar(MarkedString::String(l)) => l,
                _ => panic!("expected scalar hover"),
            },
            None => panic!("hover on `int` should return Some"),
        };
        assert_eq!(
            label, "type `int`",
            "primitive type annotation must label as 'type', got: {label}"
        );
    }

    /// A task whose name happens to be a symbol-hint keyword (`json`, `session`,
    /// etc.) must still be renameable.  Regressed when `prelude_names()` (which
    /// includes symbol-hint words) was used in `is_rename_blocked`.
    #[tokio::test]
    async fn prepare_rename_non_namespace_prelude_word_is_allowed() {
        // `json` is in prelude_names() as a symbol hint but is NOT a namespace.
        let src = "task json() -> str { \"{}\" }\n";
        let (service, _socket) = make_backend();
        let backend = service.inner();
        let uri = Url::parse("file:///test.keel").unwrap();
        backend
            .docs
            .lock()
            .insert(uri.clone(), DocumentState::analyzed(src.to_string()));

        let offset = src.find("json").unwrap() + 1;
        let pos = offset_to_position(src, offset);
        let result = backend
            .prepare_rename(TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: pos,
            })
            .await
            .expect("prepare_rename should succeed");
        assert!(
            result.is_some(),
            "task named `json` must be renameable (it is not a namespace)"
        );
    }
}
