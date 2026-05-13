use std::collections::HashMap;
use std::net::SocketAddr;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use keel_lang::runtime::context::EnvProvider;
use keel_lang::runtime::llm::{LlmClient, LlmError};
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

struct TestEnv {
    values: HashMap<String, String>,
}

impl TestEnv {
    fn new(values: &[(&str, &str)]) -> Self {
        Self {
            values: values
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        }
    }
}

impl EnvProvider for TestEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.values.get(name).cloned()
    }

    fn vars(&self) -> Vec<(String, String)> {
        self.values
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
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

async fn chat(State(state): State<FakeOllama>, Json(payload): Json<Value>) -> impl IntoResponse {
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
    LlmClient::from_env(&TestEnv::new(&values))
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
