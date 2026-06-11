use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::human;
use crate::runtime::namespace::{ns, positional};

pub(crate) const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "io",
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
        namespace: "io",
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
        namespace: "io",
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
        namespace: "io",
        name: "show",
        params: &[BuiltinParam {
            name: "value",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Display a value to the user.",
    },
];

pub(crate) fn namespace() -> Namespace {
    ns!("io", {
        "notify" => |_host, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            human::notify(&msg);
            Ok(Value::None)
        }),
        "show" => |host, args| Box::pin(async move {
            let v = positional(&args, 0).cloned().unwrap_or(Value::None);
            human::show_with_repl(&v, host.runtime().env.var("KEEL_REPL").as_deref() == Some("1"));
            Ok(Value::None)
        }),
        "ask" => |_i, args| Box::pin(async move {
            let prompt = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            // Off-thread the blocking stdin read so the tokio runtime
            // can keep polling other tasks (signal watcher, scheduler).
            let answer = tokio::task::spawn_blocking(move || human::ask(&prompt))
                .await
                .map_err(|e| miette::miette!("io.ask task join error: {e}"))?
                .map_err(|e| miette::miette!("io.ask failed: {e}"))?;
            Ok(Value::String(answer))
        }),
        "confirm" => |_i, args| Box::pin(async move {
            let prompt = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            let answer = tokio::task::spawn_blocking(move || human::confirm(&prompt))
                .await
                .map_err(|e| miette::miette!("io.confirm task join error: {e}"))?
                .map_err(|e| miette::miette!("io.confirm failed: {e}"))?;
            Ok(Value::Bool(answer))
        }),
    })
}
