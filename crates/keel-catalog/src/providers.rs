// Rust guideline compliant 2026-02-21
//! Built-in LLM backend names.
//!
//! The single source of truth for which `@provider` names (and model-tag
//! `provider:` prefixes) are valid. Both the type checker (compile-time
//! `@provider` validation, prelude allowlist) and the runtime (registry
//! construction, routing, startup validation) import this, so the two phases
//! never disagree on the set of built-in backends.

/// The built-in LLM provider names, in registry order (Ollama is the default).
pub const BUILTIN_LLM_PROVIDERS: [&str; 3] = ["ollama", "openai", "anthropic"];

/// Returns whether `name` is a built-in LLM provider.
pub fn is_builtin_llm_provider(name: &str) -> bool {
    BUILTIN_LLM_PROVIDERS.contains(&name)
}

/// If `model` carries a `<provider>:` prefix naming a built-in backend, returns
/// that provider name. A bare tag — or one whose prefix isn't a built-in —
/// returns `None`. Provider names contain no colon, so the text before the first
/// `:` is the only prefix candidate.
pub fn builtin_provider_prefix(model: &str) -> Option<&'static str> {
    let (prefix, _) = model.split_once(':')?;
    BUILTIN_LLM_PROVIDERS
        .into_iter()
        .find(|provider| *provider == prefix)
}

/// The shared error message for an `@provider` attribute that doesn't name a
/// built-in backend. Used by both the compiler checker and the interpreter so
/// the two phases word the rejection identically.
pub fn provider_attribute_error() -> String {
    format!(
        "@provider must name a built-in provider — use one of: {}. \
         User-authored providers are planned; see SPEC §5.5.",
        BUILTIN_LLM_PROVIDERS.join(", ")
    )
}
