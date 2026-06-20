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
