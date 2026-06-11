use std::collections::HashSet;
use std::sync::Arc;

use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::value::{MapKey, Value};
use crate::interpreter::{CallArgValue, Namespace, RuntimeErrorKind};
use crate::runtime::args::expect_str;
use crate::runtime::convert::{json_to_value, value_to_json};
use crate::runtime::namespace::{make_typed_report, ns, positional};

pub(crate) const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "json",
        name: "parse",
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        // Unknown (ExternalDynamic): the JSON structure depends on runtime input and is
        // not statically knowable. `keel check --strict` will flag unannotated bindings;
        // users silence this with an explicit `let x: dynamic = Json.parse(...)`.
        result: BuiltinResult::Unknown,
        doc: "Parse a JSON string into a dynamic value.",
    },
    BuiltinMethod {
        namespace: "json",
        name: "stringify",
        params: &[BuiltinParam {
            name: "value",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Serialize a value to a JSON string.",
    },
];

pub(crate) fn namespace() -> Namespace {
    ns!("json", {
        // Json.parse(str) — deserialize a JSON string into a Keel value.
        // Raises JsonError on invalid input.
        "parse" => |_i, args| Box::pin(async move {
            let json_str = expect_str(&args, 0, "json.parse")?;

            match serde_json::from_str::<serde_json::Value>(json_str) {
                Ok(json_val) => Ok(json_to_value(&json_val)),
                Err(e) => Err(make_typed_report(RuntimeErrorKind::Json, format!("json.parse: invalid JSON: {e}"))),
            }
        }),
        // Json.stringify(value) — serialize a Keel value to a JSON string.
        // If the value implements Serializable, calls to_json() instead.
        "stringify" => |host, args| Box::pin(async move {
            let value = positional(&args, 0)
                .cloned()
                .ok_or_else(|| miette::miette!("json.stringify: missing argument"))?;

            // For untagged Value::Map values, attempt exact field-set promotion to a
            // named struct so that user-defined Serializable impls are reachable even
            // when the caller didn't bind the literal to a typed variable.  Only
            // promote when there is exactly one struct type whose string-key field set
            // matches the map — ambiguous or zero matches fall through to the built-in
            // serializer.
            //
            // Owned match avoids a borrow-then-re-match; hoisting the TaskDecl from the
            // probe eliminates a second find_impl_task lookup in the promotion path.
            let value = match value {
                Value::Map(m) => {
                    // Only attempt promotion when every key is a string — maps with
                    // integer or bool keys can never match any struct field schema.
                    let all_str_keys = m.keys().all(|k| matches!(k, MapKey::Str(_)));
                    'promo: {
                        if !all_str_keys {
                            break 'promo;
                        }
                        let map_keys: HashSet<&str> = m
                            .keys()
                            .map(|k| {
                                let MapKey::Str(s) = k else { unreachable!() };
                                s.as_str()
                            })
                            .collect();
                        // Count only struct types that implement Serializable (have to_json),
                        // so that a non-serializable struct with identical fields doesn't
                        // make the match ambiguous and suppress the custom serializer.
                        let mut unique: Option<(String, Arc<crate::ast::TaskDecl>)> = None;
                        for (name, schema) in host.struct_types() {
                            let schema_keys: HashSet<&str> =
                                schema.iter().map(|(k, _)| k.as_str()).collect();
                            if schema_keys != map_keys {
                                continue;
                            }
                            let probe = Value::Struct(
                                name.clone(),
                                std::collections::HashMap::new(),
                            );
                            let Some(task) = host.find_impl_task(&probe, "to_json") else {
                                continue;
                            };
                            if unique.is_some() {
                                unique = None;
                                break;
                            }
                            unique = Some((name.clone(), task));
                        }
                        if let Some((type_name, task)) = unique {
                            let promoted = Value::Struct(
                                type_name,
                                m.into_iter()
                                    .map(|(k, v)| {
                                        let MapKey::Str(s) = k else { unreachable!() };
                                        (s, v)
                                    })
                                    .collect(),
                            );
                            return host
                                .call_task(
                                    "to_json",
                                    &task,
                                    vec![CallArgValue { name: None, value: promoted }],
                                )
                                .await;
                        }
                    }
                    Value::Map(m)
                }
                other => other,
            };

            // Delegate to Serializable.to_json() if available (handles Value::Struct).
            if let Some(task) = host.find_impl_task(&value, "to_json") {
                return host
                    .call_task("to_json", &task, vec![CallArgValue { name: None, value }])
                    .await;
            }

            let json_val = value_to_json(&value);
            match serde_json::to_string(&json_val) {
                Ok(json_str) => Ok(Value::String(json_str)),
                Err(e) => Err(make_typed_report(RuntimeErrorKind::Json, format!("json.stringify: serialization failed: {e}"))),
            }
        }),
    })
}
