//! `ai` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "ai",
        name: "classify",
        method_id: 0,
        params: &[
            BuiltinParam {
                name: "input",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "as",
                ty: TySpec::Dynamic,
                optional: false,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "considering",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "using",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::AiClassify,
        doc: "Classify text into an enum variant.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "summarize",
        method_id: 1,
        params: &[
            BuiltinParam {
                name: "input",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "unit",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "in",
                ty: TySpec::Int,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "format",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "max",
                ty: TySpec::Int,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "using",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Summarize text.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "draft",
        method_id: 2,
        params: &[
            BuiltinParam {
                name: "description",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "tone",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "guidance",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "max_length",
                ty: TySpec::Int,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "context",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "format",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "using",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Draft text from a prompt.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "extract",
        method_id: 3,
        params: &[
            BuiltinParam {
                name: "from",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::Either,
            },
            BuiltinParam {
                name: "schema",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "as",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "using",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::AiExtract,
        doc: "Extract a typed value from text.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "translate",
        method_id: 4,
        params: &[
            BuiltinParam {
                name: "input",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "to",
                ty: TySpec::Dynamic,
                optional: false,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "using",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Translate text to another language.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "decide",
        method_id: 5,
        params: &[
            BuiltinParam {
                name: "input",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "options",
                ty: TySpec::ListOfStr,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            // Not read by the runtime (`ai.decide` always returns a fixed
            // `{choice, reason, confidence}` map) — declared here purely so
            // the checker's `BuiltinResult::AiExtract` handling can resolve
            // it to the call's inferred result type, same as `ai.extract`'s
            // `as`. A pre-existing checker/runtime behavior gap, not
            // introduced by this arg-validation pass — tracked as #244.
            BuiltinParam {
                name: "as",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "using",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::AiExtract,
        doc: "Decide by extracting a typed value from context.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "prompt",
        method_id: 6,
        params: &[
            BuiltinParam {
                name: "system",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "user",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "response_format",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "using",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Send a raw prompt to the LLM and return its response.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "embed",
        method_id: 7,
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Embed text into a vector (reserved, not yet stable).",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "install",
        method_id: 8,
        params: &[BuiltinParam {
            name: "provider",
            ty: TySpec::Dynamic,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Register a user-authored `LlmProvider` as the program-wide backend.",
    },
];
