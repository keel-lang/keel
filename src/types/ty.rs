//! Core type representations shared across the type-checker sub-modules.
//!
//! This module owns the resolved-type enum ([`Ty`]), the diagnostic value
//! ([`TypeError`]), and the human-readable description helper
//! ([`describe_ty`]).  Everything here is intentionally free of AST
//! references so that it can be imported by any sub-module without risk of
//! circular dependencies.

use crate::lexer::Span;

// ---------------------------------------------------------------------------
// Error shape
// ---------------------------------------------------------------------------

/// A type-checking diagnostic with an optional source location.
#[derive(Debug)]
pub struct TypeError {
    pub message: String,
    pub span: Option<Span>,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl TypeError {
    pub(crate) fn new(msg: impl Into<String>) -> Self {
        TypeError {
            message: msg.into(),
            span: None,
        }
    }

    pub(crate) fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

// ---------------------------------------------------------------------------
// Types (resolved, not AST-level)
// ---------------------------------------------------------------------------

/// The resolved type assigned to every expression by the checker.
///
/// Variants map 1-to-1 to Keel surface types with two escape hatches:
/// `Unknown` (type could not be determined cheaply — no error reported) and
/// `Dynamic` (explicitly typed `dynamic` — suppresses all type mismatches).
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Int,
    Float,
    Str,
    Bool,
    None_,
    Duration,
    Datetime,
    Uuid,
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Set(Box<Ty>),
    Struct(Vec<(String, Ty)>),
    Tuple(Vec<Ty>),
    Func(Vec<Ty>, Box<Ty>),
    /// Enum type. The second field carries the resolved type arguments for
    /// generic enums (e.g. `Pair[str, int]` → `Enum("Pair", [Str, Int])`).
    /// For non-generic enums the vec is empty.
    Enum(String, Vec<Ty>),
    /// An open database connection returned by `Db.connect`.
    DbConnection,
    /// Unresolved or unsupported — skip further checks.
    Unknown,
    Nullable(Box<Ty>),
    Dynamic,
}

impl Ty {
    pub(crate) fn strip_nullable(&self) -> &Ty {
        match self {
            Ty::Nullable(inner) => inner,
            _ => self,
        }
    }
}

// ---------------------------------------------------------------------------
// Human-readable descriptions
// ---------------------------------------------------------------------------

/// Produce a human-readable name for a resolved type — used in error messages.
pub(crate) fn describe_ty(ty: &Ty) -> String {
    match ty {
        Ty::Int => "int".into(),
        Ty::Float => "float".into(),
        Ty::Str => "str".into(),
        Ty::Bool => "bool".into(),
        Ty::None_ => "none".into(),
        Ty::Duration => "duration".into(),
        Ty::Datetime => "datetime".into(),
        Ty::Uuid => "Uuid".into(),
        Ty::List(inner) => format!("list[{}]", describe_ty(inner)),
        Ty::Map(k, v) => format!("map[{}, {}]", describe_ty(k), describe_ty(v)),
        Ty::Set(inner) => format!("set[{}]", describe_ty(inner)),
        Ty::Struct(_) => "struct".into(),
        Ty::Tuple(items) => {
            let s: Vec<String> = items.iter().map(describe_ty).collect();
            format!("({})", s.join(", "))
        }
        Ty::Func(_, _) => "function".into(),
        Ty::Enum(name, _) => name.clone(),
        Ty::DbConnection => "DbConnection".into(),
        Ty::Unknown => "unknown".into(),
        Ty::Nullable(inner) => format!("{}?", describe_ty(inner)),
        Ty::Dynamic => "dynamic".into(),
    }
}
