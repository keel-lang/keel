use std::collections::HashMap;

use crate::builtins::{BuiltinMethod, BuiltinResult, TySpec};
use crate::interpreter::value::{MapKey, Value};
use crate::interpreter::{CallArgValue, Host, Namespace, RuntimeErrorKind};
use crate::runtime::convert::json_to_value;
use crate::runtime::namespace::{find_arg, ns, positional, throw_typed_error};

pub(crate) const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "ai",
        name: "classify",
        params: &[],
        result: BuiltinResult::AiClassify,
        doc: "Classify text into an enum variant.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "summarize",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Summarize text.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "draft",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Draft text from a prompt.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "extract",
        params: &[],
        result: BuiltinResult::AiExtract,
        doc: "Extract a typed value from text.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "translate",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Translate text to another language.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "decide",
        params: &[],
        result: BuiltinResult::AiExtract,
        doc: "Decide by extracting a typed value from context.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "prompt",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Send a raw prompt to the LLM and return its response.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "embed",
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Embed text into a vector (reserved, not yet stable).",
    },
];

pub(crate) fn namespace() -> Namespace {
    ns!("ai", {
        "classify" => |host, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("ai.classify: missing input"))?
                .to_display_string();
            let variants = classify_variants(host, &args)?;
            let criteria = extract_criteria(&args);
            let model = resolve_model(host, &args);
            let role = host.current_role();
            let enum_type = find_arg(&args, "as").and_then(|v| match v {
                Value::Namespace(n) => Some(n.clone()),
                _ => None,
            }).unwrap_or_default();

            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            match llm.classify(role.as_deref(), &rules, &input, &variants, &criteria, &model).await {
                Ok(Some(variant)) => Ok(Value::EnumVariant(enum_type, variant, None)),
                Ok(None) => Ok(Value::None),
                Err(crate::runtime::llm::LlmError::SchemaValidation { got }) => {
                    throw_typed_error(
                        RuntimeErrorKind::AiSchema,
                        &format!("LLM output did not match expected schema: '{got}'"),
                        Some(("got", got)),
                    )
                }
                // Network / timeout / mock failures: return none so `??` provides the default.
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(RuntimeErrorKind::Ai, &e.to_string(), None),
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
            let model = resolve_model(host, &args);
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            match llm.summarize(role.as_deref(), &rules, &input, length, format, max, unit_val, &model).await {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(Value::None),
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(RuntimeErrorKind::Ai, &e.to_string(), None),
            }
        }),

        "draft" => |host, args| Box::pin(async move {
            let description = positional(&args, 0)
                .ok_or_else(|| miette::miette!("ai.draft: missing description"))?
                .to_display_string();
            let tone = find_arg(&args, "tone").map(|v| v.to_display_string());
            let guidance = find_arg(&args, "guidance").map(|v| v.to_display_string());
            let max_length = find_arg(&args, "max_length").and_then(|v| v.as_int());
            let model = resolve_model(host, &args);
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            match llm
                .draft(role.as_deref(), &rules, &description, tone.as_deref(), guidance.as_deref(), max_length, &model)
                .await
            {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(Value::None),
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(RuntimeErrorKind::Ai, &e.to_string(), None),
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
            let model = resolve_model(host, &args);
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            match llm.extract(role.as_deref(), &rules, &input, &schema, &model).await {
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
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(RuntimeErrorKind::Ai, &e.to_string(), None),
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
            let model = resolve_model(host, &args);
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            match llm.translate(role.as_deref(), &rules, &input, &target_langs, &model).await {
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
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(RuntimeErrorKind::Ai, &e.to_string(), None),
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
            let model = resolve_model(host, &args);
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            match llm.decide(role.as_deref(), &rules, &input, &options, &model).await {
                Ok(Some((choice, reason))) => {
                    let mut m = HashMap::new();
                    m.insert(MapKey::Str("choice".into()), Value::String(choice));
                    m.insert(MapKey::Str("reason".into()), Value::String(reason));
                    m.insert(MapKey::Str("confidence".into()), Value::Float(1.0));
                    Ok(Value::Map(m))
                }
                Ok(None) => Ok(Value::None),
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(RuntimeErrorKind::Ai, &e.to_string(), None),
            }
        }),

        "prompt" => |host, args| Box::pin(async move {
            let system = find_arg(&args, "system").map(|v| v.to_display_string()).unwrap_or_default();
            let user = find_arg(&args, "user").map(|v| v.to_display_string()).unwrap_or_default();
            let response_format = find_arg(&args, "response_format").map(|v| v.to_display_string());
            let model = resolve_model(host, &args);
            let role = host.current_role();
            let rules = host.current_rules();
            let llm = host.runtime().llm.clone();
            match llm.prompt(role.as_deref(), &rules, &system, &user, response_format, &model).await {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(Value::None),
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(RuntimeErrorKind::Ai, &e.to_string(), None),
            }
        }),

        "embed" => |_i, _args| Box::pin(async move {
            // v0.1: embeddings not wired yet.
            Ok(Value::List(vec![]))
        }),
    })
}

/// Resolve the model string for an Ai.* call:
///   1. explicit `using: "model"` argument
///   2. enclosing agent's `@model` attribute
///   3. `"default"` (triggers KEEL_OLLAMA_MODEL catch-all)
fn resolve_model(host: &dyn Host, args: &[CallArgValue]) -> String {
    if let Some(v) = find_arg(args, "using") {
        return v.to_display_string();
    }
    host.current_model()
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
