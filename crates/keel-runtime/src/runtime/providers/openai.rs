// Rust guideline compliant 2026-02-21
//! OpenAI backend over the Chat Completions API.
//!
//! Wire contract (`POST /v1/chat/completions`): `Authorization: Bearer` header,
//! the system prompt as a `system` message role, and a response whose text is
//! `choices[0].message.content`.

use serde::{Deserialize, Serialize};

use crate::runtime::context::EnvProvider;
use crate::runtime::providers::{CompletionRequest, LlmError, LlmFuture, LlmProvider};

/// Default Chat Completions endpoint; overridable via `OPENAI_BASE_URL`.
const DEFAULT_BASE_URL: &str = "https://api.openai.com";

/// Calls OpenAI models through the Chat Completions API.
#[derive(Debug)]
pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    // `max_tokens` is correct for `gpt-4o`-class chat models. Reasoning models
    // (the `o`-series) reject it and require `max_completion_tokens` instead — if
    // those are added as routable targets, this field must be chosen per model.
    max_tokens: u32,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

impl OpenAiProvider {
    /// Builds an OpenAI provider from the environment. The API key is read
    /// lazily — a missing key only fails when the backend is actually used.
    pub fn from_env(env: &dyn EnvProvider) -> Self {
        let base_url = env
            .var("OPENAI_BASE_URL")
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        Self {
            client: reqwest::Client::new(),
            base_url,
            api_key: env.var("OPENAI_API_KEY"),
        }
    }
}

impl LlmProvider for OpenAiProvider {
    fn complete(&self, request: CompletionRequest) -> LlmFuture<Option<String>> {
        let model = request
            .model
            .strip_prefix("openai:")
            .unwrap_or(&request.model)
            .to_string();
        let key = self.api_key.clone();
        let client = self.client.clone();
        let url = format!("{}/v1/chat/completions", self.base_url);
        Box::pin(async move {
            let Some(key) = key else {
                return Err(LlmError::ConfigError(
                    "OPENAI_API_KEY is not set; export it to use the `openai` provider".to_string(),
                ));
            };
            let body = ChatRequest {
                model,
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
                max_tokens: request.max_tokens,
            };
            let response = client
                .post(&url)
                .bearer_auth(key)
                .json(&body)
                .send()
                .await
                .map_err(|e| LlmError::CallFailed(format!("OpenAI unreachable: {e}")))?;

            let status = response.status();
            if !status.is_success() {
                let detail = response.text().await.unwrap_or_default();
                return Err(LlmError::CallFailed(format!(
                    "OpenAI returned {status}: {detail}"
                )));
            }

            let parsed: ChatResponse = response.json().await.map_err(|e| {
                LlmError::CallFailed(format!("Failed to parse OpenAI response: {e}"))
            })?;

            let content = parsed
                .choices
                .into_iter()
                .next()
                .map(|c| c.message.content)
                .ok_or_else(|| LlmError::CallFailed("Empty response from OpenAI".to_string()))?;
            Ok(Some(content))
        })
    }

    fn describe_model(&self, model: &str) -> String {
        let model = model.strip_prefix("openai:").unwrap_or(model);
        format!("{model} (openai)")
    }
}
