use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::namespace::{find_arg, ns, positional};

pub(crate) fn namespace() -> Namespace {
    ns!("Agent", {
        "run" => |interp, args| Box::pin(async move {
            let agent_name = match args.first().map(|a| &a.value) {
                Some(Value::AgentRef(name)) => name.clone(),
                _ => return Err(miette::miette!("Agent.run expects an agent argument")),
            };
            interp.start_agent(&agent_name).await?;
            Ok(Value::None)
        }),
        "stop" => |interp, args| Box::pin(async move {
            let agent_name = match args.first().map(|a| &a.value) {
                Some(Value::AgentRef(name)) => name.clone(),
                _ => return Err(miette::miette!("Agent.stop expects an agent argument")),
            };
            interp.stop_agent(&agent_name).await?;
            Ok(Value::None)
        }),
        // Agent.send(target, message) — posts `message` to the target
        // agent's `on message` handler via the event loop. Returns
        // immediately; the handler runs later in the target's context.
        "send" => |interp, args| Box::pin(async move {
            let target = match args.first().map(|a| &a.value) {
                Some(Value::AgentRef(name)) => name.clone(),
                _ => return Err(miette::miette!("Agent.send: first arg must be an agent")),
            };
            let data = args.iter().skip(1)
                .find(|a| a.name.is_none())
                .map(|a| a.value.clone())
                .unwrap_or(Value::None);
            let event_name = find_arg(&args, "event").map(|v| v.to_display_string()).unwrap_or_else(|| "message".to_string());
            interp.enqueue_event(crate::interpreter::Event::Dispatch {
                agent_name: target,
                event: event_name,
                data,
            })?;
            Ok(Value::None)
        }),
        // Agent.delegate(target, task, args) — posts a named task event to
        // target's mailbox. Unlike Agent.send, the task name is a positional
        // arg rather than a named `event:` parameter.
        "delegate" => |interp, args| Box::pin(async move {
            let target = match args.first().map(|a| &a.value) {
                Some(Value::AgentRef(name)) => name.clone(),
                _ => return Err(miette::miette!("Agent.delegate: first arg must be an agent")),
            };
            let task_name = args.get(1)
                .map(|a| a.value.to_display_string())
                .unwrap_or_else(|| "message".to_string());
            let data = args.get(2)
                .map(|a| a.value.clone())
                .unwrap_or(Value::None);
            interp.enqueue_event(crate::interpreter::Event::Dispatch {
                agent_name: target,
                event: task_name,
                data,
            })?;
            Ok(Value::None)
        }),
        // Agent.broadcast(team, data) — fan-out a `message` event to every
        // running agent whose `@team [...]` declaration includes the given
        // team name. Useful for system-wide signals to a labeled group.
        "broadcast" => |interp, args| Box::pin(async move {
            let team = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Agent.broadcast: missing team name"))?;
            let data = positional(&args, 1).cloned().unwrap_or(Value::None);
            let event_name = find_arg(&args, "event")
                .map(|v| v.to_display_string())
                .unwrap_or_else(|| "message".to_string());

            let recipients = agents_in_team(interp, &team);
            for agent_name in recipients {
                interp.enqueue_event(crate::interpreter::Event::Dispatch {
                    agent_name,
                    event: event_name.clone(),
                    data: data.clone(),
                })?;
            }
            Ok(Value::None)
        }),
    })
}

/// Return the names of every running agent whose `@team [...]` declaration
/// contains `team`. Strings inside the list are matched literally.
fn agents_in_team(interp: &crate::interpreter::Interpreter, team: &str) -> Vec<String> {
    use crate::ast::{AttributeBody, Expr, StringPart};

    let live = interp.live_agents.lock();
    let mut out = Vec::new();
    for (name, instance) in live.iter() {
        let def = instance.lock().def.clone();
        let in_team = def.attributes.iter().any(|attr| {
            if attr.name != "team" {
                return false;
            }
            let AttributeBody::Expr(Expr::ListLit(items)) = &attr.body else {
                return false;
            };
            items.iter().any(|e| match e {
                Expr::StringLit(parts) => {
                    let s: String = parts
                        .iter()
                        .filter_map(|p| match p {
                            StringPart::Literal(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect();
                    s == team
                }
                Expr::Ident(s) => s == team,
                _ => false,
            })
        });
        if in_team {
            out.push(name.clone());
        }
    }
    out
}
