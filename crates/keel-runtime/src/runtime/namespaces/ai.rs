use std::collections::HashMap;

use crate::interpreter::value::{MapKey, Value};
use crate::interpreter::{CallArgValue, Host, Namespace, RuntimeErrorKind};
use crate::runtime::convert::json_to_value;
use crate::runtime::llm::{LlmError, Transport};
use crate::runtime::namespace::{
    find_arg, make_typed_report_with, ns, positional, throw_typed_error,
};
use crate::runtime::providers::CompletionRequest;

/// Map an `LlmError` to a typed Keel runtime error.
///
/// Real failures throw rather than degrade to `none`, so a production agent
/// whose model is unreachable or misconfigured surfaces the cause instead of
/// silently taking a `??` default:
///   - network / provider unreachable → `AiError { reason: "unavailable" }`
///   - model not mapped / config fault → `AiError { reason: "provider" }`
///   - output matched no enum/schema   → `AiSchemaError { got }`
///
/// Mock mode and a model that genuinely returns no answer never reach here —
/// those yield `Ok(None)` so `??` and `when` handle the absence.
fn throw_llm_error(e: LlmError) -> miette::Result<Value> {
    match e {
        LlmError::SchemaValidation { got } => throw_typed_error(
            RuntimeErrorKind::AiSchema,
            &format!("LLM output did not match expected schema: '{got}'"),
            Some(("got", got)),
        ),
        LlmError::CallFailed(msg) => throw_typed_error(
            RuntimeErrorKind::Ai,
            &msg,
            Some(("reason", "unavailable".into())),
        ),
        LlmError::ConfigError(msg) => throw_typed_error(
            RuntimeErrorKind::Ai,
            &msg,
            Some(("reason", "provider".into())),
        ),
    }
}

pub(crate) fn namespace() -> Namespace {
    ns!("ai", {
        "classify" => |host, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("ai.classify: missing input"))?
                .to_display_string();
            let variants = classify_variants(host, &args)?;
            let criteria = extract_criteria(&args);
            let (model, user_provider) = resolve_provider(host, &args);
            let max_tokens = host.current_max_tokens();
            let role = host.current_role();
            let enum_type = find_arg(&args, "as").and_then(|v| match v {
                Value::Namespace(n) => Some(n.clone()),
                _ => None,
            }).unwrap_or_default();

            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            let transport = transport_for(host, user_provider)?;
            match llm.classify(role.as_deref(), &rules, &input, &variants, &criteria, &model, max_tokens, transport).await {
                Ok(Some(variant)) => Ok(Value::EnumVariant(enum_type, variant, None)),
                Ok(None) => Ok(Value::None),
                Err(e) => throw_llm_error(e),
            }
        }),

        "summarize" => |host, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("ai.summarize: missing input"))?
                .to_display_string();
            let unit_val = find_arg(&args, "unit").map(|v| v.to_display_string());
            let length = match (find_arg(&args, "in"), &unit_val) {
                (Some(Value::Integer(n)), Some(u)) => Some((*n, u.clone())),
                _ => None,
            };
            let format = find_arg(&args, "format").map(|v| v.to_display_string());
            let max = find_arg(&args, "max").and_then(|v| v.as_int());
            let (model, user_provider) = resolve_provider(host, &args);
            let max_tokens = host.current_max_tokens();
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            let transport = transport_for(host, user_provider)?;
            match llm.summarize(role.as_deref(), &rules, &input, length, format, max, unit_val, &model, max_tokens, transport).await {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(Value::None),
                Err(e) => throw_llm_error(e),
            }
        }),

        "draft" => |host, args| Box::pin(async move {
            let description = positional(&args, 0)
                .ok_or_else(|| miette::miette!("ai.draft: missing description"))?
                .to_display_string();
            let tone = find_arg(&args, "tone").map(|v| v.to_display_string());
            let guidance = find_arg(&args, "guidance").map(|v| v.to_display_string());
            let max_length = find_arg(&args, "max_length").and_then(|v| v.as_int());
            let (model, user_provider) = resolve_provider(host, &args);
            let max_tokens = host.current_max_tokens();
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            let transport = transport_for(host, user_provider)?;
            match llm
                .draft(role.as_deref(), &rules, &description, tone.as_deref(), guidance.as_deref(), max_length, &model, max_tokens, transport)
                .await
            {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(Value::None),
                Err(e) => throw_llm_error(e),
            }
        }),

        "extract" => |host, args| Box::pin(async move {
            let input = match find_arg(&args, "from") {
                Some(v) => v.to_display_string(),
                None => positional(&args, 0)
                    .ok_or_else(|| miette::miette!("ai.extract: missing input"))?
                    .to_display_string(),
            };
            // Schema from `schema: { field: "type" }` map, or derived from `as: T` struct type.
            // target_type records the struct name when using `as: T` so the result can be tagged.
            let (schema, target_type): (Vec<(String, String)>, Option<String>) =
                match find_arg(&args, "schema") {
                    Some(Value::Map(m)) => (
                        m.iter()
                            .map(|(k, v)| (k.to_string(), v.to_display_string()))
                            .collect(),
                        None,
                    ),
                    _ => match find_arg(&args, "as") {
                        Some(Value::Namespace(type_name)) => {
                            let type_name = type_name.clone();
                            let schema =
                                host.struct_types().get(&type_name).cloned().ok_or_else(|| {
                                    miette::miette!(
                                        "ai.extract: `as: {type_name}` is not a known struct type. \
                                         Declare it with `type {type_name} {{ field: type }}`"
                                    )
                                })?;
                            (schema, Some(type_name))
                        }
                        _ => (Vec::new(), None),
                    },
                };
            let (model, user_provider) = resolve_provider(host, &args);
            let max_tokens = host.current_max_tokens();
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            let transport = transport_for(host, user_provider)?;
            match llm.extract(role.as_deref(), &rules, &input, &schema, &model, max_tokens, transport).await {
                Ok(Some(json)) => {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json) {
                        let v = json_to_value(&parsed);
                        let v = if let Some(tn) = target_type
                            && let Value::Map(m) = v
                        {
                            let fields = m.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
                            Value::Struct(tn, fields)
                        } else {
                            v
                        };
                        Ok(v)
                    } else {
                        Ok(Value::String(json))
                    }
                }
                Ok(None) => Ok(Value::None),
                Err(e) => throw_llm_error(e),
            }
        }),

        "translate" => |host, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("ai.translate: missing input"))?
                .to_display_string();
            let target_langs: Vec<String> = match find_arg(&args, "to") {
                Some(Value::List(items)) => items.iter().map(|v| v.to_display_string()).collect(),
                Some(other) => vec![other.to_display_string()],
                None => return Err(miette::miette!("ai.translate: missing `to:` argument")),
            };
            if target_langs.is_empty() {
                return Err(miette::miette!(
                    "ai.translate: `to:` must contain at least one language"
                ));
            }
            let (model, user_provider) = resolve_provider(host, &args);
            let max_tokens = host.current_max_tokens();
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            let transport = transport_for(host, user_provider)?;
            match llm.translate(role.as_deref(), &rules, &input, &target_langs, &model, max_tokens, transport).await {
                Ok(Some(map)) if target_langs.len() == 1 => {
                    Ok(Value::String(map.into_values().next().unwrap_or_default()))
                }
                Ok(Some(map)) => {
                    let mut out = HashMap::new();
                    for (k, v) in map {
                        out.insert(MapKey::Str(k), Value::String(v));
                    }
                    Ok(Value::Map(out))
                }
                Ok(None) => Ok(Value::None),
                Err(e) => throw_llm_error(e),
            }
        }),

        "decide" => |host, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("ai.decide: missing input"))?
                .to_display_string();
            let options: Vec<String> = match find_arg(&args, "options") {
                Some(Value::List(items)) => items.iter().map(|v| v.to_display_string()).collect(),
                _ => Vec::new(),
            };
            let (model, user_provider) = resolve_provider(host, &args);
            let max_tokens = host.current_max_tokens();
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            let transport = transport_for(host, user_provider)?;
            match llm.decide(role.as_deref(), &rules, &input, &options, &model, max_tokens, transport).await {
                Ok(Some((choice, reason))) => {
                    let mut m = HashMap::new();
                    m.insert(MapKey::Str("choice".into()), Value::String(choice));
                    m.insert(MapKey::Str("reason".into()), Value::String(reason));
                    m.insert(MapKey::Str("confidence".into()), Value::Float(1.0));
                    Ok(Value::Map(m))
                }
                Ok(None) => Ok(Value::None),
                Err(e) => throw_llm_error(e),
            }
        }),

        "prompt" => |host, args| Box::pin(async move {
            let system = find_arg(&args, "system").map(|v| v.to_display_string()).unwrap_or_default();
            let user = find_arg(&args, "user").map(|v| v.to_display_string()).unwrap_or_default();
            let response_format = find_arg(&args, "response_format").map(|v| v.to_display_string());
            let (model, user_provider) = resolve_provider(host, &args);
            let max_tokens = host.current_max_tokens();
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            let transport = transport_for(host, user_provider)?;
            match llm.prompt(role.as_deref(), &rules, &system, &user, response_format, &model, max_tokens, transport).await {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(Value::None),
                Err(e) => throw_llm_error(e),
            }
        }),

        "embed" => |_i, _args| Box::pin(async move {
            // v0.1: embeddings not wired yet.
            Ok(Value::List(vec![]))
        }),

        "install" => |host, args| Box::pin(async move {
            let type_name = match positional(&args, 0) {
                Some(Value::Namespace(n)) => n.clone(),
                other => {
                    let got = other.map_or_else(
                        || "nothing".to_string(),
                        |v| format!("a {}", v.type_name()),
                    );
                    return Err(miette::miette!(
                        "ai.install expects a provider type implementing `LlmProvider`, \
                         e.g. `ai.install(MyProvider)` — got {got}"
                    ));
                }
            };
            // Conformance is also enforced by the checker; this guards the
            // interpreter path (e.g. REPL) and pins the error to the install site.
            let probe = Value::Struct(type_name.clone(), HashMap::new());
            if host.find_impl_task(&probe, "complete").is_none() {
                return throw_typed_error(
                    RuntimeErrorKind::Ai,
                    &format!(
                        "`{type_name}` cannot be installed as a provider: it has no \
                         `impl LlmProvider` with a `complete` method"
                    ),
                    Some(("reason", "provider".into())),
                );
            }
            host.set_installed_provider(type_name);
            Ok(Value::None)
        }),
    })
}

/// Resolve the model tag and selected provider for an `ai.*` call.
///
/// Returns the model string to send and, when a user-authored provider applies,
/// its type name. Precedence, most-specific first:
///   1. explicit `provider:` prefix on the tag (`using:` or `@model`) — built-in
///   2. `@provider` on the enclosing agent (built-in name or user type)
///   3. the program-wide `ai.install` provider (always a user type)
///   4. the built-in program default
///
/// A built-in `@provider` is folded into the tag as a routing prefix (a bare
/// `"gpt-4o"` under `@provider openai` becomes `"openai:gpt-4o"`); a user
/// provider leaves the tag bare and is dispatched by re-entering the interpreter.
fn resolve_provider(host: &dyn Host, args: &[CallArgValue]) -> (String, Option<String>) {
    let tag = match find_arg(args, "using") {
        Some(v) => v.to_display_string(),
        None => host.current_model(),
    };
    route_tag(
        tag,
        host.current_provider()
            .or_else(|| host.installed_provider()),
    )
}

/// Route a bare model tag against the selected `@provider`/`ai.install` name.
///
/// An explicit `provider:` prefix on `tag` always wins. Otherwise a built-in
/// `provider` is folded in as a routing prefix, while a user-provider name is
/// returned separately and the tag is left bare.
fn route_tag(tag: String, provider: Option<String>) -> (String, Option<String>) {
    if keel_catalog::builtin_provider_prefix(&tag).is_some() {
        return (tag, None);
    }
    match provider {
        Some(name) if !keel_catalog::is_builtin_llm_provider(&name) => (tag, Some(name)),
        Some(name) => (format!("{name}:{tag}"), None),
        None => (tag, None),
    }
}

/// Build the transport for a call: the built-in registry, or a user provider.
///
/// # Errors
///
/// Returns an `AiError` when `user_provider`'s `complete()` is already on the
/// call stack — a provider must not call `ai.*` from inside its own `complete()`.
fn transport_for(
    host: &mut dyn Host,
    user_provider: Option<String>,
) -> miette::Result<Transport<'_>> {
    let Some(type_name) = user_provider else {
        return Ok(Transport::Builtin);
    };
    if host.provider_is_active(&type_name) {
        return Err(make_typed_report_with(
            RuntimeErrorKind::Ai,
            format!(
                "provider `{type_name}` called an `ai.*` operation from inside its own \
                 `complete()`; this would recurse without bound"
            ),
            Some(("reason", "provider".into())),
        ));
    }
    Ok(Transport::User(Box::new(move |req| {
        Box::pin(async move {
            let receiver = Value::Struct(type_name.clone(), HashMap::new());
            let Some(decl) = host.find_impl_task(&receiver, "complete") else {
                return Err(LlmError::ConfigError(format!(
                    "installed provider `{type_name}` has no `complete` method"
                )));
            };
            host.push_active_provider(type_name.clone());
            let call_args = vec![
                CallArgValue {
                    name: None,
                    value: receiver,
                },
                CallArgValue {
                    name: None,
                    value: completion_request_to_value(&req),
                },
            ];
            let outcome = host.call_task("complete", &decl, call_args).await;
            host.pop_active_provider(&type_name);
            match outcome {
                Ok(Value::String(s)) => Ok(Some(s)),
                Ok(Value::None) => Ok(None),
                Ok(other) => Err(LlmError::CallFailed(format!(
                    "provider `{type_name}` `complete()` must return str, got {}",
                    other.type_name()
                ))),
                Err(report) => Err(LlmError::CallFailed(report.to_string())),
            }
        })
    })))
}

/// Render a Rust [`CompletionRequest`] as the Keel `CompletionRequest` struct
/// the user's `complete()` receives.
fn completion_request_to_value(req: &CompletionRequest) -> Value {
    let mut fields = HashMap::new();
    fields.insert("system".to_string(), Value::String(req.system.clone()));
    fields.insert("user".to_string(), Value::String(req.user.clone()));
    fields.insert("model".to_string(), Value::String(req.model.clone()));
    fields.insert(
        "max_tokens".to_string(),
        Value::Integer(i64::from(req.max_tokens)),
    );
    Value::Struct("CompletionRequest".to_string(), fields)
}

/// Extract enum variants from `as: T` (Value::Namespace(T)) by looking
/// T up in the host's enum registry.
fn classify_variants(host: &dyn Host, args: &[CallArgValue]) -> miette::Result<Vec<String>> {
    match find_arg(args, "as") {
        Some(Value::Namespace(name)) => {
            host.enum_types().get(name).cloned().ok_or_else(|| {
                miette::miette!("ai.classify: `as: {name}` is not a simple enum type")
            })
        }
        Some(Value::List(items)) => {
            // Inline form: `as: [low, medium, high]`
            Ok(items.iter().map(|v| v.to_display_string()).collect())
        }
        _ => Err(miette::miette!("ai.classify: missing `as:` argument")),
    }
}

/// Extract classification criteria from `considering: { "hint": Variant }`.
fn extract_criteria(args: &[CallArgValue]) -> Vec<(String, String)> {
    match find_arg(args, "considering") {
        Some(Value::Map(m)) => m
            .iter()
            .map(|(k, v)| {
                let variant_name = match v {
                    Value::EnumVariant(_, variant, _) => variant.clone(),
                    other => other.to_display_string(),
                };
                (k.to_string(), variant_name)
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::RuntimeError;

    fn typed(e: LlmError) -> RuntimeError {
        throw_llm_error(e)
            .unwrap_err()
            .downcast::<RuntimeError>()
            .expect("LlmError should map to a typed RuntimeError")
    }

    #[test]
    fn call_failed_throws_ai_error_with_unavailable_reason() {
        let err = typed(LlmError::CallFailed("Ollama unreachable".into()));
        assert_eq!(err.type_name(), "AiError");
        assert_eq!(
            err.fields.get("reason").map(|v| v.to_display_string()),
            Some("unavailable".into())
        );
    }

    #[test]
    fn config_error_throws_ai_error_with_provider_reason() {
        let err = typed(LlmError::ConfigError("model not mapped".into()));
        assert_eq!(err.type_name(), "AiError");
        assert_eq!(
            err.fields.get("reason").map(|v| v.to_display_string()),
            Some("provider".into())
        );
    }

    #[test]
    fn schema_validation_throws_ai_schema_error_with_got() {
        let err = typed(LlmError::SchemaValidation {
            got: "maybe".into(),
        });
        assert_eq!(err.type_name(), "AiSchemaError");
        assert_eq!(
            err.fields.get("got").map(|v| v.to_display_string()),
            Some("maybe".into())
        );
    }

    #[test]
    fn route_tag_explicit_prefix_wins_over_provider() {
        // An explicit `provider:` prefix is never overridden by @provider.
        assert_eq!(
            route_tag("anthropic:claude-opus-4-8".into(), Some("openai".into())),
            ("anthropic:claude-opus-4-8".into(), None)
        );
    }

    #[test]
    fn route_tag_bare_tag_gets_builtin_provider_prefix() {
        assert_eq!(
            route_tag("gpt-4o".into(), Some("openai".into())),
            ("openai:gpt-4o".into(), None)
        );
    }

    #[test]
    fn route_tag_bare_tag_without_provider_is_unchanged() {
        assert_eq!(route_tag("gpt-4o".into(), None), ("gpt-4o".into(), None));
    }

    #[test]
    fn route_tag_user_provider_leaves_tag_bare() {
        // A non-built-in provider name is a user type: the tag stays bare and
        // the type name is returned for re-entrant dispatch.
        assert_eq!(
            route_tag("my-model".into(), Some("MyProvider".into())),
            ("my-model".into(), Some("MyProvider".into()))
        );
    }

    #[test]
    fn route_tag_user_provider_does_not_override_explicit_prefix() {
        assert_eq!(
            route_tag("openai:gpt-4o".into(), Some("MyProvider".into())),
            ("openai:gpt-4o".into(), None)
        );
    }
}
