use std::collections::HashMap;

use super::value::Value;

/// A lexical scope for variable bindings.
/// Environments form a chain: each scope has an optional parent.
#[derive(Debug, Clone)]
pub struct Environment {
    scopes: Vec<HashMap<String, Value>>,
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

impl Environment {
    pub fn new() -> Self {
        Environment {
            scopes: vec![HashMap::with_capacity(8)],
        }
    }

    /// Push a new scope (entering a block/task/agent).
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::with_capacity(8));
    }

    /// Pop the current scope (leaving a block/task/agent).
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Define a variable in the current (innermost) scope.
    pub fn define(&mut self, name: String, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
        }
    }

    /// Look up a variable, searching from innermost scope outward.
    pub fn get(&self, name: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Some(val);
            }
        }
        None
    }

    /// Update an existing variable in the nearest scope that contains it.
    /// Returns false if the variable doesn't exist.
    pub fn set(&mut self, name: &str, value: Value) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return true;
            }
        }
        false
    }

    #[cfg(test)]
    pub fn top_scope_names(&self) -> Vec<String> {
        self.scopes
            .first()
            .map(|s| s.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Enumerate scopes from innermost (current block) to outermost (root),
    /// for a debugger's nested "scopes"/"variables" tree.
    pub fn scopes(&self) -> impl DoubleEndedIterator<Item = &HashMap<String, Value>> {
        self.scopes.iter().rev()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_and_get_in_single_scope() {
        let mut env = Environment::new();
        env.define("x".to_string(), Value::Integer(42));
        assert_eq!(env.get("x"), Some(&Value::Integer(42)));
        assert_eq!(env.get("y"), None);
    }

    #[test]
    fn inner_scope_shadows_outer() {
        let mut env = Environment::new();
        env.define("x".to_string(), Value::Integer(1));
        env.push_scope();
        env.define("x".to_string(), Value::Integer(2));
        assert_eq!(env.get("x"), Some(&Value::Integer(2)));
        env.pop_scope();
        assert_eq!(env.get("x"), Some(&Value::Integer(1)));
    }

    #[test]
    fn set_updates_nearest_scope() {
        let mut env = Environment::new();
        env.define("x".to_string(), Value::Integer(1));
        env.push_scope();
        assert!(env.set("x", Value::Integer(99)));
        env.pop_scope();
        assert_eq!(env.get("x"), Some(&Value::Integer(99)));
    }

    #[test]
    fn set_returns_false_for_undefined() {
        let mut env = Environment::new();
        assert!(!env.set("missing", Value::Bool(true)));
    }

    #[test]
    fn pop_scope_never_removes_root() {
        let mut env = Environment::new();
        env.pop_scope();
        env.define("x".to_string(), Value::Integer(7));
        assert_eq!(env.get("x"), Some(&Value::Integer(7)));
    }

    #[test]
    fn top_scope_names_lists_root_bindings() {
        let mut env = Environment::new();
        env.define("a".to_string(), Value::Bool(true));
        env.push_scope();
        env.define("b".to_string(), Value::Bool(false));
        let names = env.top_scope_names();
        assert!(names.contains(&"a".to_string()));
        assert!(!names.contains(&"b".to_string()));
    }
}
