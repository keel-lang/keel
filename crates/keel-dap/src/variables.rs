//! `Environment`/agent-state → DAP `scopes`/`variables` tree.
//!
//! Values are snapshotted into a per-pause arena keyed by an opaque
//! `variablesReference`: DAP expands nested structures lazily, one
//! `variables` request per level, so each compound child gets a fresh
//! reference the first time its parent is expanded.

use std::collections::HashMap;

use keel_runtime::interpreter::value::{MapKey, Value};

use crate::protocol::DapVariable;

/// Per-pause snapshot arena. Reset on every new stop — reference ids are
/// only meaningful for the lifetime of one paused frame.
#[derive(Default)]
pub struct VariablesArena {
    next_ref: i64,
    nodes: HashMap<i64, Value>,
}

impl VariablesArena {
    pub fn new() -> Self {
        Self {
            next_ref: 1,
            nodes: HashMap::new(),
        }
    }

    /// Register a value, returning the `variablesReference` DAP should use
    /// to expand it — `0` for a leaf (DAP's "no children" sentinel).
    pub fn register(&mut self, value: Value) -> i64 {
        if !has_children(&value) {
            return 0;
        }
        let id = self.next_ref;
        self.next_ref += 1;
        self.nodes.insert(id, value);
        id
    }

    /// Build the DAP variable rows for a previously-registered reference.
    pub fn variables_for(&mut self, reference: i64) -> Vec<DapVariable> {
        let Some(value) = self.nodes.get(&reference).cloned() else {
            return Vec::new();
        };
        expand(self, &value)
    }
}

/// A lazy range is deliberately never expanded into elements (it may be
/// unbounded/huge) — it always renders as a leaf via `Value`'s `Display`.
fn has_children(value: &Value) -> bool {
    match value {
        Value::List(items) | Value::Set(items) => !items.is_empty(),
        Value::Map(m) => !m.is_empty(),
        Value::Struct(_, fields) => !fields.is_empty(),
        Value::EnumVariant(_, _, Some(fields)) => !fields.is_empty(),
        _ => false,
    }
}

fn expand(arena: &mut VariablesArena, value: &Value) -> Vec<DapVariable> {
    match value {
        // A set expands under its insertion-order indices, same as a list —
        // the index is a stable handle for the debugger, not a claim that
        // sets are ordered by position.
        Value::List(items) | Value::Set(items) => items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let ty = item.type_name().to_string();
                DapVariable {
                    name: i.to_string(),
                    value: item.to_string(),
                    ty: Some(ty),
                    variables_reference: arena.register(item.clone()),
                }
            })
            .collect(),
        Value::Map(m) => {
            let mut pairs: Vec<(&MapKey, &Value)> = m.iter().collect();
            pairs.sort_by_key(|(k, _)| (*k).clone());
            pairs
                .into_iter()
                .map(|(k, v)| DapVariable {
                    name: k.to_string(),
                    value: v.to_string(),
                    ty: Some(v.type_name().to_string()),
                    variables_reference: arena.register(v.clone()),
                })
                .collect()
        }
        Value::Struct(_, fields) => named_fields(arena, fields),
        Value::EnumVariant(_, _, Some(fields)) => named_fields(arena, fields),
        _ => Vec::new(),
    }
}

fn named_fields(arena: &mut VariablesArena, fields: &HashMap<String, Value>) -> Vec<DapVariable> {
    let mut names: Vec<&String> = fields.keys().collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let v = &fields[name];
            DapVariable {
                name: name.clone(),
                value: v.to_string(),
                ty: Some(v.type_name().to_string()),
                variables_reference: arena.register(v.clone()),
            }
        })
        .collect()
}

/// Build the top-level "Locals" variable rows from a paused frame's
/// `Environment::scopes()`, innermost first — the first binding seen for a
/// given name is the one currently visible (shadowing), matching
/// `Environment::get`'s own resolution order.
pub fn locals_from_env<'a>(
    arena: &mut VariablesArena,
    scopes: impl Iterator<Item = &'a HashMap<String, Value>>,
) -> Vec<DapVariable> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for scope in scopes {
        let mut names: Vec<&String> = scope.keys().collect();
        names.sort();
        for name in names {
            if !seen.insert(name.clone()) {
                continue;
            }
            let v = &scope[name];
            out.push(DapVariable {
                name: name.clone(),
                value: v.to_string(),
                ty: Some(v.type_name().to_string()),
                variables_reference: arena.register(v.clone()),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_values_get_zero_reference() {
        let mut arena = VariablesArena::new();
        assert_eq!(arena.register(Value::Integer(1)), 0);
        assert_eq!(arena.register(Value::String("x".into())), 0);
        assert_eq!(arena.register(Value::List(vec![])), 0);
    }

    #[test]
    fn list_gets_reference_and_expands_by_index() {
        let mut arena = VariablesArena::new();
        let list = Value::List(vec![Value::Integer(1), Value::Integer(2)]);
        let reference = arena.register(list);
        assert_ne!(reference, 0);
        let vars = arena.variables_for(reference);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].name, "0");
        assert_eq!(vars[0].value, "1");
        assert_eq!(vars[1].name, "1");
    }

    #[test]
    fn struct_fields_are_sorted_and_expandable() {
        let mut arena = VariablesArena::new();
        let mut fields = HashMap::new();
        fields.insert("b".to_string(), Value::Integer(2));
        fields.insert("a".to_string(), Value::Integer(1));
        let reference = arena.register(Value::Struct("Point".into(), fields));
        let vars = arena.variables_for(reference);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].name, "a");
        assert_eq!(vars[1].name, "b");
    }

    #[test]
    fn range_never_expands_even_though_conceptually_iterable() {
        let mut arena = VariablesArena::new();
        assert_eq!(arena.register(Value::Range(0, 1_000_000)), 0);
    }

    #[test]
    fn locals_from_env_respects_innermost_shadowing() {
        let mut outer = HashMap::new();
        outer.insert("x".to_string(), Value::Integer(1));
        let mut inner = HashMap::new();
        inner.insert("x".to_string(), Value::Integer(2));
        let scopes: Vec<&HashMap<String, Value>> = vec![&inner, &outer];
        let mut arena = VariablesArena::new();
        let vars = locals_from_env(&mut arena, scopes.into_iter());
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].value, "2");
    }
}
