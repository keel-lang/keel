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
    /// A trailing task/closure argument (`control.with_timeout(duration, fn)`,
    /// `schedule.every(interval, fn)`, `http.serve(port, fn)`, …). The
    /// runtime matches these by value shape (`find_fn_value`) rather than by
    /// name or position, so there is no scalar/collection shape to declare —
    /// this variant exists purely so the parameter is *present* in `params`
    /// (arity-visible to docs/hover/future checker work) instead of being
    /// silently absent like an undeclared param.
    Callback,
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
// ParamBinding
// ---------------------------------------------------------------------------

/// How a parameter's argument is looked up by the runtime, independent of
/// whether it's required.
///
/// The naive assumption — required params are read positionally, optional
/// ones by name — holds for some namespaces (`file`, `cache`'s `key`) but not
/// others: `email.send`'s `to` is required *and* named-only
/// (`find_arg(args, "to")`, no positional fallback), while `http.serve`'s
/// `port` is optional *and* positional-only (`positional(args, 0)`, defaults
/// when absent). This field records the runtime's actual lookup strategy so
/// the two axes (required/optional, positional/named) don't get conflated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamBinding {
    /// Only a bare positional argument at this parameter's declared slot
    /// binds. The runtime reads it via `positional(args, idx)`
    /// (`expect_str`/`expect_int`/`expect_list`/`expect_duration` and their
    /// hand-rolled equivalents) — a `name: value` form is invisible to it.
    PositionalOnly,
    /// Only `name: value` binds. The runtime reads it via
    /// `find_arg(args, name)` (`expect_*_named` and hand-rolled equivalents)
    /// with no positional fallback — a bare positional value is invisible to
    /// it.
    NamedOnly,
    /// Either form binds. The runtime checks the name first and falls back
    /// to position (`find_arg(args, name).or_else(|| positional(args, idx))`),
    /// e.g. `crypto.token`'s `bytes` and `uuid.v5`'s `ns`.
    Either,
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
    ///
    /// Convention: `false` when the call is not meaningful without it, even
    /// if the runtime currently degrades silently instead of erroring on
    /// omission (e.g. `io.ask`'s `prompt`, `ai.prompt`'s `system`/`user`) —
    /// that silent-default is treated as a runtime bug to fix, not a
    /// contract to document as optional. `true` is reserved for a
    /// deliberate default (`http.serve`'s `port` defaulting to 8080,
    /// `cache.set`'s `ttl` meaning "never expire").
    pub optional: bool,
    /// How the runtime actually looks this argument up. See [`ParamBinding`].
    pub binding: ParamBinding,
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
    /// Stable id for this method within its namespace, consumed by
    /// `keel-kir`'s `CallTarget::Ns` lowering and `keel-rt-ffi`'s
    /// `keel_rt_call_ns` dispatch. Assigned once per method and never
    /// reused: removing a method leaves a gap rather than shifting the ids
    /// of the methods after it. Unique only within a namespace — pair with
    /// [`crate::specs::namespace_id`] for a globally unique id.
    pub method_id: u16,
    /// Declared parameter list. Empty `&[]` means zero or variadic
    /// parameters that are not statically validated by the checker yet.
    #[allow(dead_code)]
    pub params: &'static [BuiltinParam],
    /// How to compute the return type.
    pub result: BuiltinResult,
    /// One-sentence description, shown in LSP hover and generated docs.
    pub doc: &'static str,
}
