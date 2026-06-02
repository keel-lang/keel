//! Core type representations shared across the type-checker sub-modules.
//!
//! This module owns the resolved-type enum ([`Ty`]) and the human-readable
//! description helper ([`describe_ty`]).  Everything here is intentionally free
//! of AST references so that it can be imported by any sub-module without risk
//! of circular dependencies.

// ---------------------------------------------------------------------------
// Types (resolved, not AST-level)
// ---------------------------------------------------------------------------

/// The reason the checker could not infer or resolve a type.
///
/// Used by [`Ty::Unknown`] to distinguish checker limitations from external
/// dynamism, enabling precise strict-mode diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnknownReason {
    /// The checker has not yet implemented inference for this construct.
    ///
    /// Like all `Unknown(_)` reasons, this fires a "cannot infer type" error
    /// in strict mode (`keel check --strict`).
    UnsupportedFeature(&'static str),
    /// A built-in namespace method whose return type depends on runtime
    /// context (LLM outputs, JSON payloads, external API responses, etc.).
    ExternalDynamic,
    /// Shallow inference: the checker could not propagate the type cheaply.
    InferenceLimitation,
}

/// The resolved type assigned to every expression by the checker.
///
/// Concrete variants map 1-to-1 to Keel surface types.  Four escape hatches
/// handle situations where the type is absent or unknowable:
///
/// | Variant | Meaning | Generates cascade errors? |
/// |---|---|---|
/// | `Dynamic` | User-written `dynamic` annotation | No |
/// | `Unknown(reason)` | Checker limitation or external dynamism | No |
/// | `Error` | An error was already reported at this site | No |
/// | `Unresolved(name)` | Type name written but never declared | No |
///
/// All four are considered *opaque* (see [`Ty::is_opaque`]) and suppress
/// further diagnostics so a single root error does not flood output.
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
    Struct {
        /// `Some(name)` for a named struct type (`type Score { val: int }`);
        /// `None` for inline / anonymous struct shapes (`{body: str, from: str}`).
        name: Option<String>,
        fields: Vec<(String, Ty)>,
    },
    Tuple(Vec<Ty>),
    Func(Vec<Ty>, Box<Ty>),
    /// Enum type.  The second field carries resolved type arguments for
    /// generic enums (`Pair[str, int]` → `Enum("Pair", [Str, Int])`).
    /// For non-generic enums the vec is empty.
    Enum(String, Vec<Ty>),
    /// An open database connection returned by `Db.connect`.
    DbConnection,
    /// Explicitly typed `dynamic` — the user opted out of static typing.
    ///
    /// Accepted everywhere without generating warnings even in strict mode.
    Dynamic,
    /// An error was already reported at this expression site.
    ///
    /// Suppresses cascade errors so a single root cause does not flood
    /// the diagnostic output.
    Error,
    /// A named type that was written in source but was never declared.
    ///
    /// Kept silent (no new diagnostic is emitted on creation) to preserve
    /// existing behaviour.  Downstream checks treat it like `Error`.
    Unresolved(String),
    /// The checker could not determine the type for the given reason.
    ///
    /// See [`UnknownReason`] for the full taxonomy.  In strict mode
    /// (`keel check --strict`), **any** `Unknown(_)` binding triggers a
    /// "cannot infer type" error.  [`Dynamic`] is never flagged — it
    /// represents an intentional programmer choice, not a checker gap.
    Unknown(UnknownReason),
    Nullable(Box<Ty>),
}

impl Ty {
    /// Strip a `Nullable` wrapper, returning the inner type.
    ///
    /// Returns `self` unchanged for all non-nullable variants.
    pub(crate) fn strip_nullable(&self) -> &Ty {
        match self {
            Ty::Nullable(inner) => inner,
            _ => self,
        }
    }

    /// Returns `true` when the type encodes an inference gap or prior error.
    ///
    /// Opaque types suppress cascade diagnostics and are accepted in any
    /// position where a concrete type is expected.  The four opaque variants
    /// are `Dynamic`, `Unknown(_)`, `Error`, and `Unresolved(_)`.
    #[inline]
    pub(crate) fn is_opaque(&self) -> bool {
        matches!(
            self,
            Ty::Dynamic | Ty::Unknown(_) | Ty::Error | Ty::Unresolved(_)
        )
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
        Ty::Struct { name: Some(n), .. } => n.clone(),
        Ty::Struct { .. } => "struct".into(),
        Ty::Tuple(items) => {
            let s: Vec<String> = items.iter().map(describe_ty).collect();
            format!("({})", s.join(", "))
        }
        Ty::Func(_, _) => "function".into(),
        Ty::Enum(name, _) => name.clone(),
        Ty::DbConnection => "DbConnection".into(),
        Ty::Dynamic => "dynamic".into(),
        Ty::Error => "unknown".into(),
        Ty::Unresolved(name) => name.clone(),
        Ty::Unknown(_) => "unknown".into(),
        Ty::Nullable(inner) => format!("{}?", describe_ty(inner)),
    }
}
