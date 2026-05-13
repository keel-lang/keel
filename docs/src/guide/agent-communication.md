# Agent Communication

Keel agents communicate by sending **events** to each other. The sender
posts a named event and returns immediately. The receiver handles it when
the runtime delivers it.

## How It Works

```mermaid
sequenceDiagram
    participant A as Agent A
    participant B as Agent B

    note over A: @on_start { ... }
    A->>B: Agent.send(B, data, event: "greeting")
    note over A: returns immediately — continues execution

    note over B: on greeting(msg: str) { ... }
    B->>B: handler runs with msg = data
```

## Event Routing

The `event:` parameter in `Agent.send` determines which `on` handler runs on the receiver.

```mermaid
flowchart LR
    s1["Agent.send(B, data, event: &quot;greeting&quot;)"]
    s2["Agent.send(B, data, event: &quot;payment&quot;)"]
    s3["Agent.send(B, data)"]

    h1["on greeting(msg) { ... }"]
    h2["on payment(msg) { ... }"]
    h3["on message(msg) { ... }"]
    drop["silently dropped"]

    s1 -->|"event matches"| h1
    s2 -->|"event matches"| h2
    s3 -->|"default event: message"| h3
    s1 -->|"no matching handler"| drop
```

The default event name is `"message"` — omitting `event:` routes to `on message`.

## Asynchronous Delivery

Send and receive are **decoupled**. Agent A does not wait for Agent B's handler to finish.

```mermaid
sequenceDiagram
    participant A as Agent A
    participant Q as Queue
    participant B as Agent B

    A->>Q: post event (non-blocking)
    A->>A: continues own work

    Q->>B: deliver event
    B->>B: on greeting runs
```

## Agent.delegate vs Agent.send

`Agent.delegate(target, task, args)` posts a **named task event** to another agent:

```keel
Agent.delegate(Processor, "handle", payload)
# Processor's `on handle` fires with payload
```

`Agent.send(target, data, event: "...")` posts a **data event** with explicit routing:

```keel
Agent.send(Processor, payload, event: "process")
# Processor's `on process` fires with payload
```

Both are non-blocking. Choose `delegate` when the receiver's task name is the event name; use `send` when you want more explicit control over the `event:` label.

Direct cross-agent calls such as `Worker.process(...)` are not part of the
agent model. Inside an agent, call agent-owned helpers as `self.task(...)`.
Across agents, use mailbox APIs so delivery remains explicit and asynchronous.

## Agent.send vs Ai.*

These are two completely separate communication paths.

```mermaid
flowchart LR
    code["Agent code"]
    code -->|"Agent.send(B, data)"| b["Agent B\n→ on &lt;event&gt; handler"]
    code -->|"Ai.classify / Ai.prompt / ..."| llm["LLM\n→ returns a value"]
```

`Agent.send` is agent-to-agent messaging — no LLM involved.
`Ai.*` calls send a prompt to the LLM and return its response.

## Example: Bi-directional Communication

```keel
agent Manager {
    state { done: int = 0 }

    @on_start {
        Agent.send(Worker, {id: 1}, event: "process")
    }

    on result(summary: str) {
        Io.show("Result received: {summary}")
        self.done = self.done + 1
        stop(Manager)
    }
}

agent Worker {
    on process(task: dynamic) {
        output = Ai.summarize(task.id, in: 1, unit: sentences)
        Agent.send(Manager, output, event: "result")
    }
}

run(Manager)
run(Worker)
```

```mermaid
sequenceDiagram
    participant M as Manager
    participant W as Worker
    participant LLM as LLM (Ollama)

    M->>W: send(event: "process", data: {id: 1})
    W->>LLM: Ai.summarize(...)
    LLM-->>W: summary text
    W->>M: send(event: "result", data: summary)
    note over M: on result — prints summary, stops
```

## Broadcasting to a team

`Agent.broadcast(team, data, event: "...")` fans out a single event to
every live agent whose `@team [...]` attribute contains the target team
name. Agents on other teams stay silent.

```keel
agent Alpha {
    @team ["frontline"]
    on alert(msg: str) { Io.show("Alpha got {msg}") }
}

agent Beta {
    @team ["frontline"]
    on alert(msg: str) { Io.show("Beta got {msg}") }
}

agent Gamma {
    @team ["backoffice"]
    on alert(msg: str) { Io.show("Gamma got {msg}") }
}

agent Coordinator {
    @on_start {
        Agent.run(Alpha); Agent.run(Beta); Agent.run(Gamma)
        Agent.broadcast("frontline", "production-down", event: "alert")
        # Alpha and Beta fire; Gamma does not.
    }
}
```

`@team` accepts a list, so an agent can belong to multiple teams. The
broadcast is non-blocking — every recipient handles the event on its
own mailbox in its own time.

## Key Properties

| Property | Behaviour |
|----------|-----------|
| **Routing** | `event:` string in `Agent.send` matches the name in `on <event>` |
| **Default event** | Omitting `event:` routes to `on message` |
| **No match** | Unhandled events are silently dropped |
| **Send** | Non-blocking — sender continues immediately |
| **Execution** | Handlers run one at a time — no race conditions on `self.` |
| **Scope** | In-process only — no network, no serialization |

## See Also

- [Agents & Attributes](./agents.md)
- [Scheduling](./scheduling.md)
