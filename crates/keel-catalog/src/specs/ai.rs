//! `ai` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "ai",
        name: "classify",
        params: &[],
        result: BuiltinResult::AiClassify,
        doc: "Classify text into an enum variant.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "summarize",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Summarize text.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "draft",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Draft text from a prompt.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "extract",
        params: &[],
        result: BuiltinResult::AiExtract,
        doc: "Extract a typed value from text.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "translate",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Translate text to another language.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "decide",
        params: &[],
        result: BuiltinResult::AiExtract,
        doc: "Decide by extracting a typed value from context.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "prompt",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Send a raw prompt to the LLM and return its response.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "embed",
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Embed text into a vector (reserved, not yet stable).",
    },
];
