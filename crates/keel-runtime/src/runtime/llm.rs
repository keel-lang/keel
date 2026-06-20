// Rust guideline compliant 2026-02-21
//! High-level `ai.*` primitives over a swappable [`LlmProvider`] backend.
//!
//! [`LlmClient`] owns prompt construction (role, rules, task system text) and
//! output parsing; the chosen [`LlmProvider`] owns the transport. Model
//! resolution has no silent fallbacks. `KEEL_LLM=mock` selects the mock backend.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use colored::Colorize;

use super::context::EnvProvider;
use super::providers::anthropic::AnthropicProvider;
use super::providers::mock::MockProvider;
use super::providers::ollama::OllamaProvider;
use super::providers::openai::OpenAiProvider;
use super::providers::{CompletionRequest, LlmProvider};

pub use super::providers::LlmError;

/// Default upper bound on generated tokens when an agent sets no `@limits`.
///
/// Anthropic and OpenAI require `max_tokens`; this value is large enough for the
/// short structured outputs the `ai.*` primitives produce without truncating.
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Built-in backend names. Routing and `@provider` validation share this set.
///
/// Re-exported from [`keel_catalog`] so the checker and runtime never disagree.
pub use keel_catalog::BUILTIN_LLM_PROVIDERS as BUILTIN_PROVIDERS;

/// Dispatches `ai.*` primitives through a registry of [`LlmProvider`] backends.
///
/// A `provider:` prefix on the model tag picks the backend; a bare tag routes
/// to the program default. The registry always holds every built-in name.
pub struct LlmClient {
    backends: HashMap<&'static str, Box<dyn LlmProvider>>,
    default_provider: &'static str,
    /// A set-but-unrecognised `KEEL_PROVIDER`. Kept rather than silently ignored
    /// so the misconfiguration surfaces loudly on the first `ai.*` call instead
    /// of quietly routing to Ollama.
    provider_config_error: Option<String>,
    trace: Arc<AtomicBool>,
}

/// Resolves the program-default provider from `KEEL_PROVIDER` (defaults to
/// Ollama). A set-but-unrecognised value is a hard error rather than a silent
/// fallback — consistent with the compile-time rejection of an unknown
/// `@provider` name — surfaced as `AiError { reason: "provider" }` on first use.
fn resolve_default_provider(env: &dyn EnvProvider) -> Result<&'static str, String> {
    match env.var("KEEL_PROVIDER") {
        None => Ok("ollama"),
        Some(value) if value.is_empty() => Ok("ollama"),
        Some(value) => BUILTIN_PROVIDERS
            .into_iter()
            .find(|provider| *provider == value.as_str())
            .ok_or_else(|| {
                format!(
                    "KEEL_PROVIDER='{value}' is not a built-in provider — use one of: {}.",
                    BUILTIN_PROVIDERS.join(", ")
                )
            }),
    }
}

impl Default for LlmClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmClient {
    pub fn new() -> Self {
        Self::from_env(&super::context::NativeEnv)
    }

    pub fn from_env(env: &dyn EnvProvider) -> Self {
        let trace = env.var("KEEL_TRACE").as_deref() == Some("1");
        Self::from_env_with_trace(env, Arc::new(AtomicBool::new(trace)))
    }

    pub fn from_env_with_trace(env: &dyn EnvProvider, trace: Arc<AtomicBool>) -> Self {
        let backends = if env.var("KEEL_LLM").as_deref() == Some("mock") {
            mock_backends()
        } else {
            let mut backends: HashMap<&'static str, Box<dyn LlmProvider>> = HashMap::new();
            backends.insert("ollama", Box::new(OllamaProvider::from_env(env)));
            backends.insert("openai", Box::new(OpenAiProvider::from_env(env)));
            backends.insert("anthropic", Box::new(AnthropicProvider::from_env(env)));
            backends
        };
        let (default_provider, provider_config_error) = match resolve_default_provider(env) {
            Ok(provider) => (provider, None),
            // Keep a valid routing default; the error fires on the first call.
            Err(message) => ("ollama", Some(message)),
        };
        if trace.load(Ordering::Relaxed) && provider_config_error.is_none() {
            println!(
                "  {} LLM default provider: {}",
                "→".dimmed(),
                default_provider.bright_cyan()
            );
        }
        LlmClient {
            backends,
            default_provider,
            provider_config_error,
            trace,
        }
    }

    pub fn mock() -> Self {
        Self::mock_with_trace(Arc::new(AtomicBool::new(false)))
    }

    pub fn mock_with_trace(trace: Arc<AtomicBool>) -> Self {
        LlmClient {
            backends: mock_backends(),
            default_provider: "ollama",
            provider_config_error: None,
            trace,
        }
    }

    /// Selects the backend for `model`: a `provider:` tag prefix wins, otherwise
    /// the program default. The registry always holds every built-in name.
    fn provider_for(&self, model: &str) -> &dyn LlmProvider {
        let name = keel_catalog::builtin_provider_prefix(model).unwrap_or(self.default_provider);
        // `name` is always a built-in (a recognised prefix or the default), and
        // every constructor seeds the registry with all built-in names, so the
        // lookup cannot miss.
        let backend = self
            .backends
            .get(name)
            .expect("registry holds every built-in provider");
        &**backend
    }

    pub fn describe_model(&self, model: &str) -> String {
        self.provider_for(model).describe_model(model)
    }

    async fn call(
        &self,
        role: Option<&str>,
        rules: &[String],
        system: &str,
        user: &str,
        model: &str,
        max_tokens: Option<u32>,
    ) -> Result<Option<String>, LlmError> {
        if let Some(message) = &self.provider_config_error {
            return Err(LlmError::ConfigError(message.clone()));
        }
        let mut full_system = match role {
            Some(r) if !r.is_empty() => format!("You are {r}.\n\n"),
            _ => String::new(),
        };
        if !rules.is_empty() {
            full_system.push_str("Rules:\n");
            for rule in rules {
                full_system.push_str(&format!("- {rule}\n"));
            }
            full_system.push('\n');
        }
        full_system.push_str(system);

        if self.trace.load(Ordering::Relaxed) {
            if !rules.is_empty() {
                println!("  {} Rules injected: {}", "→".dimmed(), rules.len());
            }
            println!(
                "  {} system prompt: {}",
                "→".dimmed(),
                truncate(&full_system, 200).as_ref().dimmed()
            );
        }

        self.provider_for(model)
            .complete(CompletionRequest {
                system: full_system,
                user: user.to_string(),
                model: model.to_string(),
                max_tokens: max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            })
            .await
    }

    // ── High-level primitives used by the Ai namespace ────────────────

    #[expect(
        clippy::too_many_arguments,
        reason = "prelude API maps named Keel arguments directly into the LLM request"
    )]
    pub async fn classify(
        &self,
        role: Option<&str>,
        rules: &[String],
        input: &str,
        variants: &[String],
        criteria: &[(String, String)],
        model: &str,
        max_tokens: Option<u32>,
    ) -> Result<Option<String>, LlmError> {
        let variants_str = variants.join(", ");
        if self.trace.load(Ordering::Relaxed) {
            println!(
                "  {} Classifying as [{}] using {}",
                "→".dimmed(),
                variants_str.bright_cyan(),
                self.describe_model(model).dimmed()
            );
            println!("     input: {}", truncate(input, 80).as_ref().dimmed());
        }

        let mut system = format!(
            "You are a classifier. Classify the following input into exactly one of these \
             categories: {variants_str}. Respond with ONLY the category name."
        );
        if !criteria.is_empty() {
            system.push_str("\n\nClassification criteria:");
            for (description, variant) in criteria {
                system.push_str(&format!("\n- {description} => {variant}"));
            }
        }

        let Some(response) = self
            .call(role, rules, &system, input, model, max_tokens)
            .await?
        else {
            return Ok(None);
        };
        let cleaned = response.trim().to_lowercase();
        for variant in variants {
            let lv = variant.to_lowercase();
            if cleaned == lv || cleaned.contains(&lv) {
                if self.trace.load(Ordering::Relaxed) {
                    println!(
                        "  {} Result: {}",
                        "✓".bright_green(),
                        variant.bright_white().bold()
                    );
                }
                return Ok(Some(variant.clone()));
            }
        }
        println!(
            "  {} LLM returned '{}', no exact match",
            "⚠".bright_yellow(),
            cleaned.dimmed()
        );
        Err(LlmError::SchemaValidation {
            got: response.trim().to_string(),
        })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "prelude API maps named Keel arguments directly into the LLM request"
    )]
    pub async fn summarize(
        &self,
        role: Option<&str>,
        rules: &[String],
        input: &str,
        length: Option<(i64, String)>,
        format: Option<String>,
        max: Option<i64>,
        unit: Option<String>,
        model: &str,
        max_tokens: Option<u32>,
    ) -> Result<Option<String>, LlmError> {
        let length_instruction = match &length {
            Some((n, unit)) => format!("in {n} {unit}"),
            None => "briefly".to_string(),
        };
        if self.trace.load(Ordering::Relaxed) {
            println!(
                "  {} Summarizing {} using {}",
                "→".dimmed(),
                length_instruction.dimmed(),
                self.describe_model(model).dimmed()
            );
            println!("     input: {}", truncate(input, 80).as_ref().dimmed());
        }

        let mut system = format!(
            "You are a summarizer. Summarize the following text {length_instruction}. \
             Respond with ONLY the summary, nothing else."
        );
        match &format {
            Some(f) if f == "bullets" => {
                system.push_str(" Format your response as a bulleted list.");
            }
            Some(f) if f == "prose" => {
                system.push_str(" Format your response as flowing prose.");
            }
            _ => {}
        }
        if let Some(n) = max {
            let unit_str = unit
                .as_deref()
                .or_else(|| length.as_ref().map(|(_, u)| u.as_str()))
                .unwrap_or("items");
            system.push_str(&format!(" Use at most {n} {unit_str}."));
        }
        let Some(response) = self
            .call(role, rules, &system, input, model, max_tokens)
            .await?
        else {
            return Ok(None);
        };
        if self.trace.load(Ordering::Relaxed) {
            println!("  {} Summary ready", "✓".bright_green());
        }
        Ok(Some(response.trim().to_string()))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "prelude API maps named Keel arguments directly into the LLM request"
    )]
    pub async fn draft(
        &self,
        role: Option<&str>,
        rules: &[String],
        description: &str,
        tone: Option<&str>,
        guidance: Option<&str>,
        max_length: Option<i64>,
        model: &str,
        max_tokens: Option<u32>,
    ) -> Result<Option<String>, LlmError> {
        let tone_s = tone.unwrap_or("neutral");
        if self.trace.load(Ordering::Relaxed) {
            println!(
                "  {} Drafting ({}) using {}",
                "→".dimmed(),
                tone_s.dimmed(),
                self.describe_model(model).dimmed()
            );
            println!(
                "     prompt: {}",
                truncate(description, 80).as_ref().dimmed()
            );
        }

        let mut system =
            format!("You are a text drafter. Draft the following with a {tone_s} tone.");
        if let Some(g) = guidance {
            system.push_str(&format!("\n\nAdditional guidance: {g}"));
        }
        if let Some(n) = max_length {
            system.push_str(&format!("\n\nKeep it under {n} characters."));
        }

        let Some(response) = self
            .call(role, rules, &system, description, model, max_tokens)
            .await?
        else {
            return Ok(None);
        };
        if self.trace.load(Ordering::Relaxed) {
            println!("  {} Draft ready", "✓".bright_green());
        }
        Ok(Some(response.trim().to_string()))
    }

    pub async fn extract(
        &self,
        role: Option<&str>,
        rules: &[String],
        input: &str,
        schema: &[(String, String)],
        model: &str,
        max_tokens: Option<u32>,
    ) -> Result<Option<String>, LlmError> {
        let fields_desc: Vec<String> = schema.iter().map(|(n, t)| format!("{n}: {t}")).collect();
        if self.trace.load(Ordering::Relaxed) {
            println!(
                "  {} Extracting {{{}}} using {}",
                "→".dimmed(),
                fields_desc.join(", ").bright_cyan(),
                self.describe_model(model).dimmed()
            );
            println!("     from: {}", truncate(input, 80).as_ref().dimmed());
        }

        let system = format!(
            "You are a structured data extractor. Extract these fields from the input:\n  {}\n\n\
             Respond in JSON with exactly these field names. Use null for missing fields.",
            fields_desc.join("\n  ")
        );
        let Some(response) = self
            .call(role, rules, &system, input, model, max_tokens)
            .await?
        else {
            return Ok(None);
        };
        if self.trace.load(Ordering::Relaxed) {
            println!("  {} Extracted", "✓".bright_green());
        }
        Ok(Some(response.trim().to_string()))
    }

    pub async fn translate(
        &self,
        role: Option<&str>,
        rules: &[String],
        input: &str,
        target_langs: &[String],
        model: &str,
        max_tokens: Option<u32>,
    ) -> Result<Option<HashMap<String, String>>, LlmError> {
        let langs = target_langs.join(", ");
        if self.trace.load(Ordering::Relaxed) {
            println!(
                "  {} Translating to [{}] using {}",
                "→".dimmed(),
                langs.bright_cyan(),
                self.describe_model(model).dimmed()
            );
            println!("     input: {}", truncate(input, 80).as_ref().dimmed());
        }

        let system = if target_langs.len() == 1 {
            format!(
                "You are a translator. Translate to {}. Respond with ONLY the translation.",
                target_langs[0]
            )
        } else {
            format!(
                "You are a translator. Translate to: {langs}. \
                 Respond in JSON with language names as keys and translations as values."
            )
        };
        let Some(response) = self
            .call(role, rules, &system, input, model, max_tokens)
            .await?
        else {
            return Ok(None);
        };
        let trimmed = response.trim().to_string();
        if self.trace.load(Ordering::Relaxed) {
            println!("  {} Translated", "✓".bright_green());
        }
        // A multi-language reply that isn't valid JSON falls back to keying the
        // raw text under the first requested language. `first()` keeps that path
        // panic-free even if a caller bypasses the namespace's non-empty guard.
        if target_langs.len() == 1 {
            let mut map = HashMap::new();
            map.insert(target_langs[0].clone(), trimmed);
            Ok(Some(map))
        } else if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(&trimmed) {
            Ok(Some(parsed))
        } else if let Some(first) = target_langs.first() {
            let mut map = HashMap::new();
            map.insert(first.clone(), trimmed);
            Ok(Some(map))
        } else {
            Ok(Some(HashMap::new()))
        }
    }

    pub async fn decide(
        &self,
        role: Option<&str>,
        rules: &[String],
        input: &str,
        options: &[String],
        model: &str,
        max_tokens: Option<u32>,
    ) -> Result<Option<(String, String)>, LlmError> {
        if self.trace.load(Ordering::Relaxed) {
            println!(
                "  {} Deciding using {}",
                "→".dimmed(),
                self.describe_model(model).dimmed()
            );
            println!("     input: {}", truncate(input, 80).as_ref().dimmed());
        }

        let system = format!(
            "You are a decision maker. Choose the best option and explain briefly.\n\n\
             Options: {}\n\n\
             Respond in this exact format:\n\
             CHOICE: <option_name>\n\
             REASON: <one sentence>",
            options.join(", ")
        );
        let Some(response) = self
            .call(role, rules, &system, input, model, max_tokens)
            .await?
        else {
            return Ok(None);
        };
        let trimmed = response.trim();
        let mut choice = String::new();
        let mut reason = String::new();
        for line in trimmed.lines() {
            if let Some(c) = line.strip_prefix("CHOICE:") {
                choice = c.trim().to_string();
            } else if let Some(r) = line.strip_prefix("REASON:") {
                reason = r.trim().to_string();
            }
        }
        if choice.is_empty() {
            choice = trimmed.to_string();
        }
        if self.trace.load(Ordering::Relaxed) {
            println!(
                "  {} Decision: {}",
                "✓".bright_green(),
                choice.bright_white().bold()
            );
        }
        Ok(Some((choice, reason)))
    }

    /// Returns a clone of the internal `Arc<AtomicBool>` so callers can verify
    /// the trace flag is the same allocation shared with `RuntimeContext`.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn trace_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.trace)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "prelude API maps named Keel arguments directly into the LLM request"
    )]
    pub async fn prompt(
        &self,
        role: Option<&str>,
        rules: &[String],
        system: &str,
        user: &str,
        response_format: Option<String>,
        model: &str,
        max_tokens: Option<u32>,
    ) -> Result<Option<String>, LlmError> {
        if self.trace.load(Ordering::Relaxed) {
            println!(
                "  {} Prompt using {}",
                "→".dimmed(),
                self.describe_model(model).dimmed()
            );
        }
        let mut full_sys = system.to_string();
        if response_format.as_deref() == Some("json") {
            full_sys.push_str("\n\nRespond with valid JSON only. No prose, no markdown fences.");
        }
        let Some(response) = self
            .call(role, rules, &full_sys, user, model, max_tokens)
            .await?
        else {
            return Ok(None);
        };
        let trimmed = response.trim().to_string();
        if response_format.as_deref() == Some("json")
            && serde_json::from_str::<serde_json::Value>(&trimmed).is_err()
        {
            return Err(LlmError::SchemaValidation { got: trimmed });
        }
        if self.trace.load(Ordering::Relaxed) {
            println!("  {} Response ready", "✓".bright_green());
        }
        Ok(Some(trimmed))
    }
}

/// A registry mapping every built-in provider name to the mock backend. Shared
/// by `KEEL_LLM=mock` and the explicit `mock` constructors.
fn mock_backends() -> HashMap<&'static str, Box<dyn LlmProvider>> {
    BUILTIN_PROVIDERS
        .into_iter()
        .map(|name| (name, Box::new(MockProvider) as Box<dyn LlmProvider>))
        .collect()
}

fn truncate(s: &str, max: usize) -> Cow<'_, str> {
    if s.len() > max {
        // Back off to the nearest char boundary so a multi-byte codepoint
        // straddling `max` (common for non-ASCII trace input) never panics.
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        Cow::Owned(format!("{}...", &s[..end]))
    } else {
        Cow::Borrowed(s)
    }
}

#[cfg(test)]
mod tests {
    use super::{LlmClient, LlmError, truncate};
    use crate::runtime::context::MapEnv;
    use std::net::SocketAddr;

    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{Value, json};
    use tokio::net::TcpListener;

    #[derive(Clone)]
    enum FakeResponse {
        EchoModel,
        Content(&'static str),
        Status(StatusCode, &'static str),
        Raw(&'static str),
    }

    #[derive(Clone)]
    struct FakeOllama {
        response: FakeResponse,
    }

    async fn fake_ollama(response: FakeResponse) -> String {
        let app = Router::new()
            .route("/api/chat", post(chat))
            .with_state(FakeOllama { response });
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake Ollama server");
        let addr: SocketAddr = listener.local_addr().expect("read fake Ollama address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve fake Ollama");
        });
        format!("http://{addr}")
    }

    async fn chat(
        State(state): State<FakeOllama>,
        Json(payload): Json<Value>,
    ) -> impl IntoResponse {
        match state.response {
            FakeResponse::EchoModel => {
                let model = payload["model"].as_str().unwrap_or_default();
                Json(json!({ "message": { "content": model } })).into_response()
            }
            FakeResponse::Content(content) => {
                Json(json!({ "message": { "content": content } })).into_response()
            }
            FakeResponse::Status(status, body) => (status, body).into_response(),
            FakeResponse::Raw(body) => (StatusCode::OK, body).into_response(),
        }
    }

    fn client(base_url: &str, extra: &[(&str, &str)]) -> LlmClient {
        let mut values = vec![
            ("OLLAMA_HOST", base_url),
            ("KEEL_OLLAMA_MODEL", "default-model"),
        ];
        values.extend_from_slice(extra);
        LlmClient::from_env(&MapEnv::with(&values))
    }

    #[tokio::test]
    async fn summarize_uses_alias_model_mapping() {
        let base_url = fake_ollama(FakeResponse::EchoModel).await;
        let client = client(&base_url, &[("KEEL_MODEL_FAST_MODEL", "mapped-model")]);

        let summary = client
            .summarize(
                None,
                &[],
                "input",
                None,
                None,
                None,
                None,
                "fast-model",
                None,
            )
            .await
            .expect("summarize should succeed")
            .expect("summary should be present");

        assert_eq!(summary, "mapped-model");
        assert!(client.describe_model("fast-model").contains("mapped-model"));
    }

    #[tokio::test]
    async fn ollama_prefix_still_resolves_alias_map() {
        // Regression: an `ollama:` routing prefix (added by `@provider ollama`)
        // must not bypass the alias map — `ollama:fast-model` should resolve the
        // same as the bare `fast-model` tag, not be sent literally.
        let base_url = fake_ollama(FakeResponse::EchoModel).await;
        let client = client(&base_url, &[("KEEL_MODEL_FAST_MODEL", "mapped-model")]);

        let summary = client
            .summarize(
                None,
                &[],
                "input",
                None,
                None,
                None,
                None,
                "ollama:fast-model",
                None,
            )
            .await
            .expect("summarize should succeed")
            .expect("summary should be present");

        assert_eq!(summary, "mapped-model");
    }

    #[tokio::test]
    async fn ollama_prefix_unmapped_name_is_sent_literally() {
        // When the prefix is explicit and no alias matches, the literal tag is
        // used directly (no "has no mapping" error) — `ollama:llama3` → `llama3`.
        let base_url = fake_ollama(FakeResponse::EchoModel).await;
        let client = client(&base_url, &[]);

        let summary = client
            .summarize(
                None,
                &[],
                "input",
                None,
                None,
                None,
                None,
                "ollama:llama3",
                None,
            )
            .await
            .expect("summarize should succeed")
            .expect("summary should be present");

        assert_eq!(summary, "llama3");
    }

    #[tokio::test]
    async fn classify_validates_returned_variant() {
        let base_url = fake_ollama(FakeResponse::Content("High priority")).await;
        let client = client(&base_url, &[]);

        let variant = client
            .classify(
                Some("triage assistant"),
                &["be deterministic".to_string()],
                "ticket",
                &["low".to_string(), "high".to_string()],
                &[("urgent tickets".to_string(), "high".to_string())],
                "default",
                None,
            )
            .await
            .expect("classify should succeed");

        assert_eq!(variant.as_deref(), Some("high"));
    }

    #[tokio::test]
    async fn classify_reports_schema_validation_for_unknown_variant() {
        let base_url = fake_ollama(FakeResponse::Content("maybe")).await;
        let client = client(&base_url, &[]);

        let err = client
            .classify(
                None,
                &[],
                "ticket",
                &["low".to_string(), "high".to_string()],
                &[],
                "default",
                None,
            )
            .await
            .expect_err("unknown variant should be a schema error");

        assert!(matches!(err, LlmError::SchemaValidation { got } if got == "maybe"));
    }

    #[tokio::test]
    async fn prompt_json_rejects_non_json_content() {
        let base_url = fake_ollama(FakeResponse::Content("not json")).await;
        let client = client(&base_url, &[]);

        let err = client
            .prompt(
                None,
                &[],
                "system",
                "user",
                Some("json".to_string()),
                "default",
                None,
            )
            .await
            .expect_err("json prompt should validate response JSON");

        assert!(matches!(err, LlmError::SchemaValidation { got } if got == "not json"));
    }

    #[tokio::test]
    async fn ollama_non_success_status_is_call_failed() {
        let base_url = fake_ollama(FakeResponse::Status(
            StatusCode::INTERNAL_SERVER_ERROR,
            "model crashed",
        ))
        .await;
        let client = client(&base_url, &[]);

        let err = client
            .draft(None, &[], "write", None, None, None, "default", None)
            .await
            .expect_err("HTTP 500 should fail");

        assert!(
            matches!(err, LlmError::CallFailed(message) if message.contains("500") && message.contains("model crashed"))
        );
    }

    #[tokio::test]
    async fn malformed_ollama_response_is_call_failed() {
        let base_url = fake_ollama(FakeResponse::Raw("not-json")).await;
        let client = client(&base_url, &[]);

        let err = client
            .extract(
                None,
                &[],
                "input",
                &[("name".into(), "str".into())],
                "default",
                None,
            )
            .await
            .expect_err("malformed JSON should fail");

        assert!(
            matches!(err, LlmError::CallFailed(message) if message.contains("Failed to parse Ollama response"))
        );
    }

    #[tokio::test]
    async fn translate_multiple_languages_parses_json_map() {
        let base_url = fake_ollama(FakeResponse::Content(r#"{"fr":"bonjour","es":"hola"}"#)).await;
        let client = client(&base_url, &[]);

        let translations = client
            .translate(
                None,
                &[],
                "hello",
                &["fr".to_string(), "es".to_string()],
                "default",
                None,
            )
            .await
            .expect("translate should succeed")
            .expect("translations should be present");

        assert_eq!(translations.get("fr").map(String::as_str), Some("bonjour"));
        assert_eq!(translations.get("es").map(String::as_str), Some("hola"));
    }

    #[tokio::test]
    async fn decide_falls_back_to_full_text_when_choice_prefix_missing() {
        let base_url = fake_ollama(FakeResponse::Content("ship it")).await;
        let client = client(&base_url, &[]);

        let decision = client
            .decide(
                None,
                &[],
                "release?",
                &["ship".to_string()],
                "default",
                None,
            )
            .await
            .expect("decide should succeed")
            .expect("decision should be present");

        assert_eq!(decision.0, "ship it");
        assert_eq!(decision.1, "");
    }

    async fn spawn(router: Router) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake provider server");
        let addr: SocketAddr = listener.local_addr().expect("read fake provider address");
        tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("serve fake provider");
        });
        format!("http://{addr}")
    }

    async fn fake_openai(content: &'static str) -> String {
        let app = Router::new().route(
            "/v1/chat/completions",
            post(move || async move {
                Json(json!({ "choices": [{ "message": { "role": "assistant", "content": content } }] }))
            }),
        );
        spawn(app).await
    }

    async fn fake_anthropic(content: &'static str) -> String {
        let app = Router::new().route(
            "/v1/messages",
            post(move || async move {
                Json(json!({ "content": [{ "type": "text", "text": content }], "stop_reason": "end_turn" }))
            }),
        );
        spawn(app).await
    }

    #[tokio::test]
    async fn openai_backend_returns_choice_content() {
        let base = fake_openai("hi from openai").await;
        let client = LlmClient::from_env(&MapEnv::with(&[
            ("OPENAI_BASE_URL", base.as_str()),
            ("OPENAI_API_KEY", "sk-test"),
        ]));

        let out = client
            .prompt(None, &[], "sys", "user", None, "openai:gpt-4o", None)
            .await
            .expect("prompt should succeed")
            .expect("content should be present");

        assert_eq!(out, "hi from openai");
    }

    #[tokio::test]
    async fn anthropic_backend_extracts_text_block() {
        let base = fake_anthropic("hi from claude").await;
        let client = LlmClient::from_env(&MapEnv::with(&[
            ("ANTHROPIC_BASE_URL", base.as_str()),
            ("ANTHROPIC_API_KEY", "sk-test"),
        ]));

        let out = client
            .prompt(
                None,
                &[],
                "sys",
                "user",
                None,
                "anthropic:claude-opus-4-8",
                None,
            )
            .await
            .expect("prompt should succeed")
            .expect("content should be present");

        assert_eq!(out, "hi from claude");
    }

    #[tokio::test]
    async fn anthropic_missing_key_is_config_error() {
        let client = LlmClient::from_env(&MapEnv::with(&[(
            "ANTHROPIC_BASE_URL",
            "http://127.0.0.1:1",
        )]));

        let err = client
            .prompt(None, &[], "sys", "user", None, "anthropic:claude-x", None)
            .await
            .expect_err("missing key should be a config error");

        assert!(matches!(err, LlmError::ConfigError(m) if m.contains("ANTHROPIC_API_KEY")));
    }

    #[tokio::test]
    async fn default_provider_routes_bare_tag() {
        // `KEEL_PROVIDER=openai` routes an unprefixed model tag to OpenAI.
        let base = fake_openai("routed").await;
        let client = LlmClient::from_env(&MapEnv::with(&[
            ("KEEL_PROVIDER", "openai"),
            ("OPENAI_BASE_URL", base.as_str()),
            ("OPENAI_API_KEY", "sk-test"),
        ]));

        let out = client
            .prompt(None, &[], "s", "u", None, "gpt-4o", None)
            .await
            .expect("prompt should succeed")
            .expect("content should be present");

        assert_eq!(out, "routed");
    }

    #[tokio::test]
    async fn ollama_prefix_default_sentinel_resolves_configured_model() {
        // Regression: `@provider ollama` turns a bare `default` tag into
        // `ollama:default`. The sentinel must still resolve to KEEL_OLLAMA_MODEL
        // rather than being sent to Ollama as a model literally named "default".
        let base_url = fake_ollama(FakeResponse::EchoModel).await;
        let client = client(&base_url, &[]); // KEEL_OLLAMA_MODEL = "default-model"

        let summary = client
            .summarize(
                None,
                &[],
                "input",
                None,
                None,
                None,
                None,
                "ollama:default",
                None,
            )
            .await
            .expect("summarize should succeed")
            .expect("summary should be present");

        assert_eq!(summary, "default-model");
    }

    #[tokio::test]
    async fn unknown_keel_provider_is_config_error() {
        // A set-but-unrecognised KEEL_PROVIDER must fail loudly, not silently
        // route to Ollama.
        let client = LlmClient::from_env(&MapEnv::with(&[("KEEL_PROVIDER", "claude")]));

        let err = client
            .prompt(None, &[], "s", "u", None, "gpt-4o", None)
            .await
            .expect_err("an unknown KEEL_PROVIDER must be a config error");

        assert!(matches!(err, LlmError::ConfigError(m) if m.contains("KEEL_PROVIDER")));
    }

    #[tokio::test]
    async fn openai_default_sentinel_is_config_error() {
        // `KEEL_PROVIDER=openai` with an agent that sets no @model yields the
        // `default` sentinel; OpenAI has no default-model fallback, so this must
        // be a clear config error rather than a literal "default" model request.
        let client = LlmClient::from_env(&MapEnv::with(&[
            ("KEEL_PROVIDER", "openai"),
            ("OPENAI_API_KEY", "sk-test"),
        ]));

        let err = client
            .prompt(None, &[], "s", "u", None, "default", None)
            .await
            .expect_err("the default sentinel under openai must be a config error");

        assert!(matches!(err, LlmError::ConfigError(m) if m.contains("default model")));
    }

    #[test]
    fn truncate_backs_off_to_char_boundary() {
        // A byte cap landing mid-codepoint must not panic.
        assert_eq!(truncate("a😀b", 2).as_ref(), "a...");
        // ASCII still truncates exactly at the cap.
        assert_eq!(truncate("abcdef", 3).as_ref(), "abc...");
        // Within the cap is returned untouched.
        assert_eq!(truncate("hi", 8).as_ref(), "hi");
    }
}
