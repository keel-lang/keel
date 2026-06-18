//! Ollama-only LLM client for Keel v0.1.
//!
//! Model resolution has no silent fallbacks: if a model can't be
//! reached or mapped, the call fails with an error that explains how
//! to fix it. `KEEL_LLM=mock` short-circuits all calls for tests.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use colored::Colorize;
use serde::{Deserialize, Serialize};

use super::context::EnvProvider;

#[derive(Debug, Clone)]
enum Provider {
    Ollama {
        base_url: String,
    },
    /// No-op provider for tests. Every primitive returns `Ok(None)` —
    /// deterministic *absence*, not failure — so `??` defaults fire in mock
    /// mode while real provider failures (network/config) still throw `AiError`.
    Mock,
}

/// LLM client for `Ai.*` namespace operations.
pub struct LlmClient {
    client: reqwest::Client,
    provider: Provider,
    model_map: HashMap<String, String>,
    ollama_default: String,
    trace: Arc<AtomicBool>,
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: Option<OllamaMessage>,
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

pub type LlmResult = Result<String, LlmError>;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// Misconfiguration the operator must fix (model not mapped). Surfaces as
    /// `AiError { reason: "provider" }`.
    #[error("{0}")]
    ConfigError(String),
    /// Provider unreachable / network or HTTP failure calling the LLM. Surfaces
    /// as `AiError { reason: "unavailable" }`.
    #[error("{0}")]
    CallFailed(String),
    /// LLM output didn't match the expected enum or schema. `got` is the raw output.
    #[error("LLM output did not match expected schema: '{got}'")]
    SchemaValidation { got: String },
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
        if env.var("KEEL_LLM").as_deref() == Some("mock") {
            return Self::mock_with_trace(trace);
        }

        let client = reqwest::Client::new();
        let model_map = Self::load_model_map(env);
        let ollama_host = env
            .var("OLLAMA_HOST")
            .unwrap_or_else(|| "http://localhost:11434".to_string());
        let ollama_default = env.var("KEEL_OLLAMA_MODEL").unwrap_or_default();

        if trace.load(Ordering::Relaxed) {
            println!(
                "  {} LLM provider: {} ({})",
                "→".dimmed(),
                "Ollama".bright_cyan(),
                ollama_host.dimmed()
            );
            for (keel_name, ollama_name) in &model_map {
                println!(
                    "     {} → {}",
                    keel_name.dimmed(),
                    ollama_name.bright_cyan()
                );
            }
            if !ollama_default.is_empty() {
                println!("     {} → {}", "*".dimmed(), ollama_default.bright_cyan());
            }
        }

        LlmClient {
            client,
            provider: Provider::Ollama {
                base_url: ollama_host,
            },
            model_map,
            ollama_default,
            trace,
        }
    }

    pub fn mock() -> Self {
        Self::mock_with_trace(Arc::new(AtomicBool::new(false)))
    }

    pub fn mock_with_trace(trace: Arc<AtomicBool>) -> Self {
        LlmClient {
            client: reqwest::Client::new(),
            provider: Provider::Mock,
            model_map: HashMap::new(),
            ollama_default: String::new(),
            trace,
        }
    }

    fn load_model_map(env: &dyn EnvProvider) -> HashMap<String, String> {
        // `KEEL_MODEL_<NAME>=<ollama_model>` maps a Keel-side model
        // alias to an Ollama tag, e.g.:
        //   KEEL_MODEL_FAST=gemma4
        //   KEEL_MODEL_SMART=mistral:7b-instruct
        let mut map = HashMap::new();
        for (key, val) in env.vars() {
            if let Some(suffix) = key.strip_prefix("KEEL_MODEL_")
                && !val.is_empty()
            {
                map.insert(suffix.to_ascii_lowercase().replace('_', "-"), val);
            }
        }
        map
    }

    pub fn describe_model(&self, model: &str) -> String {
        match &self.provider {
            Provider::Ollama { base_url } => match self.resolve_model(model) {
                Ok(name) => format!("{name} (ollama @ {base_url})"),
                Err(_) => format!("{model} (not mapped)"),
            },
            Provider::Mock => format!("{model} (mock)"),
        }
    }

    fn resolve_model<'a>(&'a self, model: &'a str) -> Result<&'a str, LlmError> {
        if let Some(stripped) = model.strip_prefix("ollama:") {
            return Ok(stripped);
        }
        if let Some(mapped) = self.model_map.get(model) {
            return Ok(mapped);
        }
        if !self.ollama_default.is_empty() {
            return Ok(&self.ollama_default);
        }
        Err(LlmError::ConfigError(format!(
            "Model '{model}' has no mapping.\n\
             Set one of:\n  \
               export KEEL_MODEL_{}=<ollama_model>\n  \
               export KEEL_OLLAMA_MODEL=<ollama_model>",
            model.to_uppercase().replace('-', "_")
        )))
    }

    async fn call(
        &self,
        role: Option<&str>,
        rules: &[String],
        system: &str,
        user: &str,
        model: &str,
    ) -> Result<Option<String>, LlmError> {
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

        match &self.provider {
            Provider::Ollama { base_url } => self
                .call_ollama(base_url, &full_system, user, model)
                .await
                .map(Some),
            // Mock yields deterministic *absence* (not failure): the prompt is
            // still built and traced above, but no provider is called, so `??`
            // and `when` handle the `none` in tests.
            Provider::Mock => Ok(None),
        }
    }

    async fn call_ollama(
        &self,
        base_url: &str,
        system: &str,
        user: &str,
        model: &str,
    ) -> LlmResult {
        let resolved = self.resolve_model(model)?;
        let url = format!("{base_url}/api/chat");
        let request = OllamaRequest {
            model: resolved.to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: system.to_string(),
                },
                ChatMessage {
                    role: "user".into(),
                    content: user.to_string(),
                },
            ],
            stream: false,
        };
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| LlmError::CallFailed(format!("Ollama unreachable at {base_url}: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::CallFailed(format!(
                "Ollama returned {status}: {body}"
            )));
        }

        let body: OllamaResponse = response
            .json()
            .await
            .map_err(|e| LlmError::CallFailed(format!("Failed to parse Ollama response: {e}")))?;

        body.message
            .and_then(|m| m.content)
            .ok_or_else(|| LlmError::CallFailed("Empty response from Ollama".into()))
    }

    // ── High-level primitives used by the Ai namespace ────────────────

    pub async fn classify(
        &self,
        role: Option<&str>,
        rules: &[String],
        input: &str,
        variants: &[String],
        criteria: &[(String, String)],
        model: &str,
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

        let Some(response) = self.call(role, rules, &system, input, model).await? else {
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
        let Some(response) = self.call(role, rules, &system, input, model).await? else {
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

        let Some(response) = self.call(role, rules, &system, description, model).await? else {
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
        let Some(response) = self.call(role, rules, &system, input, model).await? else {
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
        let Some(response) = self.call(role, rules, &system, input, model).await? else {
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
        let Some(response) = self.call(role, rules, &system, input, model).await? else {
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

    pub async fn prompt(
        &self,
        role: Option<&str>,
        rules: &[String],
        system: &str,
        user: &str,
        response_format: Option<String>,
        model: &str,
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
        let Some(response) = self.call(role, rules, &full_sys, user, model).await? else {
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

fn truncate(s: &str, max: usize) -> Cow<'_, str> {
    if s.len() > max {
        Cow::Owned(format!("{}...", &s[..max]))
    } else {
        Cow::Borrowed(s)
    }
}

#[cfg(test)]
mod tests {
    use super::{LlmClient, LlmError};
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
            .summarize(None, &[], "input", None, None, None, None, "fast-model")
            .await
            .expect("summarize should succeed")
            .expect("summary should be present");

        assert_eq!(summary, "mapped-model");
        assert!(client.describe_model("fast-model").contains("mapped-model"));
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
            .draft(None, &[], "write", None, None, None, "default")
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
            .decide(None, &[], "release?", &["ship".to_string()], "default")
            .await
            .expect("decide should succeed")
            .expect("decision should be present");

        assert_eq!(decision.0, "ship it");
        assert_eq!(decision.1, "");
    }
}
