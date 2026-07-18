//! `ai` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "ai",
        name: "classify",
        method_id: 0,
        params: &[],
        result: BuiltinResult::AiClassify,
        doc: "Classify text into an enum variant.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "summarize",
        method_id: 1,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Summarize text.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "draft",
        method_id: 2,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Draft text from a prompt.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "extract",
        method_id: 3,
        params: &[],
        result: BuiltinResult::AiExtract,
        doc: "Extract a typed value from text.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "translate",
        method_id: 4,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Translate text to another language.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "decide",
        method_id: 5,
        params: &[],
        result: BuiltinResult::AiExtract,
        doc: "Decide by extracting a typed value from context.",
    },
    BuiltinMethod {
        namespace: "ai",
        name: "prompt",
        method_id: 6,
        params: &[],
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
        params: &[],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Register a user-authored `LlmProvider` as the program-wide backend.",
    },
];
