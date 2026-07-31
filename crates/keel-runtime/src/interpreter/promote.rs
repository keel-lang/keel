use std::collections::HashMap;

use crate::ast::TypeExpr;

use super::value::{MapKey, Value};

/// Promote a `Value` to its type-tagged form based on the declared `TypeExpr`.
///
/// Handles:
/// - `Named(T)` → promotes `Value::Map` (string-keyed only) to `Value::Struct(T, …)`
/// - `List(Named(T))` → promotes each `Value::Map` element in a `Value::List`
/// - `Set(Named(T))` → promotes each `Value::Map` element in a `Value::Set`
/// - `Map(_, Named(T))` → promotes each value inside a `Value::Map` (for `map[K, T]`)
/// - `Nullable(inner)` → peels the wrapper and recurses on `inner`
///
/// Non-string-keyed maps are never promoted — integer or bool keys cannot be
/// valid struct field names and the result would be silently inaccessible.
///
/// An already-tagged `Value::Struct` is returned unchanged regardless of whether
/// its name matches `type_name`. Renaming a live struct at a promotion boundary
/// would corrupt type identity; the checker enforces name compatibility statically.
pub(crate) fn promote_value(
    v: Value,
    ty: &TypeExpr,
    struct_types: &HashMap<String, Vec<(String, String)>>,
    struct_aliases: &HashMap<String, String>,
) -> Value {
    match ty {
        TypeExpr::Named(type_name) => match struct_aliases
            .get(type_name.as_str())
            .unwrap_or(type_name)
        {
            canonical if struct_types.contains_key(canonical.as_str()) => match v {
                // Only promote when every key is a string — integer/bool keys produce
                // field names like "1" or "true" that can never be accessed via normal
                // field-access syntax.
                Value::Map(m) if m.keys().all(|k| matches!(k, MapKey::Str(_))) => Value::Struct(
                    canonical.clone(),
                    m.into_iter()
                        .map(|(k, v)| {
                            let MapKey::Str(key) = k else { unreachable!() };
                            (key, v)
                        })
                        .collect(),
                ),
                // An already-tagged struct (or any other value shape) is returned as-is.
                other => other,
            },
            _ => v,
        },
        TypeExpr::List(inner) => match v {
            Value::List(items) => Value::List(
                items
                    .into_iter()
                    .map(|item| promote_value(item, inner, struct_types, struct_aliases))
                    .collect(),
            ),
            other => other,
        },
        // `set[Named(T)]` promotes element-wise like `list[Named(T)]`.
        // Promotion cannot introduce duplicates that weren't already there:
        // two maps equal after tagging were equal before it, and the set was
        // deduplicated at construction.
        TypeExpr::Set(inner) => match v {
            Value::Set(items) => Value::Set(
                items
                    .into_iter()
                    .map(|item| promote_value(item, inner, struct_types, struct_aliases))
                    .collect(),
            ),
            other => other,
        },
        TypeExpr::Map(_, v_expr) => match v {
            Value::Map(m) => Value::Map(
                m.into_iter()
                    .map(|(k, val)| (k, promote_value(val, v_expr, struct_types, struct_aliases)))
                    .collect(),
            ),
            other => other,
        },
        TypeExpr::Tuple(type_items) => match v {
            // Tuples are represented as Value::List at runtime.
            Value::List(items) => Value::List(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, item)| {
                        if let Some(ty) = type_items.get(i) {
                            promote_value(item, ty, struct_types, struct_aliases)
                        } else {
                            item
                        }
                    })
                    .collect(),
            ),
            other => other,
        },
        TypeExpr::Nullable(inner) => promote_value(v, inner, struct_types, struct_aliases),
        _ => v,
    }
}
