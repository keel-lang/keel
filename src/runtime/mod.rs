pub mod email;
pub mod human;
pub mod llm;

use std::collections::HashMap;

use crate::interpreter::value::Value;

/// Runtime services available to the interpreter.
pub struct Runtime {
    pub llm: llm::LlmClient,
    model: String,
}

impl Runtime {
    pub fn new() -> Self {
        Runtime {
            llm: llm::LlmClient::new(),
            model: "claude-sonnet".to_string(),
        }
    }

    pub fn set_model(&mut self, model: &str) {
        self.model = model.to_string();
    }

    pub fn describe_model(&self, model: &str) -> String {
        self.llm.describe_model(model)
    }

    // ── Human interaction ────────────────────────────────────────

    pub fn notify(&self, message: &str) {
        human::notify(message);
    }

    pub fn show(&self, value: &Value) {
        human::show(value);
    }

    pub fn ask(&self, prompt: &str) -> String {
        human::ask(prompt)
    }

    pub fn confirm(&self, message: &str) -> bool {
        human::confirm(message)
    }

    // ── AI operations ────────────────────────────────────────────
    //
    // Return type: Result<Option<String>, String>
    //   Ok(Some(val))  — success
    //   Ok(None)       — LLM responded but no valid result (fallback applies)
    //   Err(msg)       — config/infra error, execution stops

    pub async fn classify(
        &self,
        input: &str,
        variants: &[String],
        criteria: Option<&[(String, String)]>,
        model_override: Option<&str>,
    ) -> Result<Option<String>, String> {
        let model = model_override.unwrap_or(&self.model);
        self.llm.classify(input, variants, criteria, model).await
    }

    pub async fn summarize(
        &self,
        input: &str,
        length: Option<&(i64, String)>,
        model_override: Option<&str>,
    ) -> Result<Option<String>, String> {
        let model = model_override.unwrap_or(&self.model);
        self.llm.summarize(input, length, model).await
    }

    pub async fn draft(
        &self,
        description: &str,
        options: &HashMap<String, String>,
        model_override: Option<&str>,
    ) -> Result<Option<String>, String> {
        let model = model_override.unwrap_or(&self.model);
        self.llm.draft(description, options, model).await
    }

    pub async fn extract(
        &self,
        input: &str,
        fields: &[(String, String)],
        model_override: Option<&str>,
    ) -> Result<Option<String>, String> {
        let model = model_override.unwrap_or(&self.model);
        self.llm.extract(input, fields, model).await
    }

    pub async fn translate(
        &self,
        input: &str,
        target_langs: &[String],
        model_override: Option<&str>,
    ) -> Result<Option<HashMap<String, String>>, String> {
        let model = model_override.unwrap_or(&self.model);
        self.llm.translate(input, target_langs, model).await
    }

    pub async fn decide(
        &self,
        input: &str,
        options: &HashMap<String, String>,
        model_override: Option<&str>,
    ) -> Result<Option<(String, String)>, String> {
        let model = model_override.unwrap_or(&self.model);
        self.llm.decide(input, options, model).await
    }

    pub async fn http_get(&self, url: &str) -> Result<Value, String> {
        let response = reqwest::get(url)
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        let status = response.status().as_u16() as i64;
        let headers: HashMap<String, Value> = response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    Value::String(v.to_str().unwrap_or("").to_string()),
                )
            })
            .collect();
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {e}"))?;

        let mut map = HashMap::new();
        map.insert("status".to_string(), Value::Integer(status));
        map.insert("body".to_string(), Value::String(body));
        map.insert("headers".to_string(), Value::Map(headers));
        map.insert("is_ok".to_string(), Value::Bool((200..300).contains(&(status as u16))));
        Ok(Value::Map(map))
    }

    pub async fn prompt(
        &self,
        system: &str,
        user: &str,
        model_override: Option<&str>,
    ) -> Result<Option<String>, String> {
        let model = model_override.unwrap_or(&self.model);
        self.llm.prompt(system, user, model).await
    }
}
