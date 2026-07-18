//! Shared desugaring helpers (method-call form, spread flattening, string
//! interpolation, `?.`/`??`, `when` decision trees — `designs/llvm-
//! compilation.md` §2.3 "Desugared").
//!
//! Empty in M0: the scalar subset lowered by `lower/expr.rs` and
//! `lower/stmt.rs` has no sugar to rewrite (string interpolation is
//! explicitly rejected rather than desugared — see `lower_string_lit` in
//! `expr.rs`). This module gains content in M2, when interpolation,
//! `?.`/`??`, `when`, and method-call sugar start lowering instead of
//! erroring.
