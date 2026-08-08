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
    /// `frames` indices at which a lambda body begins, innermost last.
    /// Lambdas are non-capturing (mirrors the HIR lowerer's identical
    /// restriction): [`Scope::get`] never looks below the innermost boundary.
    lambda_boundaries: Vec<usize>,
}

impl Scope {
    pub(crate) fn new() -> Self {
        Scope {
            frames: vec![HashMap::new()],
            lambda_boundaries: Vec::new(),
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

    /// Enter a lambda body: like [`Scope::push`], but also records a
    /// capture boundary so [`Scope::get`] stops at this frame.
    pub(crate) fn push_lambda(&mut self) {
        self.lambda_boundaries.push(self.frames.len());
        self.push();
    }

    /// Leave a lambda body pushed by [`Scope::push_lambda`].
    pub(crate) fn pop_lambda(&mut self) {
        self.pop();
        self.lambda_boundaries.pop();
    }

    pub(crate) fn define(&mut self, name: String, ty: Ty) {
        if let Some(f) = self.frames.last_mut() {
            f.insert(name, ty);
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Ty> {
        let boundary = self.lambda_boundaries.last().copied().unwrap_or(0);
        for f in self.frames[boundary..].iter().rev() {
            if let Some(t) = f.get(name) {
                return Some(t);
            }
        }
        None
    }
}
