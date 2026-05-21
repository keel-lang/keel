use std::collections::HashMap;

use crate::interpreter::value::Value;
use crate::interpreter::{CallArgValue, Interpreter, Namespace};
use crate::runtime::convert::json_to_value;
use crate::runtime::namespace::{find_arg, ns, positional, throw_typed_error};

pub(crate) fn namespace() -> Namespace {
    ns!("Ai", {
        "classify" => |interp, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("Ai.classify: missing input"))?
                .to_display_string();
            let variants = classify_variants(interp, &args)?;
            let criteria = extract_criteria(&args);
            let model = resolve_model(interp, &args);
            let role = interp.current_role();
            let enum_type = find_arg(&args, "as").and_then(|v| match v {
                Value::Namespace(n) => Some(n.clone()),
                _ => None,
            }).unwrap_or_default();

            let rules = interp.current_rules();
            let llm = interp.runtime.llm.clone();
            match llm.classify(role.as_deref(), &rules, &input, &variants, &criteria, &model).await {
                Ok(Some(variant)) => Ok(Value::EnumVariant(enum_type, variant, None)),
                Ok(None) => Ok(Value::None),
                Err(crate::runtime::llm::LlmError::ConfigError(msg)) => Err(miette::miette!("{msg}")),
                Err(crate::runtime::llm::LlmError::SchemaValidation { got }) => {
                    throw_typed_error(interp, "AiSchemaError",
                        &format!("LLM output did not match expected schema: '{got}'"),
                        Some(("got", got)))
                }
                // Network / timeout / mock failures: return none so `??` provides the default.
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
            }
        }),

        "summarize" => |interp, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("Ai.summarize: missing input"))?
                .to_display_string();
            let unit_val = find_arg(&args, "unit").map(|v| v.to_display_string());
            let length = match (find_arg(&args, "in"), &unit_val) {
                (Some(Value::Integer(n)), Some(u)) => Some((*n, u.clone())),
                _ => None,
            };
            let format = find_arg(&args, "format").map(|v| v.to_display_string());
            let max = find_arg(&args, "max").and_then(|v| v.as_int());
            let model = resolve_model(interp, &args);
            let role = interp.current_role();
            let rules = interp.current_rules();
            let llm = interp.runtime.llm.clone();
            match llm.summarize(role.as_deref(), &rules, &input, length, format, max, unit_val, &model).await {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(Value::None),
                Err(crate::runtime::llm::LlmError::ConfigError(msg)) => Err(miette::miette!("{msg}")),
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(interp, "AiError", &e.to_string(), None),
            }
        }),

        "draft" => |interp, args| Box::pin(async move {
            let description = positional(&args, 0)
                .ok_or_else(|| miette::miette!("Ai.draft: missing description"))?
                .to_display_string();
            let tone = find_arg(&args, "tone").map(|v| v.to_display_string());
            let guidance = find_arg(&args, "guidance").map(|v| v.to_display_string());
            let max_length = find_arg(&args, "max_length").and_then(|v| v.as_int());
            let model = resolve_model(interp, &args);
            let role = interp.current_role();
            let rules = interp.current_rules();
            let llm = interp.runtime.llm.clone();
            match llm
                .draft(role.as_deref(), &rules, &description, tone.as_deref(), guidance.as_deref(), max_length, &model)
                .await
            {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(Value::None),
                Err(crate::runtime::llm::LlmError::ConfigError(msg)) => Err(miette::miette!("{msg}")),
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(interp, "AiError", &e.to_string(), None),
            }
        }),

        "extract" => |interp, args| Box::pin(async move {
            let input = match find_arg(&args, "from") {
                Some(v) => v.to_display_string(),
                None => positional(&args, 0)
                    .ok_or_else(|| miette::miette!("Ai.extract: missing input"))?
                    .to_display_string(),
            };
            // Schema from `schema: { field: "type" }` map, or derived from `as: T` struct type.
            // target_type records the struct name when using `as: T` so the result can be tagged.
            let (schema, target_type): (Vec<(String, String)>, Option<String>) =
                match find_arg(&args, "schema") {
                    Some(Value::Map(m)) => (
                        m.iter().map(|(k, v)| (k.clone(), v.to_display_string())).collect(),
                        None,
                    ),
                    _ => match find_arg(&args, "as") {
                        Some(Value::Namespace(type_name)) => {
                            let type_name = type_name.clone();
                            let schema =
                                interp.struct_types.get(&type_name).cloned().ok_or_else(|| {
                                    miette::miette!(
                                        "Ai.extract: `as: {type_name}` is not a known struct type. \
                                         Declare it with `type {type_name} {{ field: type }}`"
                                    )
                                })?;
                            (schema, Some(type_name))
                        }
                        _ => (Vec::new(), None),
                    },
                };
            let model = resolve_model(interp, &args);
            let role = interp.current_role();
            let rules = interp.current_rules();
            let llm = interp.runtime.llm.clone();
            match llm.extract(role.as_deref(), &rules, &input, &schema, &model).await {
                Ok(Some(json)) => {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json) {
                        let v = json_to_value(&parsed);
                        let v = if let Some(tn) = target_type
                            && let Value::Map(fields) = v
                        {
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
                Err(crate::runtime::llm::LlmError::ConfigError(msg)) => Err(miette::miette!("{msg}")),
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(interp, "AiError", &e.to_string(), None),
            }
        }),

        "translate" => |interp, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("Ai.translate: missing input"))?
                .to_display_string();
            let target_langs: Vec<String> = match find_arg(&args, "to") {
                Some(Value::List(items)) => items.iter().map(|v| v.to_display_string()).collect(),
                Some(other) => vec![other.to_display_string()],
                None => return Err(miette::miette!("Ai.translate: missing `to:` argument")),
            };
            let model = resolve_model(interp, &args);
            let role = interp.current_role();
            let rules = interp.current_rules();
            let llm = interp.runtime.llm.clone();
            match llm.translate(role.as_deref(), &rules, &input, &target_langs, &model).await {
                Ok(Some(map)) if target_langs.len() == 1 => {
                    Ok(Value::String(map.into_values().next().unwrap_or_default()))
                }
                Ok(Some(map)) => {
                    let mut out = HashMap::new();
                    for (k, v) in map { out.insert(k, Value::String(v)); }
                    Ok(Value::Map(out))
                }
                Ok(None) => Ok(Value::None),
                Err(crate::runtime::llm::LlmError::ConfigError(msg)) => Err(miette::miette!("{msg}")),
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(interp, "AiError", &e.to_string(), None),
            }
        }),

        "decide" => |interp, args| Box::pin(async move {
            let input = positional(&args, 0)
                .ok_or_else(|| miette::miette!("Ai.decide: missing input"))?
                .to_display_string();
            let options: Vec<String> = match find_arg(&args, "options") {
                Some(Value::List(items)) => items.iter().map(|v| v.to_display_string()).collect(),
                _ => Vec::new(),
            };
            let model = resolve_model(interp, &args);
            let role = interp.current_role();
            let rules = interp.current_rules();
            let llm = interp.runtime.llm.clone();
            match llm.decide(role.as_deref(), &rules, &input, &options, &model).await {
                Ok(Some((choice, reason))) => {
                    let mut m = HashMap::new();
                    m.insert("choice".to_string(), Value::String(choice));
                    m.insert("reason".to_string(), Value::String(reason));
                    m.insert("confidence".to_string(), Value::Float(1.0));
                    Ok(Value::Map(m))
                }
                Ok(None) => Ok(Value::None),
                Err(crate::runtime::llm::LlmError::ConfigError(msg)) => Err(miette::miette!("{msg}")),
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(interp, "AiError", &e.to_string(), None),
            }
        }),

        "prompt" => |interp, args| Box::pin(async move {
            let system = find_arg(&args, "system").map(|v| v.to_display_string()).unwrap_or_default();
            let user = find_arg(&args, "user").map(|v| v.to_display_string()).unwrap_or_default();
            let response_format = find_arg(&args, "response_format").map(|v| v.to_display_string());
            let model = resolve_model(interp, &args);
            let role = interp.current_role();
            let rules = interp.current_rules();
            let llm = interp.runtime.llm.clone();
            match llm.prompt(role.as_deref(), &rules, &system, &user, response_format, &model).await {
                Ok(Some(s)) => Ok(Value::String(s)),
                Ok(None) => Ok(Value::None),
                Err(crate::runtime::llm::LlmError::ConfigError(msg)) => Err(miette::miette!("{msg}")),
                Err(crate::runtime::llm::LlmError::CallFailed(_)) => Ok(Value::None),
                Err(e) => throw_typed_error(interp, "AiError", &e.to_string(), None),
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
fn resolve_model(interp: &Interpreter, args: &[CallArgValue]) -> String {
    if let Some(v) = find_arg(args, "using") {
        return v.to_display_string();
    }
    interp.current_model()
}

/// Extract enum variants from `as: T` (Value::Namespace(T)) by looking
/// T up in the interpreter's enum registry.
fn classify_variants(interp: &Interpreter, args: &[CallArgValue]) -> miette::Result<Vec<String>> {
    match find_arg(args, "as") {
        Some(Value::Namespace(name)) => {
            interp.enum_types.get(name).cloned().ok_or_else(|| {
                miette::miette!("Ai.classify: `as: {name}` is not a simple enum type")
            })
        }
        Some(Value::List(items)) => {
            // Inline form: `as: [low, medium, high]`
            Ok(items.iter().map(|v| v.to_display_string()).collect())
        }
        _ => Err(miette::miette!("Ai.classify: missing `as:` argument")),
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
                (k.clone(), variant_name)
            })
            .collect(),
        _ => Vec::new(),
    }
}
