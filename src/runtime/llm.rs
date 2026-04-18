//! Ollama-only LLM client for Keel v0.1.
//!
//! Model resolution has no silent fallbacks: if a model can't be
//! reached or mapped, the call fails with an error that explains how
//! to fix it. `KEEL_LLM=mock` short-circuits all calls for tests.

use std::collections::HashMap;

use colored::Colorize;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
enum Provider {
    Ollama { base_url: String },
    /// No-op provider for tests. Every call returns `CallFailed`.
    Mock,
}

/// LLM client for `Ai.*` namespace operations.
pub struct LlmClient {
    client: reqwest::Client,
    provider: Provider,
    model_map: HashMap<String, String>,
    ollama_default: String,
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

#[derive(Debug)]
pub enum LlmError {
    /// Configuration problem (model not mapped, Ollama unreachable).
    /// Execution should halt with an actionable message.
    ConfigError(String),
    /// Runtime call failure (network, parse). `fallback:` may apply.
    CallFailed(String),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::ConfigError(msg) | LlmError::CallFailed(msg) => write!(f, "{msg}"),
        }
    }
}

impl LlmClient {
    pub fn new() -> Self {
        if std::env::var("KEEL_LLM").as_deref() == Ok("mock") {
            return Self::mock();
        }

        let client = reqwest::Client::new();
        let model_map = Self::load_model_map();
        let ollama_host =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let ollama_default = std::env::var("KEEL_OLLAMA_MODEL").unwrap_or_default();

        println!(
            "  {} LLM provider: {} ({})",
            "⚡".dimmed(),
            "Ollama".bright_cyan(),
            ollama_host.dimmed()
        );
        for (keel_name, ollama_name) in &model_map {
            println!("     {} → {}", keel_name.dimmed(), ollama_name.bright_cyan());
        }
        if !ollama_default.is_empty() {
            println!("     {} → {}", "*".dimmed(), ollama_default.bright_cyan());
        }

        LlmClient {
            client,
            provider: Provider::Ollama { base_url: ollama_host },
            model_map,
            ollama_default,
        }
    }

    pub fn mock() -> Self {
        LlmClient {
            client: reqwest::Client::new(),
            provider: Provider::Mock,
            model_map: HashMap::new(),
            ollama_default: String::new(),
        }
    }

    fn load_model_map() -> HashMap<String, String> {
        // `KEEL_MODEL_<NAME>=<ollama_model>` maps a Keel-side model
        // alias to an Ollama tag, e.g.:
        //   KEEL_MODEL_FAST=gemma4
        //   KEEL_MODEL_SMART=mistral:7b-instruct
        let mut map = HashMap::new();
        for (key, val) in std::env::vars() {
            if let Some(suffix) = key.strip_prefix("KEEL_MODEL_") {
                if !val.is_empty() {
                    map.insert(suffix.to_ascii_lowercase().replace('_', "-"), val);
                }
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

    async fn call(&self, system: &str, user: &str, model: &str) -> LlmResult {
        match &self.provider {
            Provider::Ollama { base_url } => self.call_ollama(base_url, system, user, model).await,
            Provider::Mock => Err(LlmError::CallFailed("mock mode".into())),
        }
    }

    async fn call_ollama(&self, base_url: &str, system: &str, user: &str, model: &str) -> LlmResult {
        let resolved = self.resolve_model(model)?;
        let url = format!("{base_url}/api/chat");
        let request = OllamaRequest {
            model: resolved.to_string(),
            messages: vec![
                ChatMessage { role: "system".into(), content: system.to_string() },
                ChatMessage { role: "user".into(), content: user.to_string() },
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
            return Err(LlmError::CallFailed(format!("Ollama returned {status}: {body}")));
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
        input: &str,
        variants: &[String],
        criteria: &[(String, String)],
        model: &str,
    ) -> Result<Option<String>, String> {
        let variants_str = variants.join(", ");
        let preview = truncate(input, 80);
        println!(
            "  {} Classifying as [{}] using {}",
            "🤖".dimmed(),
            variants_str.bright_cyan(),
            self.describe_model(model).dimmed()
        );
        println!("     input: {}", preview.dimmed());

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

        match self.call(&system, input, model).await {
            Ok(response) => {
                let cleaned = response.trim().to_lowercase();
                for variant in variants {
                    let lv = variant.to_lowercase();
                    if cleaned == lv || cleaned.contains(&lv) {
                        println!("  {} Result: {}", "✓".bright_green(), variant.bright_white().bold());
                        return Ok(Some(variant.clone()));
                    }
                }
                println!("  {} LLM returned '{}', no exact match", "⚠".bright_yellow(), cleaned.dimmed());
                Ok(None)
            }
            Err(LlmError::ConfigError(msg)) => Err(msg),
            Err(LlmError::CallFailed(msg)) => {
                println!("  {} {}", "⚠".bright_yellow(), msg.dimmed());
                Ok(None)
            }
        }
    }

    pub async fn summarize(
        &self,
        input: &str,
        length: Option<(i64, String)>,
        model: &str,
    ) -> Result<Option<String>, String> {
        let length_instruction = match length {
            Some((n, unit)) => format!("in {n} {unit}"),
            None => "briefly".to_string(),
        };
        let preview = truncate(input, 80);
        println!(
            "  {} Summarizing {} using {}",
            "🤖".dimmed(),
            length_instruction.dimmed(),
            self.describe_model(model).dimmed()
        );
        println!("     input: {}", preview.dimmed());

        let system = format!(
            "You are a summarizer. Summarize the following text {length_instruction}. \
             Respond with ONLY the summary, nothing else."
        );
        match self.call(&system, input, model).await {
            Ok(response) => {
                println!("  {} Summary ready", "✓".bright_green());
                Ok(Some(response.trim().to_string()))
            }
            Err(LlmError::ConfigError(msg)) => Err(msg),
            Err(LlmError::CallFailed(msg)) => {
                println!("  {} {}", "⚠".bright_yellow(), msg.dimmed());
                Ok(None)
            }
        }
    }

    pub async fn draft(
        &self,
        description: &str,
        tone: Option<&str>,
        guidance: Option<&str>,
        max_length: Option<i64>,
        model: &str,
    ) -> Result<Option<String>, String> {
        let tone_s = tone.unwrap_or("neutral");
        let preview = truncate(description, 80);
        println!(
            "  {} Drafting ({}) using {}",
            "🤖".dimmed(),
            tone_s.dimmed(),
            self.describe_model(model).dimmed()
        );
        println!("     prompt: {}", preview.dimmed());

        let mut system = format!("You are a text drafter. Draft the following with a {tone_s} tone.");
        if let Some(g) = guidance {
            system.push_str(&format!("\n\nAdditional guidance: {g}"));
        }
        if let Some(n) = max_length {
            system.push_str(&format!("\n\nKeep it under {n} characters."));
        }

        match self.call(&system, description, model).await {
            Ok(response) => {
                println!("  {} Draft ready", "✓".bright_green());
                Ok(Some(response.trim().to_string()))
            }
            Err(LlmError::ConfigError(msg)) => Err(msg),
            Err(LlmError::CallFailed(msg)) => {
                println!("  {} {}", "⚠".bright_yellow(), msg.dimmed());
                Ok(None)
            }
        }
    }

    pub async fn extract(
        &self,
        input: &str,
        schema: &[(String, String)],
        model: &str,
    ) -> Result<Option<String>, String> {
        let fields_desc: Vec<String> = schema.iter().map(|(n, t)| format!("{n}: {t}")).collect();
        let preview = truncate(input, 80);
        println!(
            "  {} Extracting {{{}}} using {}",
            "🤖".dimmed(),
            fields_desc.join(", ").bright_cyan(),
            self.describe_model(model).dimmed()
        );
        println!("     from: {}", preview.dimmed());

        let system = format!(
            "You are a structured data extractor. Extract these fields from the input:\n  {}\n\n\
             Respond in JSON with exactly these field names. Use null for missing fields.",
            fields_desc.join("\n  ")
        );
        match self.call(&system, input, model).await {
            Ok(response) => {
                println!("  {} Extracted", "✓".bright_green());
                Ok(Some(response.trim().to_string()))
            }
            Err(LlmError::ConfigError(msg)) => Err(msg),
            Err(LlmError::CallFailed(msg)) => {
                println!("  {} {}", "⚠".bright_yellow(), msg.dimmed());
                Ok(None)
            }
        }
    }

    pub async fn translate(
        &self,
        input: &str,
        target_langs: &[String],
        model: &str,
    ) -> Result<Option<HashMap<String, String>>, String> {
        let preview = truncate(input, 80);
        let langs = target_langs.join(", ");
        println!(
            "  {} Translating to [{}] using {}",
            "🤖".dimmed(),
            langs.bright_cyan(),
            self.describe_model(model).dimmed()
        );
        println!("     input: {}", preview.dimmed());

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
        match self.call(&system, input, model).await {
            Ok(response) => {
                let trimmed = response.trim().to_string();
                println!("  {} Translated", "✓".bright_green());
                if target_langs.len() == 1 {
                    let mut map = HashMap::new();
                    map.insert(target_langs[0].clone(), trimmed);
                    Ok(Some(map))
                } else if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(&trimmed) {
                    Ok(Some(parsed))
                } else {
                    let mut map = HashMap::new();
                    map.insert(target_langs[0].clone(), trimmed);
                    Ok(Some(map))
                }
            }
            Err(LlmError::ConfigError(msg)) => Err(msg),
            Err(LlmError::CallFailed(msg)) => {
                println!("  {} {}", "⚠".bright_yellow(), msg.dimmed());
                Ok(None)
            }
        }
    }

    pub async fn decide(
        &self,
        input: &str,
        options: &[String],
        model: &str,
    ) -> Result<Option<(String, String)>, String> {
        let preview = truncate(input, 80);
        println!(
            "  {} Deciding using {}",
            "🤖".dimmed(),
            self.describe_model(model).dimmed()
        );
        println!("     input: {}", preview.dimmed());

        let system = format!(
            "You are a decision maker. Choose the best option and explain briefly.\n\n\
             Options: {}\n\n\
             Respond in this exact format:\n\
             CHOICE: <option_name>\n\
             REASON: <one sentence>",
            options.join(", ")
        );
        match self.call(&system, input, model).await {
            Ok(response) => {
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
                println!("  {} Decision: {}", "✓".bright_green(), choice.bright_white().bold());
                Ok(Some((choice, reason)))
            }
            Err(LlmError::ConfigError(msg)) => Err(msg),
            Err(LlmError::CallFailed(msg)) => {
                println!("  {} {}", "⚠".bright_yellow(), msg.dimmed());
                Ok(None)
            }
        }
    }

    pub async fn prompt(
        &self,
        system: &str,
        user: &str,
        model: &str,
    ) -> Result<Option<String>, String> {
        println!(
            "  {} Prompt using {}",
            "🤖".dimmed(),
            self.describe_model(model).dimmed()
        );
        match self.call(system, user, model).await {
            Ok(response) => {
                println!("  {} Response ready", "✓".bright_green());
                Ok(Some(response.trim().to_string()))
            }
            Err(LlmError::ConfigError(msg)) => Err(msg),
            Err(LlmError::CallFailed(msg)) => {
                println!("  {} {}", "⚠".bright_yellow(), msg.dimmed());
                Ok(None)
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s.to_string()
    }
}
