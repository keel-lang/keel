use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::convert::{json_to_value, value_to_json};
use crate::runtime::namespace::{ns, positional};

pub(crate) fn namespace() -> Namespace {
    ns!("Json", {
        // Json.parse(str) — deserialize a JSON string into a Keel value.
        // Raises JsonError on invalid input.
        "parse" => |_i, args| Box::pin(async move {
            let json_str = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Json.parse: missing argument"))?;

            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(json_val) => Ok(json_to_value(&json_val)),
                Err(e) => Err(miette::miette!("JsonError: Json.parse invalid JSON: {e}")),
            }
        }),
        // Json.stringify(value) — serialize a Keel value to a JSON string.
        "stringify" => |_i, args| Box::pin(async move {
            let value = positional(&args, 0)
                .cloned()
                .ok_or_else(|| miette::miette!("Json.stringify: missing argument"))?;

            let json_val = value_to_json(&value);
            match serde_json::to_string(&json_val) {
                Ok(json_str) => Ok(Value::String(json_str)),
                Err(e) => Err(miette::miette!("JsonError: Json.stringify serialization failed: {e}")),
            }
        }),
    })
}
