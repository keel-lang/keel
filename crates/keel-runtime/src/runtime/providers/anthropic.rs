// Rust guideline compliant 2026-02-21
//! Anthropic (Claude) backend over the Messages API.
//!
//! Wire contract (`POST /v1/messages`): `x-api-key` + `anthropic-version`
//! headers, a top-level `system` field, `max_tokens` required, and a response
//! whose text lives in `content[]` blocks of `type: "text"`.

use serde::{Deserialize, Serialize};

use crate::runtime::context::EnvProvider;
use crate::runtime::providers::{CompletionRequest, LlmError, LlmFuture, LlmProvider};

/// Anthropic API version pinned for the Messages API request shape used here.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Default Messages API endpoint; overridable via `ANTHROPIC_BASE_URL`.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Calls Claude models through the Anthropic Messages API.
#[derive(Debug)]
pub struct AnthropicProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

impl AnthropicProvider {
    /// Builds an Anthropic provider from the environment. The API key is read
    /// lazily — a missing key only fails when the backend is actually used.
    pub fn from_env(env: &dyn EnvProvider) -> Self {
        let base_url = env
            .var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        Self {
            client: reqwest::Client::new(),
            base_url,
            api_key: env.var("ANTHROPIC_API_KEY"),
        }
    }
}

impl LlmProvider for AnthropicProvider {
    fn complete(&self, request: CompletionRequest) -> LlmFuture<Option<String>> {
        let model = request
            .model
            .strip_prefix("anthropic:")
            .unwrap_or(&request.model)
            .to_string();
        let key = self.api_key.clone();
        let client = self.client.clone();
        let url = format!("{}/v1/messages", self.base_url);
        Box::pin(async move {
            // `default` is Keel's "no @model set" sentinel; Anthropic has no
            // server-side default, so reject it with an actionable error rather
            // than asking the API for a model literally named "default".
            if model == "default" {
                return Err(LlmError::ConfigError(
                    "the `anthropic` provider has no default model; set @model \"<model>\" on the \
                     agent or pass `using: \"anthropic:<model>\"`"
                        .to_string(),
                ));
            }
            let Some(key) = key else {
                return Err(LlmError::ConfigError(
                    "ANTHROPIC_API_KEY is not set; export it to use the `anthropic` provider"
                        .to_string(),
                ));
            };
            let body = MessagesRequest {
                model,
                max_tokens: request.max_tokens,
                system: request.system,
                messages: vec![Message {
                    role: "user".into(),
                    content: request.user,
                }],
            };
            let response = client
                .post(&url)
                .header("x-api-key", key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .json(&body)
                .send()
                .await
                .map_err(|e| LlmError::CallFailed(format!("Anthropic unreachable: {e}")))?;

            let status = response.status();
            if !status.is_success() {
                let detail = response.text().await.unwrap_or_default();
                return Err(LlmError::CallFailed(format!(
                    "Anthropic returned {status}: {detail}"
                )));
            }

            let parsed: MessagesResponse = response.json().await.map_err(|e| {
                LlmError::CallFailed(format!("Failed to parse Anthropic response: {e}"))
            })?;

            if parsed.stop_reason.as_deref() == Some("refusal") {
                return Err(LlmError::CallFailed(
                    "Anthropic declined the request (stop_reason: refusal)".to_string(),
                ));
            }

            let text: String = parsed
                .content
                .into_iter()
                .filter(|b| b.kind == "text")
                .map(|b| b.text)
                .collect();
            if text.is_empty() {
                return Err(LlmError::CallFailed(
                    "Empty response from Anthropic".to_string(),
                ));
            }
            Ok(Some(text))
        })
    }

    fn describe_model(&self, model: &str) -> String {
        let model = model.strip_prefix("anthropic:").unwrap_or(model);
        format!("{model} (anthropic)")
    }
}
