//! Lexical scope stack used by the type checker.
//!
//! The scope is a simple stack of frames.  The innermost frame (back of the
//! vec) is pushed on block entry and popped on block exit.  Name lookup
//! walks frames from back to front so inner bindings shadow outer ones.

use std::collections::{HashMap, HashSet};

use crate::types::ty::Ty;

/// Chained lexical scope: newer scopes on the back of the vec.
pub(crate) struct Scope {
    pub(crate) frames: Vec<HashMap<String, Ty>>,
    /// Parallel to `frames`: names whose *current* binding in that same
    /// frame is known, with certainty, to be a lambda literal's own
    /// inferred type — set only by [`Scope::define_exact_lambda`], cleared
    /// for a name by every plain [`Scope::define`] (so `x = (a,b)=>a+b; x =
    /// f` un-marks `x`).
    ///
    /// This can't be folded into `Ty::Func` itself (issue #238 originally
    /// tried exactly that, via a trailing `bool`): several checker sites
    /// approximate a merged type from just one of its sources — a list
    /// literal's element type is its first element's type, a `for` loop's
    /// element type is its iterable's — and any of them would silently
    /// launder a non-exact `Ty::Func` (a named task exposed as a value,
    /// which may hide a defaulted or variadic param) into one indistinguishable
    /// from a real lambda's. Tracking exactness here instead, as a fact
    /// about *this specific name's current binding* rather than data
    /// carried on the type, means it only exists where it was explicitly
    /// proven — nothing to launder.
    exact_lambda: Vec<HashSet<String>>,
    /// `frames` indices at which a lambda body begins, innermost last.
    /// Lambdas are non-capturing (mirrors the HIR lowerer's identical
    /// restriction): [`Scope::get`] never looks below the innermost boundary.
    lambda_boundaries: Vec<usize>,
}

impl Scope {
    pub(crate) fn new() -> Self {
        Scope {
            frames: vec![HashMap::new()],
            exact_lambda: vec![HashSet::new()],
            lambda_boundaries: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self) {
        self.frames.push(HashMap::new());
        self.exact_lambda.push(HashSet::new());
    }

    pub(crate) fn pop(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
            self.exact_lambda.pop();
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
        if let Some(s) = self.exact_lambda.last_mut() {
            s.remove(&name);
        }
        if let Some(f) = self.frames.last_mut() {
            f.insert(name, ty);
        }
    }

    /// Like [`Scope::define`], but also marks `name` as exactly a lambda
    /// literal's own type in the current frame — see `exact_lambda`'s doc.
    /// Callers must only pass a `ty` that was itself inferred directly from
    /// an `Expr::Lambda` node, with no intervening merge.
    pub(crate) fn define_exact_lambda(&mut self, name: String, ty: Ty) {
        if let Some(f) = self.frames.last_mut() {
            f.insert(name.clone(), ty);
        }
        if let Some(s) = self.exact_lambda.last_mut() {
            s.insert(name);
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

    /// Whether `name`'s *current* binding is known to be exactly a lambda
    /// literal's own type — see `exact_lambda`'s doc. Walks frames with the
    /// same shadowing/lambda-boundary rules as [`Scope::get`], so a
    /// non-lambda binding that shadows an outer lambda-marked one correctly
    /// reads as not-exact.
    pub(crate) fn is_exact_lambda(&self, name: &str) -> bool {
        let boundary = self.lambda_boundaries.last().copied().unwrap_or(0);
        for i in (boundary..self.frames.len()).rev() {
            if self.frames[i].contains_key(name) {
                return self.exact_lambda[i].contains(name);
            }
        }
        false
    }
}
