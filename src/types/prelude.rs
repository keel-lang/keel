//! Prelude catalog — identifiers and built-in interfaces known to the checker
//! before user code is examined.
//!
//! Keeping this in one place prevents checker, runtime, LSP, and docs from
//! drifting independently.  When a new stdlib namespace or built-in type name
//! is added, this file is the single source of truth.
//!
//! # Namespace method catalog
//!
//! [`catalog()`] returns a `'static` slice of [`BuiltinMethod`] entries.
//! Every surface that needs to know about stdlib methods — the checker,
//! the LSP, and the docs generator — must derive that knowledge from this
//! slice rather than maintaining its own independent list.
//!
//! To convert a [`TySpec`] into the runtime [`Ty`] used by the checker,
//! call [`ty_from_spec`].

use std::collections::{HashMap, HashSet};

use crate::ast::{Binding, Node, Param, TaskSig, TypeExpr};
use crate::types::ty::{Ty, UnknownReason};

// ---------------------------------------------------------------------------
// TySpec — heap-free type representation for static catalog entries
// ---------------------------------------------------------------------------

/// A flat, `Copy`-friendly representation of a built-in return type.
///
/// Avoids heap allocation so [`BuiltinMethod`] entries can live in `static`
/// storage.  Convert to the full [`Ty`] with [`ty_from_spec`].
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
    /// `list[map[str, str]]`  — Csv.parse_records
    ListOfMapStrStr,
    /// `list[map[str, dynamic]]` — Db.query
    ListOfMapStrDynamic,
    /// Caller must handle this case contextually (type is unknown statically).
    Unknown,
}

/// Convert a [`TySpec`] into the full [`Ty`] used by the checker.
pub(crate) fn ty_from_spec(spec: TySpec) -> Ty {
    match spec {
        TySpec::Int => Ty::Int,
        TySpec::Float => Ty::Float,
        TySpec::Str => Ty::Str,
        TySpec::Bool => Ty::Bool,
        TySpec::None_ => Ty::None_,
        TySpec::Datetime => Ty::Datetime,
        TySpec::Duration => Ty::Duration,
        TySpec::Uuid => Ty::Uuid,
        TySpec::Dynamic => Ty::Dynamic,
        TySpec::DbConnection => Ty::DbConnection,
        TySpec::NullableStr => Ty::Nullable(Box::new(Ty::Str)),
        TySpec::NullableInt => Ty::Nullable(Box::new(Ty::Int)),
        TySpec::NullableFloat => Ty::Nullable(Box::new(Ty::Float)),
        TySpec::NullableUuid => Ty::Nullable(Box::new(Ty::Uuid)),
        TySpec::NullableDatetime => Ty::Nullable(Box::new(Ty::Datetime)),
        TySpec::NullableDynamic => Ty::Nullable(Box::new(Ty::Dynamic)),
        TySpec::ListOfStr => Ty::List(Box::new(Ty::Str)),
        TySpec::ListOfInt => Ty::List(Box::new(Ty::Int)),
        TySpec::ListOfListOfStr => Ty::List(Box::new(Ty::List(Box::new(Ty::Str)))),
        TySpec::ListOfMapStrStr => {
            Ty::List(Box::new(Ty::Map(Box::new(Ty::Str), Box::new(Ty::Str))))
        }
        TySpec::ListOfMapStrDynamic => {
            Ty::List(Box::new(Ty::Map(Box::new(Ty::Str), Box::new(Ty::Dynamic))))
        }
        // Runtime-dynamic: the return type depends on external data (JSON
        // payloads, LLM outputs, etc.) and cannot be determined statically.
        TySpec::Unknown => Ty::Unknown(UnknownReason::ExternalDynamic),
    }
}

// ---------------------------------------------------------------------------
// BuiltinResult — how a method's return type is determined
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
    /// statically.  The checker should produce [`Ty::Unknown`].
    Unknown,
}

// ---------------------------------------------------------------------------
// BuiltinParam — a single parameter in a built-in method signature
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
// BuiltinMethod — one entry in the namespace catalog
// ---------------------------------------------------------------------------

/// Describes a single method exposed by a stdlib namespace.
///
/// All surfaces that enumerate namespace methods (checker, LSP, docs) must
/// derive their lists from [`catalog()`] rather than maintaining independent
/// copies.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinMethod {
    /// The namespace that owns this method (e.g., `"File"`).
    pub namespace: &'static str,
    /// The method name as it appears in Keel source (e.g., `"read"`).
    pub name: &'static str,
    /// Declared parameter list.  Empty `&[]` means zero or variadic
    /// parameters that are not statically validated by the checker yet.
    pub params: &'static [BuiltinParam],
    /// How to compute the return type.
    pub result: BuiltinResult,
    /// One-sentence description, shown in LSP hover and generated docs.
    pub doc: &'static str,
}

// ---------------------------------------------------------------------------
// Public catalog API
// ---------------------------------------------------------------------------

/// Return the complete built-in namespace method catalog.
///
/// The returned slice is the single source of truth for all stdlib namespace
/// methods.  The checker, the LSP completion provider, and the docs generator
/// must all derive their method lists from this function.
///
/// # Examples
///
/// ```ignore
/// assert!(catalog().iter().any(|m| m.namespace == "File" && m.name == "read"));
/// ```
pub fn catalog() -> &'static [BuiltinMethod] {
    CATALOG
}

/// Look up a built-in method by namespace and name.
///
/// Returns `None` if the pair is not registered in the catalog.
pub(crate) fn catalog_method(namespace: &str, name: &str) -> Option<&'static BuiltinMethod> {
    CATALOG
        .iter()
        .find(|m| m.namespace == namespace && m.name == name)
}

// ---------------------------------------------------------------------------
// The catalog — one entry per stdlib method, in namespace order
// ---------------------------------------------------------------------------

static CATALOG: &[BuiltinMethod] = &[
    // ── Ai ──────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Ai",
        name: "classify",
        params: &[],
        result: BuiltinResult::AiClassify,
        doc: "Classify text into an enum variant.",
    },
    BuiltinMethod {
        namespace: "Ai",
        name: "summarize",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Summarize text.",
    },
    BuiltinMethod {
        namespace: "Ai",
        name: "draft",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Draft text from a prompt.",
    },
    BuiltinMethod {
        namespace: "Ai",
        name: "extract",
        params: &[],
        result: BuiltinResult::AiExtract,
        doc: "Extract a typed value from text.",
    },
    BuiltinMethod {
        namespace: "Ai",
        name: "translate",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Translate text to another language.",
    },
    BuiltinMethod {
        namespace: "Ai",
        name: "decide",
        params: &[],
        result: BuiltinResult::AiExtract,
        doc: "Decide by extracting a typed value from context.",
    },
    BuiltinMethod {
        namespace: "Ai",
        name: "prompt",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Send a raw prompt to the LLM and return its response.",
    },
    BuiltinMethod {
        namespace: "Ai",
        name: "embed",
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Embed text into a vector (reserved, not yet stable).",
    },
    // ── Io ──────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Io",
        name: "ask",
        params: &[BuiltinParam {
            name: "prompt",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Ask the user a question and return the response as a string.",
    },
    BuiltinMethod {
        namespace: "Io",
        name: "confirm",
        params: &[BuiltinParam {
            name: "prompt",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Bool),
        doc: "Ask the user to confirm a choice and return true or false.",
    },
    BuiltinMethod {
        namespace: "Io",
        name: "notify",
        params: &[BuiltinParam {
            name: "message",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Show a notification to the user.",
    },
    BuiltinMethod {
        namespace: "Io",
        name: "show",
        params: &[BuiltinParam {
            name: "value",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Display a value to the user.",
    },
    // ── Env ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Env",
        name: "get",
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Get an environment variable, returning none if it is unset.",
    },
    BuiltinMethod {
        namespace: "Env",
        name: "require",
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Get an environment variable, raising an error if it is unset.",
    },
    // ── Time ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Time",
        name: "now",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Datetime),
        doc: "Return the current UTC datetime.",
    },
    BuiltinMethod {
        namespace: "Time",
        name: "epoch_ms",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Int),
        doc: "Return the current Unix timestamp in milliseconds.",
    },
    BuiltinMethod {
        namespace: "Time",
        name: "parse",
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableDatetime),
        doc: "Parse a datetime string, returning none on failure.",
    },
    // ── File ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "File",
        name: "read",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Read a file and return its contents as a string.",
    },
    BuiltinMethod {
        namespace: "File",
        name: "write",
        params: &[
            BuiltinParam {
                name: "path",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "content",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Write a string to a file, creating or overwriting it.",
    },
    BuiltinMethod {
        namespace: "File",
        name: "exists",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Bool),
        doc: "Return true if the path exists on the filesystem.",
    },
    BuiltinMethod {
        namespace: "File",
        name: "list",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfStr),
        doc: "List the entries in a directory.",
    },
    BuiltinMethod {
        namespace: "File",
        name: "mkdir",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Create a directory and all intermediate parents.",
    },
    BuiltinMethod {
        namespace: "File",
        name: "remove",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Remove a file or directory.",
    },
    BuiltinMethod {
        namespace: "File",
        name: "copy",
        params: &[
            BuiltinParam {
                name: "src",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "dst",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Copy a file from src to dst.",
    },
    BuiltinMethod {
        namespace: "File",
        name: "glob",
        params: &[BuiltinParam {
            name: "pattern",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfStr),
        doc: "Return file paths that match a glob pattern.",
    },
    BuiltinMethod {
        namespace: "File",
        name: "move",
        params: &[
            BuiltinParam {
                name: "src",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "dst",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Move (rename) a file from src to dst.",
    },
    BuiltinMethod {
        namespace: "File",
        name: "mktemp",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Create a temporary file and return its path.",
    },
    // ── Random ───────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Random",
        name: "float",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return a random float in the range [0, 1).",
    },
    BuiltinMethod {
        namespace: "Random",
        name: "int",
        params: &[
            BuiltinParam {
                name: "min",
                ty: TySpec::Int,
                optional: false,
            },
            BuiltinParam {
                name: "max",
                ty: TySpec::Int,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Int),
        doc: "Return a random integer in the inclusive range [min, max].",
    },
    BuiltinMethod {
        namespace: "Random",
        name: "bool",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Bool),
        doc: "Return a random boolean.",
    },
    // ── Uuid ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Uuid",
        name: "v4",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Uuid),
        doc: "Generate a random UUID v4.",
    },
    BuiltinMethod {
        namespace: "Uuid",
        name: "v7",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Uuid),
        doc: "Generate a time-sortable UUID v7.",
    },
    BuiltinMethod {
        namespace: "Uuid",
        name: "v5",
        params: &[
            BuiltinParam {
                name: "ns",
                ty: TySpec::Uuid,
                optional: false,
            },
            BuiltinParam {
                name: "name",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Uuid),
        doc: "Generate a deterministic UUID v5 from a namespace UUID and a name.",
    },
    BuiltinMethod {
        namespace: "Uuid",
        name: "parse",
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableUuid),
        doc: "Parse a UUID string, returning none on failure.",
    },
    // ── Crypto ───────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Crypto",
        name: "sha224",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-224 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "sha256",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-256 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "sha384",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-384 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "sha512",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-512 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "sha512_224",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-512/224 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "sha512_256",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-512/256 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "hmac_sha224",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-224 hex digest.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "hmac_sha256",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-256 hex digest.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "hmac_sha384",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-384 hex digest.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "hmac_sha512",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-512 hex digest.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "hmac_sha512_224",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-512/224 hex digest.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "hmac_sha512_256",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-512/256 hex digest.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "token",
        params: &[BuiltinParam {
            name: "len",
            ty: TySpec::Int,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Generate a random URL-safe token of the given byte length.",
    },
    BuiltinMethod {
        namespace: "Crypto",
        name: "random_bytes",
        params: &[BuiltinParam {
            name: "len",
            ty: TySpec::Int,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfInt),
        doc: "Generate cryptographically random bytes as a list of integers.",
    },
    // ── Json ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Json",
        name: "parse",
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        // Return type is externally dynamic — the JSON structure depends on runtime
        // input and is not statically known.  Uses Unknown(ExternalDynamic) so that
        // `keel check --strict` can flag unannotated bindings.  Users who want to
        // silence the warning should annotate explicitly: `let x: dynamic = Json.parse(...)`.
        result: BuiltinResult::Unknown,
        doc: "Parse a JSON string into a dynamic value.",
    },
    BuiltinMethod {
        namespace: "Json",
        name: "stringify",
        params: &[BuiltinParam {
            name: "value",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Serialize a value to a JSON string.",
    },
    // ── Cache ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Cache",
        name: "set",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "value",
                ty: TySpec::Dynamic,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Store a value in the in-process cache.",
    },
    BuiltinMethod {
        namespace: "Cache",
        name: "get",
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableDynamic),
        doc: "Retrieve a cached value, returning none if absent.",
    },
    BuiltinMethod {
        namespace: "Cache",
        name: "delete",
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Remove a key from the cache.",
    },
    BuiltinMethod {
        namespace: "Cache",
        name: "clear",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Clear all entries from the cache.",
    },
    // ── Csv ──────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Csv",
        name: "parse",
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfListOfStr),
        doc: "Parse a CSV string into a list of rows, each row a list of strings.",
    },
    BuiltinMethod {
        namespace: "Csv",
        name: "parse_records",
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfMapStrStr),
        doc: "Parse a CSV string into a list of named-column records.",
    },
    BuiltinMethod {
        namespace: "Csv",
        name: "stringify",
        params: &[BuiltinParam {
            name: "rows",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Serialize a list of rows into a CSV string.",
    },
    // ── Math ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Math",
        name: "PI",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "The mathematical constant π (3.14159…).",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "E",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "The mathematical constant e (2.71828…).",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "sqrt",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the square root of x.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "pow",
        params: &[
            BuiltinParam {
                name: "x",
                ty: TySpec::Float,
                optional: false,
            },
            BuiltinParam {
                name: "y",
                ty: TySpec::Float,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return x raised to the power y.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "exp",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return e raised to the power x.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "log",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the natural logarithm of x.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "log2",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the base-2 logarithm of x.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "log10",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the base-10 logarithm of x.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "sin",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the sine of x (x in radians).",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "cos",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the cosine of x (x in radians).",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "tan",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the tangent of x (x in radians).",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "asin",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the arcsine of x in radians.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "acos",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the arccosine of x in radians.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "atan",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the arctangent of x in radians.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "atan2",
        params: &[
            BuiltinParam {
                name: "y",
                ty: TySpec::Float,
                optional: false,
            },
            BuiltinParam {
                name: "x",
                ty: TySpec::Float,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the four-quadrant arctangent of y and x in radians.",
    },
    // ── Log ──────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Log",
        name: "info",
        params: &[BuiltinParam {
            name: "message",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Emit an info-level log message.",
    },
    BuiltinMethod {
        namespace: "Log",
        name: "warn",
        params: &[BuiltinParam {
            name: "message",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Emit a warning-level log message.",
    },
    BuiltinMethod {
        namespace: "Log",
        name: "error",
        params: &[BuiltinParam {
            name: "message",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Emit an error-level log message.",
    },
    BuiltinMethod {
        namespace: "Log",
        name: "debug",
        params: &[BuiltinParam {
            name: "message",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Emit a debug-level log message.",
    },
    BuiltinMethod {
        namespace: "Log",
        name: "set_level",
        params: &[BuiltinParam {
            name: "level",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Set the minimum log level (\"debug\", \"info\", \"warn\", or \"error\").",
    },
    BuiltinMethod {
        namespace: "Log",
        name: "level",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the current log level as a string.",
    },
    // ── Memory ───────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Memory",
        name: "remember",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "value",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Store a value in agent memory.",
    },
    BuiltinMethod {
        namespace: "Memory",
        name: "recall",
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Retrieve a value from agent memory, returning none if absent.",
    },
    // ── Schedule ─────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Schedule",
        name: "every",
        params: &[BuiltinParam {
            name: "interval",
            ty: TySpec::Duration,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Schedule a task to run on a recurring interval.",
    },
    BuiltinMethod {
        namespace: "Schedule",
        name: "after",
        params: &[BuiltinParam {
            name: "delay",
            ty: TySpec::Duration,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Schedule a task to run after a delay.",
    },
    BuiltinMethod {
        namespace: "Schedule",
        name: "at",
        params: &[BuiltinParam {
            name: "time",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Schedule a task to run at a specific wall-clock time.",
    },
    BuiltinMethod {
        namespace: "Schedule",
        name: "cron",
        params: &[BuiltinParam {
            name: "expr",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Schedule a task using a cron expression.",
    },
    BuiltinMethod {
        namespace: "Schedule",
        name: "sleep",
        params: &[BuiltinParam {
            name: "duration",
            ty: TySpec::Duration,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Pause execution for the given duration.",
    },
    // ── Search ───────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Search",
        name: "web",
        params: &[BuiltinParam {
            name: "query",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Search the web and return a list of SearchResult values.",
    },
    BuiltinMethod {
        namespace: "Search",
        name: "news",
        params: &[BuiltinParam {
            name: "query",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Search for recent news and return a list of SearchResult values.",
    },
    // ── Email ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Email",
        name: "fetch",
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Fetch messages from the configured email inbox.",
    },
    BuiltinMethod {
        namespace: "Email",
        name: "send",
        params: &[
            BuiltinParam {
                name: "to",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "subject",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "body",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Send an email.",
    },
    BuiltinMethod {
        namespace: "Email",
        name: "archive",
        params: &[BuiltinParam {
            name: "id",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Archive an email by its ID.",
    },
    // ── Http ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Http",
        name: "get",
        params: &[BuiltinParam {
            name: "url",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP GET request and return an HttpResponse.",
    },
    BuiltinMethod {
        namespace: "Http",
        name: "post",
        params: &[
            BuiltinParam {
                name: "url",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "body",
                ty: TySpec::Str,
                optional: true,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP POST request and return an HttpResponse.",
    },
    BuiltinMethod {
        namespace: "Http",
        name: "request",
        params: &[
            BuiltinParam {
                name: "method",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "url",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP request with full control and return an HttpResponse.",
    },
    BuiltinMethod {
        namespace: "Http",
        name: "serve",
        params: &[BuiltinParam {
            name: "port",
            ty: TySpec::Int,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Start an HTTP server on the given port.",
    },
    // ── Shell ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Shell",
        name: "run",
        params: &[BuiltinParam {
            name: "cmd",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Run a shell command and return its combined stdout.",
    },
    // ── Db ───────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Db",
        name: "connect",
        params: &[BuiltinParam {
            name: "url",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::DbConnection),
        doc: "Open a database connection and return a DbConnection.",
    },
    // ── Agent ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Agent",
        name: "run",
        params: &[BuiltinParam {
            name: "name",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Start a named agent.",
    },
    BuiltinMethod {
        namespace: "Agent",
        name: "stop",
        params: &[BuiltinParam {
            name: "name",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Stop a running agent.",
    },
    BuiltinMethod {
        namespace: "Agent",
        name: "send",
        params: &[
            BuiltinParam {
                name: "name",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "message",
                ty: TySpec::Dynamic,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Send a message to an agent's mailbox.",
    },
    BuiltinMethod {
        namespace: "Agent",
        name: "delegate",
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Delegate a task to another agent and return its result.",
    },
    BuiltinMethod {
        namespace: "Agent",
        name: "broadcast",
        params: &[BuiltinParam {
            name: "message",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Broadcast a message to all running agents.",
    },
    // ── Async ─────────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Async",
        name: "spawn",
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Spawn a concurrent task and return a handle.",
    },
    BuiltinMethod {
        namespace: "Async",
        name: "join_all",
        params: &[BuiltinParam {
            name: "handles",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Wait for all async task handles to complete.",
    },
    BuiltinMethod {
        namespace: "Async",
        name: "select",
        params: &[BuiltinParam {
            name: "handles",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Return the result of the first completed task handle.",
    },
    // ── Control ───────────────────────────────────────────────────────────────
    BuiltinMethod {
        namespace: "Control",
        name: "retry",
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Retry a task on failure with configurable backoff.",
    },
    BuiltinMethod {
        namespace: "Control",
        name: "with_timeout",
        params: &[BuiltinParam {
            name: "duration",
            ty: TySpec::Duration,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Run a task, raising an error if it exceeds the timeout.",
    },
    BuiltinMethod {
        namespace: "Control",
        name: "with_deadline",
        params: &[BuiltinParam {
            name: "deadline",
            ty: TySpec::Datetime,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Run a task, raising an error if it runs past the deadline.",
    },
];

// ---------------------------------------------------------------------------
// Prelude names
// ---------------------------------------------------------------------------

/// Return the set of identifiers that are always in scope — prelude
/// namespaces, built-in type names, top-level builtins, symbol hint
/// keywords, and built-in interface names.
///
/// The checker pre-seeds its identifier table with this set so that
/// references to `Ai`, `str`, `run`, `json`, `Stringable`, etc. do not
/// produce spurious "undefined identifier" errors.
pub(crate) fn prelude_names() -> HashSet<String> {
    let mut prelude = HashSet::new();

    // Prelude namespaces — derived from the catalog to stay in sync.
    let mut seen = HashSet::new();
    for entry in CATALOG {
        if seen.insert(entry.namespace) {
            prelude.insert(entry.namespace.to_string());
        }
    }

    // Top-level builtins
    for n in ["run", "stop", "min", "max", "uuid", "typeof"] {
        prelude.insert(n.to_string());
    }

    // Built-in type names
    for n in [
        "int",
        "float",
        "str",
        "bool",
        "none",
        "datetime",
        "duration",
        "Uuid",
        "dynamic",
        "list",
        "map",
        "set",
        "Result",
        "Message",
        "SearchResult",
        "Memory",
        "HttpResponse",
        "Decision",
        "Error",
        "AIError",
        "NetworkError",
        "TimeoutError",
        "NullError",
        "TypeError",
        "ParseError",
    ] {
        prelude.insert(n.to_string());
    }

    // Symbol identifiers used as hint args (see runtime::SYMBOL_IDENTS)
    // and attribute-value keywords (`@memory persistent`, etc.).
    for n in [
        "sentence",
        "sentences",
        "line",
        "lines",
        "word",
        "words",
        "paragraph",
        "paragraphs",
        "bullets",
        "prose",
        "json",
        "exponential",
        "linear",
        "fixed",
        "google",
        "bing",
        "arxiv",
        "text",
        "html",
        "markdown",
        "persistent",
        "session",
    ] {
        prelude.insert(n.to_string());
    }

    // Built-in interface names (Stringable, Comparable, …) are not keywords —
    // they're identifiers resolved at runtime.  Adding them to the prelude
    // prevents spurious "undefined identifier" errors when the checker
    // encounters `impl Stringable for Foo` before seeing any declaration of
    // `Stringable` in the source file.
    for iface in [
        "Stringable",
        "Comparable",
        "Equatable",
        "Serializable",
        "Iterable",
    ] {
        prelude.insert(iface.to_string());
    }

    prelude
}

// ---------------------------------------------------------------------------
// Built-in interface definitions
// ---------------------------------------------------------------------------

/// Return the built-in interface catalog.
///
/// Each entry maps an interface name to the list of method signatures that
/// implementing types must satisfy.  These are used by `check_impl_conformance`
/// to validate `impl X for Y` blocks.
pub(crate) fn builtin_interfaces() -> HashMap<String, Vec<TaskSig>> {
    let mut map = HashMap::new();

    // Synthetic params have no source position; use the 0..0 sentinel span.
    let self_param = || Param {
        name: Binding::Ident("self".to_string()),
        name_span: 0..0,
        ty: Node::synthetic(TypeExpr::Named("__impl_self__".to_string())),
        default: None,
        variadic: false,
    };
    let dynamic_param = |name: &str| Param {
        name: Binding::Ident(name.to_string()),
        name_span: 0..0,
        ty: Node::synthetic(TypeExpr::Dynamic),
        default: None,
        variadic: false,
    };

    map.insert(
        "Stringable".to_string(),
        vec![TaskSig {
            name: "to_str".to_string(),
            name_span: 0..0,
            params: vec![self_param()],
            return_type: Some(Node::synthetic(TypeExpr::Named("str".to_string()))),
        }],
    );
    map.insert(
        "Serializable".to_string(),
        vec![TaskSig {
            name: "to_json".to_string(),
            name_span: 0..0,
            params: vec![self_param()],
            return_type: Some(Node::synthetic(TypeExpr::Named("str".to_string()))),
        }],
    );
    map.insert(
        "Comparable".to_string(),
        vec![TaskSig {
            name: "compare".to_string(),
            name_span: 0..0,
            params: vec![self_param(), dynamic_param("other")],
            return_type: Some(Node::synthetic(TypeExpr::Named("int".to_string()))),
        }],
    );
    map.insert(
        "Equatable".to_string(),
        vec![TaskSig {
            name: "equals".to_string(),
            name_span: 0..0,
            params: vec![self_param(), dynamic_param("other")],
            return_type: Some(Node::synthetic(TypeExpr::Named("bool".to_string()))),
        }],
    );
    map.insert(
        "Iterable".to_string(),
        vec![TaskSig {
            name: "items".to_string(),
            name_span: 0..0,
            params: vec![self_param()],
            return_type: Some(Node::synthetic(TypeExpr::List(Box::new(TypeExpr::Dynamic)))),
        }],
    );

    map
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_no_duplicate_entries() {
        let mut seen = std::collections::HashSet::new();
        for entry in CATALOG {
            let key = format!("{}.{}", entry.namespace, entry.name);
            assert!(seen.insert(key.clone()), "duplicate catalog entry: {key}");
        }
    }

    #[test]
    fn all_catalog_namespaces_are_in_prelude_names() {
        let names = prelude_names();
        for entry in CATALOG {
            assert!(
                names.contains(entry.namespace),
                "catalog namespace `{}` missing from prelude_names()",
                entry.namespace
            );
        }
    }

    /// Every `Namespace.method` in the catalog must appear as the string
    /// `"Namespace.method"` in at least one file under `docs/src/`.
    ///
    /// If this test fails after you add a catalog entry, add a mention of the
    /// new method to the appropriate guide page in `docs/src/guide/`.
    #[test]
    fn catalog_methods_are_mentioned_in_docs() {
        use std::path::Path;

        // Collect all docs text once.
        let docs_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/src");
        let mut docs = String::new();
        fn collect(dir: &Path, out: &mut String) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        collect(&path, out);
                    } else if path.extension().and_then(|e| e.to_str()) == Some("md")
                        && let Ok(text) = std::fs::read_to_string(&path)
                    {
                        out.push_str(&text);
                    }
                }
            }
        }
        collect(&docs_root, &mut docs);

        let mut missing: Vec<String> = Vec::new();
        for entry in CATALOG {
            let key = format!("{}.{}", entry.namespace, entry.name);
            if !docs.contains(&key) {
                missing.push(key);
            }
        }

        assert!(
            missing.is_empty(),
            "The following catalog methods are not mentioned in any docs/src/**/*.md file.\n\
             Add them to the appropriate guide page before merging:\n  {}",
            missing.join("\n  ")
        );
    }

    #[test]
    fn catalog_covers_all_runtime_namespaces() {
        // Every namespace registered by the runtime must appear in the catalog.
        let runtime_namespaces = [
            "Io", "Schedule", "Ai", "Email", "Env", "Memory", "Log", "Agent", "Control", "Async",
            "Http", "Search", "Db", "Time", "File", "Json", "Cache", "Random", "Uuid", "Crypto",
            "Math", "Shell", "Csv",
        ];
        let catalog_namespaces: std::collections::HashSet<&str> =
            CATALOG.iter().map(|m| m.namespace).collect();
        for ns in runtime_namespaces {
            assert!(
                catalog_namespaces.contains(ns),
                "runtime namespace `{ns}` is not represented in the catalog"
            );
        }
    }

    #[test]
    fn ty_from_spec_round_trips_for_all_variants() {
        // Smoke-test that every TySpec variant produces a non-panicking Ty.
        let specs = [
            TySpec::Int,
            TySpec::Float,
            TySpec::Str,
            TySpec::Bool,
            TySpec::None_,
            TySpec::Datetime,
            TySpec::Duration,
            TySpec::Uuid,
            TySpec::Dynamic,
            TySpec::DbConnection,
            TySpec::NullableStr,
            TySpec::NullableInt,
            TySpec::NullableFloat,
            TySpec::NullableUuid,
            TySpec::NullableDatetime,
            TySpec::NullableDynamic,
            TySpec::ListOfStr,
            TySpec::ListOfInt,
            TySpec::ListOfListOfStr,
            TySpec::ListOfMapStrStr,
            TySpec::ListOfMapStrDynamic,
            TySpec::Unknown,
        ];
        for spec in specs {
            let _ = ty_from_spec(spec); // must not panic
        }
    }
}
