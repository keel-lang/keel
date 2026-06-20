// Rust guideline compliant 2026-02-21
//! Swappable LLM backends behind a single trait.
//!
//! [`LlmProvider`] is the seam every `ai.*` call dispatches through. The
//! high-level prompt construction and output parsing stay in
//! [`crate::runtime::llm::LlmClient`]; a provider only owns the *transport* —
//! turning a [`CompletionRequest`] into raw model output. Built-in backends
//! ([`ollama::OllamaProvider`], [`mock::MockProvider`]) implement this in Rust;
//! user-authored Keel providers are a planned follow-up (see SPEC §5.5).
//!
//! This mirrors the [`crate::runtime::db_provider::DbConnectionHandle`] pattern:
//! a `Send + Sync` trait returning boxed `'static` futures.

pub mod anthropic;
pub mod mock;
pub mod ollama;
pub mod openai;

use std::future::Future;
use std::pin::Pin;

/// Result of a provider call.
pub type LlmResult<T> = Result<T, LlmError>;

/// Future returned by [`LlmProvider`] methods.
///
/// The `'static` bound lets implementations move owned data (a cloned
/// `reqwest::Client`, owned strings) into the future without borrowing `self`.
pub type LlmFuture<T> = Pin<Box<dyn Future<Output = LlmResult<T>> + Send + 'static>>;

/// Errors surfaced by a provider, mapped to `AiError` by the `ai` namespace.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// Misconfiguration the operator must fix (model not mapped, missing key).
    /// Surfaces as `AiError { reason: "provider" }`.
    #[error("{0}")]
    ConfigError(String),
    /// Provider unreachable / network or HTTP failure calling the model.
    /// Surfaces as `AiError { reason: "unavailable" }`.
    #[error("{0}")]
    CallFailed(String),
    /// Model output didn't match the expected enum or schema. `got` is the raw output.
    #[error("LLM output did not match expected schema: '{got}'")]
    SchemaValidation { got: String },
}

/// A single model call: a built system prompt, the user content, and limits.
///
/// `system` and `user` stay separate because backends place the system prompt
/// differently — Anthropic takes a top-level `system` field, OpenAI and Ollama
/// take a `system` message role.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Fully-built system prompt (agent role, rules, and the task system text).
    pub system: String,
    /// The user content to act on.
    pub user: String,
    /// Resolved model tag, with any `provider:` prefix already stripped.
    pub model: String,
    /// Upper bound on generated tokens. Required by Anthropic and OpenAI.
    pub max_tokens: u32,
}

/// A swappable LLM backend.
///
/// Implement this to add a new model provider. The built-in implementations are
/// [`ollama::OllamaProvider`] and [`mock::MockProvider`].
pub trait LlmProvider: std::fmt::Debug + Send + Sync {
    /// Runs one completion, returning the raw output or `None` for deterministic
    /// *absence* (mock mode / no answer) — never a silent failure.
    ///
    /// # Errors
    ///
    /// Returns [`LlmError::ConfigError`] for misconfiguration and
    /// [`LlmError::CallFailed`] for transport faults.
    fn complete(&self, request: CompletionRequest) -> LlmFuture<Option<String>>;

    /// Renders a human-readable description of how `model` resolves, for tracing.
    fn describe_model(&self, model: &str) -> String;
}
