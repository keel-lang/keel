//! Lexical scope stack used by the type checker.
//!
//! The scope is a simple stack of frames.  The innermost frame (back of the
//! vec) is pushed on block entry and popped on block exit.  Name lookup
//! walks frames from back to front so inner bindings shadow outer ones.

use std::collections::HashMap;

use crate::types::ty::Ty;

/// Chained lexical scope: newer scopes on the back of the vec.
pub(crate) struct Scope {
    pub(crate) frames: Vec<HashMap<String, Ty>>,
}

impl Scope {
    pub(crate) fn new() -> Self {
        Scope {
            frames: vec![HashMap::new()],
        }
    }

    pub(crate) fn push(&mut self) {
        self.frames.push(HashMap::new());
    }

    pub(crate) fn pop(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    pub(crate) fn define(&mut self, name: String, ty: Ty) {
        if let Some(f) = self.frames.last_mut() {
            f.insert(name, ty);
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Ty> {
        for f in self.frames.iter().rev() {
            if let Some(t) = f.get(name) {
                return Some(t);
            }
        }
        None
    }
}
