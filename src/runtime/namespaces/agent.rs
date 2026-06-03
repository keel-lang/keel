use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::value::Value;
use crate::interpreter::{Host, Namespace};
use crate::runtime::args::{expect_str, expect_str_named, expect_str_value};
use crate::runtime::namespace::{ns, positional};

pub(crate) const SPEC: &[BuiltinMethod] = &[
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
];

pub(crate) fn namespace() -> Namespace {
    ns!("Agent", {
        "run" => |host, args| Box::pin(async move {
            let agent_name = match args.first().map(|a| &a.value) {
                Some(Value::AgentRef(name)) => name.clone(),
                _ => return Err(miette::miette!("Agent.run expects an agent argument")),
            };
            host.start_agent(&agent_name).await?;
            Ok(Value::None)
        }),
        "stop" => |host, args| Box::pin(async move {
            let agent_name = match args.first().map(|a| &a.value) {
                Some(Value::AgentRef(name)) => name.clone(),
                _ => return Err(miette::miette!("Agent.stop expects an agent argument")),
            };
            host.stop_agent(&agent_name).await?;
            Ok(Value::None)
        }),
        // Agent.send(target, message) — posts `message` to the target
        // agent's `on message` handler via the event loop. Returns
        // immediately; the handler runs later in the target's context.
        "send" => |host, args| Box::pin(async move {
            let target = match args.first().map(|a| &a.value) {
                Some(Value::AgentRef(name)) => name.clone(),
                _ => return Err(miette::miette!("Agent.send: first arg must be an agent")),
            };
            let data = args.iter().skip(1)
                .find(|a| a.name.is_none())
                .map(|a| a.value.clone())
                .unwrap_or(Value::None);
            let event_name = expect_str_named(&args, "event", "Agent.send")?
                .unwrap_or("message")
                .to_owned();
            host.enqueue_event(crate::interpreter::Event::Dispatch {
                agent_name: target,
                event: event_name,
                data,
            })?;
            Ok(Value::None)
        }),
        // Agent.delegate — posts a named handler event to a target agent's mailbox.
        //
        // Symbol form (preferred): Agent.delegate(Foo.handle, data)
        //   arg[0] = AgentHandlerRef(agent_name, handler_name)
        //   arg[1] = data payload
        //
        // String form (legacy): Agent.delegate(Foo, "handle", data)
        //   arg[0] = AgentRef(agent_name)
        //   arg[1] = handler name as string
        //   arg[2] = data payload
        "delegate" => |host, args| Box::pin(async move {
            let (target, event, data) = match args.first().map(|a| &a.value) {
                Some(Value::AgentHandlerRef(agent_name, handler_name)) => {
                    let data = args.get(1)
                        .map(|a| a.value.clone())
                        .unwrap_or(Value::None);
                    (agent_name.clone(), handler_name.clone(), data)
                }
                Some(Value::AgentRef(name)) => {
                    let handler = args.get(1)
                        .map(|a| expect_str_value(&a.value, "handler name", "Agent.delegate"))
                        .transpose()?
                        .unwrap_or("message")
                        .to_owned();
                    let data = args.get(2)
                        .map(|a| a.value.clone())
                        .unwrap_or(Value::None);
                    (name.clone(), handler, data)
                }
                _ => return Err(miette::miette!(
                    "Agent.delegate: first argument must be an agent handler \
                     (use `Agent.delegate(Foo.handle, data)`) or an agent \
                     (use `Agent.delegate(Foo, \"handle\", data)`)"
                )),
            };
            host.enqueue_event(crate::interpreter::Event::Dispatch {
                agent_name: target,
                event,
                data,
            })?;
            Ok(Value::None)
        }),
        // Agent.broadcast(team, data) — fan-out a `message` event to every
        // running agent whose `@team [...]` declaration includes the given
        // team name. Useful for system-wide signals to a labeled group.
        "broadcast" => |host, args| Box::pin(async move {
            let team = expect_str(&args, 0, "Agent.broadcast")?;
            let data = positional(&args, 1).cloned().unwrap_or(Value::None);
            let event_name = expect_str_named(&args, "event", "Agent.broadcast")?
                .unwrap_or("message")
                .to_owned();

            let recipients = agents_in_team(host, team);
            for agent_name in recipients {
                host.enqueue_event(crate::interpreter::Event::Dispatch {
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
fn agents_in_team(host: &dyn Host, team: &str) -> Vec<String> {
    use crate::ast::{AttributeBody, Expr, StringPart};

    let instances: Vec<_> = host
        .live_agents()
        .lock()
        .iter()
        .map(|(name, inst)| (name.clone(), inst.clone()))
        .collect();
    let mut out = Vec::new();
    for (name, instance) in &instances {
        let def = instance.lock().def.clone();
        let in_team = def.attributes.iter().any(|attr| {
            if attr.name != "team" {
                return false;
            }
            let AttributeBody::Expr(list_node) = &attr.body else {
                return false;
            };
            let Expr::ListLit(items) = &list_node.kind else {
                return false;
            };
            items.iter().any(|e| match &e.kind {
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
