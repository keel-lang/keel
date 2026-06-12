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
    A->>B: send(B, data, event: "greeting")
    note over A: returns immediately — continues execution

    note over B: on greeting(msg: str) { ... }
    B->>B: handler runs with msg = data
```

## Event Routing

The `event:` parameter in `Agent.send` determines which `on` handler runs on the receiver.

```mermaid
flowchart LR
    s1["send(B, data, event: &quot;greeting&quot;)"]
    s2["send(B, data, event: &quot;payment&quot;)"]
    s3["send(B, data)"]

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

`Agent.delegate` posts a **named handler event** to another agent's mailbox.

### Symbol form (preferred)

```keel
delegate(Processor.handle, payload)
# Processor's `on handle` fires with payload
```

`Processor.handle` is a compile-time–resolved handler reference. The type checker
validates that `handle` is a declared `on` handler on `Processor` and that `payload`
matches the handler's parameter type. Misspelled handler names and wrong argument
types are caught at compile time, not at runtime.

### String form (legacy)

```keel
delegate(Processor, "handle", payload)
# Processor's `on handle` fires with payload
```

The handler name is a string literal. The type checker validates it when the
literal is plain (no interpolation). Both forms are accepted; prefer the symbol
form for new code — handler renames update the symbol reference automatically.

### Agent.send

`send(target, data, event: "...")` posts a **data event** with explicit routing:

```keel
send(Processor, payload, event: "process")
# Processor's `on process` fires with payload
```

Both are non-blocking. Choose `delegate` when you want compile-time handler
validation; use `send` when you need the `event:` label to be determined
dynamically at runtime.

Direct cross-agent calls such as `Worker.process(...)` are not part of the
agent model. Inside an agent, call agent-owned helpers as `self.task(...)`.
Across agents, use mailbox APIs so delivery remains explicit and asynchronous.

## Agent.send vs ai.*

These are two completely separate communication paths.

```mermaid
flowchart LR
    code["Agent code"]
    code -->|"send(B, data)"| b["Agent B\n→ on &lt;event&gt; handler"]
    code -->|"ai.classify / ai.prompt / ..."| llm["LLM\n→ returns a value"]
```

`Agent.send` is agent-to-agent messaging — no LLM involved.
`ai.*` calls send a prompt to the LLM and return its response.

## Example: Bi-directional Communication

```keel
use std/ai
use std/io

agent Manager {
    @tools [io]
    state { done: int = 0 }

    @on_start {
        send(Worker, {id: 1}, event: "process")
    }

    on result(summary: str) {
        io.show("Result received: {summary}")
        self.done = self.done + 1
        stop(Manager)
    }
}

agent Worker {
    @tools [ai]
    on process(task: dynamic) {
        output = ai.summarize(task.id, in: 1, unit: sentences)
        send(Manager, output, event: "result")
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
    W->>LLM: ai.summarize(...)
    LLM-->>W: summary text
    W->>M: send(event: "result", data: summary)
    note over M: on result — prints summary, stops
```

## Broadcasting to a team

`broadcast(team, data, event: "...")` fans out a single event to
every live agent whose `@team [...]` attribute contains the target team
name. Agents on other teams stay silent.

```keel
use std/io

agent Alpha {
    @tools [io]
    @team ["frontline"]
    on alert(msg: str) { io.show("Alpha got {msg}") }
}

agent Beta {
    @tools [io]
    @team ["frontline"]
    on alert(msg: str) { io.show("Beta got {msg}") }
}

agent Gamma {
    @tools [io]
    @team ["backoffice"]
    on alert(msg: str) { io.show("Gamma got {msg}") }
}

agent Coordinator {
    @on_start {
        run(Alpha); run(Beta); run(Gamma)
        broadcast("frontline", "production-down", event: "alert")
        # Alpha and Beta fire; Gamma does not.
    }
}
```

`@team` accepts a list, so an agent can belong to multiple teams. The
broadcast is non-blocking — every recipient handles the event on its
own mailbox in its own time.

## Backpressure: RuntimeBusy

The interpreter event queue is bounded (default 1024; tunable via `KEEL_EVENT_QUEUE_CAPACITY`).
If the queue is full when `Agent.send`, `Agent.delegate`, or `Agent.broadcast` is called,
the call raises a `RuntimeBusy` error instead of blocking.

Catch it to apply your own backpressure strategy:

```keel
try {
    send(Worker, payload)
} catch e: RuntimeBusy {
    io.show("queue full — payload dropped")
}
```

A full queue typically means the event loop is processing a slow handler (e.g. an LLM call)
while producers are sending faster than the loop can drain. Strategies:
- Catch and drop (log the miss)
- Catch and retry after a delay (`schedule.after(100.ms, ...)`)
- Reduce the send rate on the producer side

> `KEEL_EVENT_QUEUE_CAPACITY=<n>` overrides the 1024 default. Use a small value (e.g. 2)
> in integration tests to deliberately trigger `RuntimeBusy` without flooding the queue.

## Key Properties

| Property | Behaviour |
|----------|-----------|
| **Routing** | `event:` string in `Agent.send` matches the name in `on <event>` |
| **Default event** | Omitting `event:` routes to `on message` |
| **No match** | Unhandled events are silently dropped |
| **Send** | Non-blocking — sender continues immediately |
| **Queue full** | `RuntimeBusy` error raised — catch to handle backpressure |
| **Execution** | Handlers run one at a time — no race conditions on `self.` |
| **Scope** | In-process only — no network, no serialization |

## See Also

- [Agents & Attributes](./agents.md)
- [Scheduling](./scheduling.md)
