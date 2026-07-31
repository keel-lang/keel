use std::collections::HashMap;

use crate::interpreter::value::{MapKey, Value};

pub fn json_to_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::None,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::None
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => Value::List(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => {
            let mut m = HashMap::new();
            for (k, v) in obj {
                m.insert(MapKey::Str(k.clone()), json_to_value(v));
            }
            Value::Map(m)
        }
    }
}

pub fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::None => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Integer(n) => serde_json::Value::Number((*n).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Uuid(id) => serde_json::Value::String(id.clone()),
        Value::EnumVariant(_, v, _) => serde_json::Value::String(v.clone()),
        // JSON has no set type — a set serializes as an array in insertion
        // order. `json_to_value` has no inverse for it (arrays always parse
        // back as lists), so this is a one-way projection by design.
        Value::List(items) | Value::Set(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        Value::Map(m) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in m {
                obj.insert(k.to_string(), value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        Value::Struct(_, m) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in m {
                obj.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        Value::DbConnection(url, _) => serde_json::Value::String(format!("<DbConnection {url}>")),
        _ => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_null_to_value() {
        assert_eq!(json_to_value(&serde_json::Value::Null), Value::None);
    }

    #[test]
    fn json_bool_to_value() {
        assert_eq!(
            json_to_value(&serde_json::Value::Bool(true)),
            Value::Bool(true)
        );
        assert_eq!(
            json_to_value(&serde_json::Value::Bool(false)),
            Value::Bool(false)
        );
    }

    #[test]
    fn json_integer_to_value() {
        assert_eq!(json_to_value(&serde_json::json!(42)), Value::Integer(42));
    }

    #[test]
    fn json_float_to_value() {
        let v = json_to_value(&serde_json::json!(2.5));
        assert!(matches!(v, Value::Float(f) if (f - 2.5).abs() < 0.001));
    }

    #[test]
    fn json_string_to_value() {
        assert_eq!(
            json_to_value(&serde_json::json!("hello")),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn json_array_to_value() {
        let v = json_to_value(&serde_json::json!([1, 2, 3]));
        match v {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Value::Integer(1));
                assert_eq!(items[1], Value::Integer(2));
                assert_eq!(items[2], Value::Integer(3));
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn json_object_to_value() {
        let v = json_to_value(&serde_json::json!({"a": 1, "b": "x"}));
        match v {
            Value::Map(m) => {
                assert_eq!(m.get(&MapKey::Str("a".into())), Some(&Value::Integer(1)));
                assert_eq!(
                    m.get(&MapKey::Str("b".into())),
                    Some(&Value::String("x".to_string()))
                );
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[test]
    fn value_none_to_json() {
        assert_eq!(value_to_json(&Value::None), serde_json::Value::Null);
    }

    #[test]
    fn value_bool_to_json() {
        assert_eq!(
            value_to_json(&Value::Bool(true)),
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn value_integer_to_json() {
        assert_eq!(value_to_json(&Value::Integer(42)), serde_json::json!(42));
    }

    #[test]
    fn value_string_to_json() {
        assert_eq!(
            value_to_json(&Value::String("hello".to_string())),
            serde_json::json!("hello")
        );
    }

    #[test]
    fn value_list_to_json() {
        let v = Value::List(vec![Value::Integer(1), Value::Integer(2)]);
        assert_eq!(value_to_json(&v), serde_json::json!([1, 2]));
    }

    #[test]
    fn json_roundtrip_preserves_structure() {
        let original = serde_json::json!({
            "name": "test",
            "count": 5,
            "active": true,
            "tags": ["a", "b"]
        });
        let value = json_to_value(&original);
        let json = value_to_json(&value);
        assert_eq!(json, original);
    }
}
