// Rust guideline compliant 2026-02-21
//! No-op backend for tests.
//!
//! Every call returns `Ok(None)` — deterministic *absence*, not failure — so
//! `??` defaults fire in mock mode while real provider failures still throw.

use crate::runtime::providers::{CompletionRequest, LlmFuture, LlmProvider};

/// A provider that never calls a model and always yields absence.
#[derive(Debug)]
pub struct MockProvider;

impl LlmProvider for MockProvider {
    fn complete(&self, _request: CompletionRequest) -> LlmFuture<Option<String>> {
        Box::pin(async { Ok(None) })
    }

    fn describe_model(&self, model: &str) -> String {
        format!("{model} (mock)")
    }
}
