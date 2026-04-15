use std::collections::HashMap;

use colored::Colorize;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Provider {
    Anthropic { api_key: String },
    Ollama { base_url: String },
    /// Mock provider — only available via LlmClient::mock() for tests.
    Mock,
}

/// LLM client for AI primitive operations.
///
/// Strict model resolution — no silent fallbacks:
///   - If a model can't be reached, the call fails with an error.
///   - Cloud model names (claude-*, gpt-*) on Ollama without mapping → error.
///   - No ANTHROPIC_API_KEY + cloud model → error.
///   - Mock mode is only for tests (LlmClient::mock()).
pub struct LlmClient {
    client: reqwest::Client,
    provider: Provider,
    model_map: HashMap<String, String>,
    ollama_default: String,
}

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ChatMessage>,
    system: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: Option<String>,
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

// ---------------------------------------------------------------------------
// Result type: distinguish config errors from LLM response issues
// ---------------------------------------------------------------------------

type LlmResult = Result<String, LlmError>;

#[derive(Debug)]
pub enum LlmError {
    /// Model not available — configuration problem, should halt execution.
    ConfigError(String),
    /// LLM call failed at network/parse level — fallback may apply.
    CallFailed(String),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::ConfigError(msg) => write!(f, "{msg}"),
            LlmError::CallFailed(msg) => write!(f, "{msg}"),
        }
    }
}

impl LlmClient {
    pub fn new() -> Self {
        // KEEL_LLM=mock → test mode, no real LLM calls
        if std::env::var("KEEL_LLM").as_deref() == Ok("mock") {
            return Self::mock();
        }

        let client = reqwest::Client::new();
        let model_map = Self::load_model_map();

        // 1. Anthropic API key
        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            if !api_key.is_empty() {
                println!(
                    "  {} LLM provider: {}",
                    "⚡".dimmed(),
                    "Anthropic API".bright_cyan()
                );
                return LlmClient {
                    client,
                    provider: Provider::Anthropic { api_key },
                    model_map,
                    ollama_default: String::new(),
                };
            }
        }

        // 2. Ollama
        let ollama_host =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let ollama_default = std::env::var("KEEL_OLLAMA_MODEL").unwrap_or_default();

        println!(
            "  {} LLM provider: {} ({})",
            "⚡".dimmed(),
            "Ollama".bright_cyan(),
            ollama_host.dimmed()
        );
        if !model_map.is_empty() {
            for (keel_name, ollama_name) in &model_map {
                println!("     {} → {}", keel_name.dimmed(), ollama_name.bright_cyan());
            }
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

    /// Mock client — for tests only. All calls return Err(CallFailed).
    pub fn mock() -> Self {
        LlmClient {
            client: reqwest::Client::new(),
            provider: Provider::Mock,
            model_map: HashMap::new(),
            ollama_default: String::new(),
        }
    }

    fn load_model_map() -> HashMap<String, String> {
        let mut map = HashMap::new();
        let mappings = [
            ("KEEL_MODEL_CLAUDE_HAIKU", "claude-haiku"),
            ("KEEL_MODEL_CLAUDE_SONNET", "claude-sonnet"),
            ("KEEL_MODEL_CLAUDE_OPUS", "claude-opus"),
        ];
        for (env_var, keel_name) in mappings {
            if let Ok(val) = std::env::var(env_var) {
                if !val.is_empty() {
                    map.insert(keel_name.to_string(), val);
                }
            }
        }
        map
    }

    // ── Model resolution ─────────────────────────────────────────

    pub fn describe_model(&self, model: &str) -> String {
        match &self.provider {
            Provider::Anthropic { .. } => {
                format!("{} (anthropic)", Self::resolve_anthropic_model(model))
            }
            Provider::Ollama { base_url } => {
                let resolved = self.resolve_ollama_model(model);
                match resolved {
                    Ok(name) => format!("{name} (ollama @ {base_url})"),
                    Err(_) => format!("{model} (not mapped)"),
                }
            }
            Provider::Mock => format!("{model} (mock)"),
        }
    }

    fn resolve_anthropic_model(model: &str) -> &str {
        match model {
            "claude-haiku" => "claude-haiku-4-5-20251001",
            "claude-sonnet" => "claude-sonnet-4-6-20260415",
            "claude-opus" => "claude-opus-4-6-20260415",
            _ => model,
        }
    }

    fn resolve_ollama_model<'a>(&'a self, model: &'a str) -> Result<&'a str, LlmError> {
        // 1. Explicit prefix: "ollama:gemma4" → "gemma4"
        if let Some(stripped) = model.strip_prefix("ollama:") {
            return Ok(stripped);
        }
        // 2. Per-model env var: KEEL_MODEL_CLAUDE_HAIKU=gemma4
        if let Some(mapped) = self.model_map.get(model) {
            return Ok(mapped);
        }
        // 3. Catch-all: KEEL_OLLAMA_MODEL
        if !self.ollama_default.is_empty() {
            return Ok(&self.ollama_default);
        }
        // 4. Cloud model name without mapping → error
        if Self::is_cloud_model(model) {
            let env_hint = model.to_uppercase().replace('-', "_");
            return Err(LlmError::ConfigError(format!(
                "Model '{model}' is not available locally.\n\
                 Set one of:\n  \
                   export KEEL_MODEL_{env_hint}=<ollama_model>\n  \
                   export KEEL_OLLAMA_MODEL=<ollama_model>"
            )));
        }
        // 5. Non-cloud name → pass through (user might have it in Ollama)
        Ok(model)
    }

    fn is_cloud_model(name: &str) -> bool {
        name.starts_with("claude-")
            || name.starts_with("gpt-")
            || name.starts_with("gemini-")
    }

    // ── Core call ────────────────────────────────────────────────

    async fn call(&self, system: &str, user: &str, model: &str) -> LlmResult {
        match &self.provider {
            Provider::Anthropic { api_key } => {
                self.call_anthropic(api_key, system, user, model).await
            }
            Provider::Ollama { base_url } => {
                self.call_ollama(base_url, system, user, model).await
            }
            Provider::Mock => Err(LlmError::CallFailed("mock mode".into())),
        }
    }

    async fn call_anthropic(
        &self,
        api_key: &str,
        system: &str,
        user: &str,
        model: &str,
    ) -> LlmResult {
        let resolved = Self::resolve_anthropic_model(model);
        let request = AnthropicRequest {
            model: resolved.to_string(),
            max_tokens: 1024,
            system: Some(system.to_string()),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: user.to_string(),
            }],
        };

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| LlmError::CallFailed(format!("Anthropic API error: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::CallFailed(format!(
                "Anthropic API returned {status}: {body}"
            )));
        }

        let body: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| LlmError::CallFailed(format!("Failed to parse Anthropic response: {e}")))?;

        body.content
            .first()
            .and_then(|c| c.text.clone())
            .ok_or_else(|| LlmError::CallFailed("Empty response from Anthropic".into()))
    }

    async fn call_ollama(
        &self,
        base_url: &str,
        system: &str,
        user: &str,
        model: &str,
    ) -> LlmResult {
        let resolved = self.resolve_ollama_model(model)?;
        let url = format!("{base_url}/api/chat");

        let request = OllamaRequest {
            model: resolved.to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
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
            .map_err(|e| {
                LlmError::CallFailed(format!("Ollama unreachable at {base_url}: {e}"))
            })?;

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

    // ── AI primitives ────────────────────────────────────────────
    //
    // Return type: Result<Option<String>, String>
    //   Ok(Some(val))  — LLM responded, parsed successfully
    //   Ok(None)       — LLM responded, but no valid match (fallback applies)
    //   Err(msg)       — hard error, execution should stop

    pub async fn classify(
        &self,
        input: &str,
        variants: &[String],
        criteria: Option<&[(String, String)]>,
        model: &str,
    ) -> Result<Option<String>, String> {
        let variants_str = variants.join(", ");
        let display_model = self.describe_model(model);
        let preview = truncate(input, 80);

        println!(
            "  {} Classifying as [{}] using {}",
            "🤖".dimmed(),
            variants_str.bright_cyan(),
            display_model.dimmed()
        );
        println!("     input: {}", preview.dimmed());

        let mut system = format!(
            "You are a classifier. Classify the following input into exactly one of these categories: {variants_str}. \
             Respond with ONLY the category name, nothing else."
        );
        if let Some(criteria) = criteria {
            system.push_str("\n\nClassification criteria:");
            for (description, variant) in criteria {
                system.push_str(&format!("\n- {description} => {variant}"));
            }
        }

        match self.call(&system, input, model).await {
            Ok(response) => {
                let cleaned = response.trim().to_lowercase();
                for variant in variants {
                    if cleaned == variant.to_lowercase()
                        || cleaned.contains(&variant.to_lowercase())
                    {
                        println!(
                            "  {} Result: {}",
                            "✓".bright_green(),
                            variant.bright_white().bold()
                        );
                        return Ok(Some(variant.clone()));
                    }
                }
                println!(
                    "  {} LLM returned '{}', no exact match",
                    "⚠".bright_yellow(),
                    cleaned.dimmed()
                );
                Ok(None)
            }
            Err(LlmError::ConfigError(msg)) => {
                eprintln!("  {} {}", "✗".bright_red(), msg);
                Err(msg)
            }
            Err(LlmError::CallFailed(msg)) => {
                println!("  {} {}", "⚠".bright_yellow(), msg.dimmed());
                Ok(None)
            }
        }
    }

    pub async fn summarize(
        &self,
        input: &str,
        length: Option<&(i64, String)>,
        model: &str,
    ) -> Result<Option<String>, String> {
        let length_instruction = match length {
            Some((n, unit)) => format!("in {n} {unit}"),
            None => "briefly".to_string(),
        };
        let display_model = self.describe_model(model);
        let preview = truncate(input, 80);

        println!(
            "  {} Summarizing {} using {}",
            "🤖".dimmed(),
            length_instruction.dimmed(),
            display_model.dimmed()
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
            Err(LlmError::ConfigError(msg)) => {
                eprintln!("  {} {}", "✗".bright_red(), msg);
                Err(msg)
            }
            Err(LlmError::CallFailed(msg)) => {
                println!("  {} {}", "⚠".bright_yellow(), msg.dimmed());
                Ok(None)
            }
        }
    }

    pub async fn draft(
        &self,
        description: &str,
        options: &HashMap<String, String>,
        model: &str,
    ) -> Result<Option<String>, String> {
        let tone = options.get("tone").map(|s| s.as_str()).unwrap_or("neutral");
        let display_model = self.describe_model(model);
        let preview = truncate(description, 80);

        println!(
            "  {} Drafting ({}) using {}",
            "🤖".dimmed(),
            tone.dimmed(),
            display_model.dimmed()
        );
        println!("     prompt: {}", preview.dimmed());

        let mut system = format!(
            "You are a text drafter. Draft the following with a {tone} tone."
        );
        if let Some(guidance) = options.get("guidance") {
            system.push_str(&format!("\n\nAdditional guidance: {guidance}"));
        }
        if let Some(max_len) = options.get("max_length") {
            system.push_str(&format!("\n\nKeep it under {max_len} characters."));
        }

        match self.call(&system, description, model).await {
            Ok(response) => {
                println!("  {} Draft ready", "✓".bright_green());
                Ok(Some(response.trim().to_string()))
            }
            Err(LlmError::ConfigError(msg)) => {
                eprintln!("  {} {}", "✗".bright_red(), msg);
                Err(msg)
            }
            Err(LlmError::CallFailed(msg)) => {
                println!("  {} {}", "⚠".bright_yellow(), msg.dimmed());
                Ok(None)
            }
        }
    }

    pub async fn extract(
        &self,
        input: &str,
        fields: &[(String, String)],
        model: &str,
    ) -> Result<Option<String>, String> {
        let display_model = self.describe_model(model);
        let preview = truncate(input, 80);
        let schema: Vec<String> = fields.iter().map(|(n, t)| format!("{n}: {t}")).collect();

        println!(
            "  {} Extracting {{{}}} using {}",
            "🤖".dimmed(),
            schema.join(", ").bright_cyan(),
            display_model.dimmed()
        );
        println!("     from: {}", preview.dimmed());

        let fields_desc = schema.join("\n  ");
        let system = format!(
            "You are a structured data extractor. Extract the following fields from the input text.\n\
             Fields:\n  {fields_desc}\n\n\
             Respond in JSON format with exactly these field names. \
             If a field cannot be found, use null."
        );

        match self.call(&system, input, model).await {
            Ok(response) => {
                println!("  {} Extracted", "✓".bright_green());
                Ok(Some(response.trim().to_string()))
            }
            Err(LlmError::ConfigError(msg)) => {
                eprintln!("  {} {}", "✗".bright_red(), msg);
                Err(msg)
            }
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
        let display_model = self.describe_model(model);
        let preview = truncate(input, 80);
        let langs = target_langs.join(", ");

        println!(
            "  {} Translating to [{}] using {}",
            "🤖".dimmed(),
            langs.bright_cyan(),
            display_model.dimmed()
        );
        println!("     input: {}", preview.dimmed());

        let system = if target_langs.len() == 1 {
            format!(
                "You are a translator. Translate the following text to {}. \
                 Respond with ONLY the translation, nothing else.",
                target_langs[0]
            )
        } else {
            format!(
                "You are a translator. Translate the following text to each of these languages: {langs}. \
                 Respond in JSON format with language names as keys and translations as values."
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
                } else {
                    // Try parsing as JSON
                    if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(&trimmed) {
                        Ok(Some(parsed))
                    } else {
                        let mut map = HashMap::new();
                        map.insert(target_langs[0].clone(), trimmed);
                        Ok(Some(map))
                    }
                }
            }
            Err(LlmError::ConfigError(msg)) => {
                eprintln!("  {} {}", "✗".bright_red(), msg);
                Err(msg)
            }
            Err(LlmError::CallFailed(msg)) => {
                println!("  {} {}", "⚠".bright_yellow(), msg.dimmed());
                Ok(None)
            }
        }
    }

    pub async fn decide(
        &self,
        input: &str,
        options_desc: &HashMap<String, String>,
        model: &str,
    ) -> Result<Option<(String, String)>, String> {
        let display_model = self.describe_model(model);
        let preview = truncate(input, 80);
        let opts: Vec<String> = options_desc
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();

        println!(
            "  {} Deciding using {}",
            "🤖".dimmed(),
            display_model.dimmed()
        );
        println!("     input: {}", preview.dimmed());

        let system = format!(
            "You are a decision maker. Given the following input, choose the best option and explain your reasoning.\n\n\
             Options:\n{}\n\n\
             Respond in exactly this format:\n\
             CHOICE: <option_name>\n\
             REASON: <one sentence explanation>",
            opts.join("\n")
        );

        match self.call(&system, input, model).await {
            Ok(response) => {
                let trimmed = response.trim();
                // Parse CHOICE: and REASON: from response
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
                    // Fallback: use the whole response as choice
                    choice = trimmed.to_string();
                }
                println!(
                    "  {} Decision: {}",
                    "✓".bright_green(),
                    choice.bright_white().bold()
                );
                Ok(Some((choice, reason)))
            }
            Err(LlmError::ConfigError(msg)) => {
                eprintln!("  {} {}", "✗".bright_red(), msg);
                Err(msg)
            }
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
        let display_model = self.describe_model(model);

        println!(
            "  {} Prompt using {}",
            "🤖".dimmed(),
            display_model.dimmed()
        );

        match self.call(system, user, model).await {
            Ok(response) => {
                println!("  {} Response ready", "✓".bright_green());
                Ok(Some(response.trim().to_string()))
            }
            Err(LlmError::ConfigError(msg)) => {
                eprintln!("  {} {}", "✗".bright_red(), msg);
                Err(msg)
            }
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
