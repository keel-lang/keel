// Rust guideline compliant 2026-02-21
//! Ollama backend — the default local LLM provider.
//!
//! Model resolution has no silent fallbacks: an unmapped or unreachable model
//! fails with an error that explains how to fix it.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::runtime::context::EnvProvider;
use crate::runtime::providers::{CompletionRequest, LlmError, LlmFuture, LlmProvider};

/// Talks to a local Ollama server over its `/api/chat` endpoint.
#[derive(Debug)]
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    model_map: HashMap<String, String>,
    default_model: String,
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    num_predict: u32,
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

impl OllamaProvider {
    /// Builds an Ollama provider from the environment.
    pub fn from_env(env: &dyn EnvProvider) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: env
                .var("OLLAMA_HOST")
                .unwrap_or_else(|| "http://localhost:11434".to_string()),
            model_map: Self::load_model_map(env),
            default_model: env.var("KEEL_OLLAMA_MODEL").unwrap_or_default(),
        }
    }

    fn load_model_map(env: &dyn EnvProvider) -> HashMap<String, String> {
        // `KEEL_MODEL_<NAME>=<ollama_model>` maps a Keel-side model alias to an
        // Ollama tag, e.g. `KEEL_MODEL_FAST=gemma4`.
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

    fn resolve_model<'a>(&'a self, model: &'a str) -> Result<&'a str, LlmError> {
        // A leading `ollama:` is the routing prefix (the registry uses it to pick
        // this backend). Strip it, then resolve the remaining name the same way a
        // bare tag is — alias lookup still applies. Only fall back to the literal
        // name when the prefix was explicit and no alias matched.
        let had_prefix = model.starts_with("ollama:");
        let name = model.strip_prefix("ollama:").unwrap_or(model);
        if let Some(mapped) = self.model_map.get(name) {
            return Ok(mapped);
        }
        if had_prefix {
            return Ok(name);
        }
        if !self.default_model.is_empty() {
            return Ok(&self.default_model);
        }
        Err(LlmError::ConfigError(format!(
            "Model '{name}' has no mapping.\n\
             Set one of:\n  \
               export KEEL_MODEL_{}=<ollama_model>\n  \
               export KEEL_OLLAMA_MODEL=<ollama_model>",
            name.to_uppercase().replace('-', "_")
        )))
    }
}

impl LlmProvider for OllamaProvider {
    fn complete(&self, request: CompletionRequest) -> LlmFuture<Option<String>> {
        let resolved = self.resolve_model(&request.model).map(ToString::to_string);
        let client = self.client.clone();
        let base_url = self.base_url.clone();
        let url = format!("{base_url}/api/chat");
        Box::pin(async move {
            let resolved = resolved?;
            let body = OllamaRequest {
                model: resolved,
                messages: vec![
                    ChatMessage {
                        role: "system".into(),
                        content: request.system,
                    },
                    ChatMessage {
                        role: "user".into(),
                        content: request.user,
                    },
                ],
                stream: false,
                options: OllamaOptions {
                    num_predict: request.max_tokens,
                },
            };
            let response = client.post(&url).json(&body).send().await.map_err(|e| {
                LlmError::CallFailed(format!("Ollama unreachable at {base_url}: {e}"))
            })?;

            let status = response.status();
            if !status.is_success() {
                let detail = response.text().await.unwrap_or_default();
                return Err(LlmError::CallFailed(format!(
                    "Ollama returned {status}: {detail}"
                )));
            }

            let parsed: OllamaResponse = response.json().await.map_err(|e| {
                LlmError::CallFailed(format!("Failed to parse Ollama response: {e}"))
            })?;

            let content = parsed
                .message
                .and_then(|m| m.content)
                .ok_or_else(|| LlmError::CallFailed("Empty response from Ollama".into()))?;
            Ok(Some(content))
        })
    }

    fn describe_model(&self, model: &str) -> String {
        match self.resolve_model(model) {
            Ok(name) => format!("{name} (ollama @ {})", self.base_url),
            Err(_) => format!("{model} (not mapped)"),
        }
    }
}
