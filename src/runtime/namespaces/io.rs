use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::human;
use crate::runtime::namespace::{ns, positional};

pub(crate) fn namespace() -> Namespace {
    ns!("Io", {
        "notify" => |_i, args| Box::pin(async move {
            let msg = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            human::notify(&msg);
            Ok(Value::None)
        }),
        "show" => |interp, args| Box::pin(async move {
            let v = positional(&args, 0).cloned().unwrap_or(Value::None);
            human::show_with_repl(&v, interp.runtime.env.var("KEEL_REPL").as_deref() == Some("1"));
            Ok(Value::None)
        }),
        "ask" => |_i, args| Box::pin(async move {
            let prompt = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            // Off-thread the blocking stdin read so the tokio runtime
            // can keep polling other tasks (signal watcher, scheduler).
            let answer = tokio::task::spawn_blocking(move || human::ask(&prompt))
                .await
                .map_err(|e| miette::miette!("Io.ask task join error: {e}"))?
                .map_err(|e| miette::miette!("Io.ask failed: {e}"))?;
            Ok(Value::String(answer))
        }),
        "confirm" => |_i, args| Box::pin(async move {
            let prompt = positional(&args, 0).map(|v| v.to_display_string()).unwrap_or_default();
            let answer = tokio::task::spawn_blocking(move || human::confirm(&prompt))
                .await
                .map_err(|e| miette::miette!("Io.confirm task join error: {e}"))?
                .map_err(|e| miette::miette!("Io.confirm failed: {e}"))?;
            Ok(Value::Bool(answer))
        }),
    })
}
