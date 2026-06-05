//! Heap-free descriptor types for built-in namespace methods.
//!
//! This is a leaf module — it imports nothing from `types` or `runtime`.
//! Both the checker (`types::prelude`) and the runtime (`runtime::namespaces`)
//! import from here, keeping the type definitions neutral.

// ---------------------------------------------------------------------------
// TySpec
// ---------------------------------------------------------------------------

/// A flat, `Copy`-friendly representation of a built-in return type.
///
/// Avoids heap allocation so [`BuiltinMethod`] entries can live in `static`
/// storage. Convert to the full checker `Ty` with `types::prelude::ty_from_spec`.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TySpec {
    // Primitives
    Int,
    Float,
    Str,
    Bool,
    None_,
    Datetime,
    Duration,
    Uuid,
    Dynamic,
    DbConnection,
    // Nullable primitives
    NullableStr,
    NullableInt,
    NullableFloat,
    NullableUuid,
    NullableDatetime,
    NullableDynamic,
    // Collection types
    ListOfStr,
    ListOfInt,
    ListOfListOfStr,
    /// `list[map[str, str]]` — Csv.parse_records
    ListOfMapStrStr,
    /// `list[map[str, dynamic]]` — Db.query
    ListOfMapStrDynamic,
    /// Caller must handle this case contextually (type is unknown statically).
    Unknown,
}

// ---------------------------------------------------------------------------
// BuiltinResult
// ---------------------------------------------------------------------------

/// Describes how to compute the return type of a built-in namespace method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinResult {
    /// The return type is a fixed, statically-known [`TySpec`].
    Fixed(TySpec),
    /// `Ai.extract` / `Ai.decide`: `Nullable(resolve_type(as:))`.
    ///
    /// The checker must inspect the `as:` named argument and resolve its
    /// type from the current scope.
    AiExtract,
    /// `Ai.classify`: `Nullable(Enum(as:))`.
    ///
    /// The checker must inspect the `as:` named argument, look up the
    /// named enum in the current scope, and return `Nullable(Enum(name))`.
    AiClassify,
    /// The return type depends on runtime context and cannot be determined
    /// statically. The checker should produce `Ty::Unknown`.
    Unknown,
}

// ---------------------------------------------------------------------------
// BuiltinParam
// ---------------------------------------------------------------------------

/// A single parameter in a built-in namespace method signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltinParam {
    /// Parameter name as it appears in Keel source (e.g., `"path"`).
    pub name: &'static str,
    /// Declared type of the parameter.
    pub ty: TySpec,
    /// Whether the parameter may be omitted by the caller.
    pub optional: bool,
}

// ---------------------------------------------------------------------------
// BuiltinMethod
// ---------------------------------------------------------------------------

/// Describes a single method exposed by a stdlib namespace.
///
/// All surfaces that enumerate namespace methods (checker, LSP, docs) must
/// derive their lists from the runtime catalog rather than maintaining
/// independent copies.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinMethod {
    /// The namespace that owns this method (e.g., `"File"`).
    pub namespace: &'static str,
    /// The method name as it appears in Keel source (e.g., `"read"`).
    pub name: &'static str,
    /// Declared parameter list. Empty `&[]` means zero or variadic
    /// parameters that are not statically validated by the checker yet.
    #[allow(dead_code)]
    pub params: &'static [BuiltinParam],
    /// How to compute the return type.
    pub result: BuiltinResult,
    /// One-sentence description, shown in LSP hover and generated docs.
    pub doc: &'static str,
}
