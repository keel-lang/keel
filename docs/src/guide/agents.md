# Agents & Attributes

> **Alpha (v0.1).** Breaking changes expected.

An agent is the one concurrency primitive Keel provides — a serial-handler mailbox with isolated mutable state accessible only via `self`. Everything else a program does (AI calls, scheduling, I/O, HTTP) is a library function; the agent is the only truly language-level construct.

## Minimal agent

```keel
agent Greeter {
  @role "You greet people warmly"
}

run(Greeter)
```

## Full anatomy

```keel
agent EmailBot {
  # --- Attributes ---
  @role "Professional email triage"
  @tools [Email, Calendar]
  @memory persistent
  @rules [
    "Never reveal internal pricing",
    "Always disclaim medical advice"
  ]
  @limits {
    max_cost_per_request: 0.50
    max_tokens_per_request: 4096
    timeout: 30.seconds
    require_confirmation: [Email.send, Db.exec]
  }

  # --- Mutable state (only via self.) ---
  state {
    processed: int = 0
    last_run: datetime? = none
  }

  # --- Agent tasks (can access self) ---
  task greet(name: str) -> str {
    Ai.draft("greeting for {name}", tone: "warm") ?? "Hello!"
  }

  # --- Event handlers ---
  on message(msg: Message) {
    response = greet(msg.from)
    Email.send(response, to: msg)
    self.processed = self.processed + 1
  }

  # --- Lifecycle hooks ---
  @on_start {
    Schedule.every(1.day, at: @9am, () => {
      Io.notify("Processed {self.processed} messages yesterday")
    })
  }
}

run(EmailBot)
```

## Attributes

Attributes are identifier-prefixed metadata clauses. They declare agent identity, capabilities, and lifecycle behavior without needing dedicated keywords. Only **two** attributes are built into the core language:

| Attribute | Core? | Status | Purpose |
|---|---|---|---|
| `@role` | Yes | ✅ | The agent's identity string. In v0.1 it's prepended as `"You are {role}.\n\n..."` to every `Ai.*` system prompt, so the LLM sees the agent's directive on every call |
| `@model` | Yes | ✅ | The model name string; overrides the global default for this agent |

Everything else — `@tools`, `@memory`, `@rules`, `@limits`, `@on_start`, `@on_stop`, and user-defined attributes — is **stdlib-registered**. Adding a new attribute requires a library, not a language change.

> Of the stdlib attributes below, only `@on_start` is wired in v0.1. The rest (`@on_stop`, `@tools`, `@memory`, `@rules`, `@limits`, `@team`, `@provider`) are parsed but have no runtime effect yet — <span class="badge badge-soon">Coming soon</span>. Individual sections note the status explicitly.

### `@tools` — capability list <span class="badge badge-soon">Coming soon</span>

```keel
@tools [Email, Calendar, Http]
```

Binds stdlib modules as the agent's declared capabilities. The runtime uses this list to:
- Allow/deny which stdlib namespaces the agent can use
- Report the agent's capabilities to the LLM (for tool-use style prompting)

> **Status:** parsed in v0.1, no capability gating enforced yet.

### `@memory` — persistent semantic memory <span class="badge badge-soon">Coming soon</span>

```keel
@memory persistent    # | session | none
```

- `persistent` — survives restarts (default SQLite backend)
- `session` — lives for the life of the runtime
- `none` — disables `Memory.*` operations for this agent

Swap the backend by installing a different `VectorStore` implementation.

> **Status:** `@memory` is parsed but ignored in v0.1, and `Memory.remember/recall/forget` are no-op stubs. A real vector-store backend is tracked in [ROADMAP](../../ROADMAP.md).

### `@rules` — natural-language guardrails <span class="badge badge-soon">Coming soon</span>

```keel
@rules [
  "Never reveal internal pricing logic",
  "Escalate if the user expresses frustration 3+ times"
]
```

Rules are injected into every LLM prompt this agent makes. They are **LLM-interpreted** — compliance is best-effort. For deterministic constraints, use `@limits`.

> **Status:** parsed in v0.1, not forwarded to the LLM yet.

### `@limits` — deterministic constraints <span class="badge badge-soon">Coming soon</span>

```keel
@limits {
  max_cost_per_request: 0.50
  max_tokens_per_request: 4096
  timeout: 30.seconds
  require_confirmation: [Email.send, Db.exec]
}
```

Enforced by the runtime with deterministic logic. Violations raise errors; they don't just ask the LLM nicely.

> **Status:** parsed as a struct literal in v0.1 but not enforced — no cost, token, timeout, or confirmation gating yet.

### `@on_start` / `@on_stop` — lifecycle hooks

```keel
@on_start { Schedule.every(5.minutes, () => { heartbeat() }) }
@on_stop  { flush_queue() }      # Coming soon
```

Run when the agent starts and stops.

> **Status:** `@on_start` is fully wired. `@on_stop` <span class="badge badge-soon">Coming soon</span> is parsed but never invoked in v0.1 (graceful-shutdown hook not wired).

### Custom attributes

Any library can register a handler for a custom attribute. In your program:

```keel
@tracing "full"      # handler installed by keel/observability
@retry_policy { ... } # handler installed by a resilience library
```

## State

`state` declares mutable fields. Access is **only via `self.`**:

```keel
agent Counter {
  state {
    count: int = 0
  }

  on message(_: Message) {
    self.count = self.count + 1
  }
}
```

- Handlers for one agent run **sequentially** — no data races on `state`.
- Different agents run concurrently but share no state.
- Cross-agent messaging uses `Agent.send(Target, data)` (wired in v0.1). `Agent.delegate` and `Agent.broadcast` <span class="badge badge-soon">Coming soon</span>.

## Lifecycle

```keel
run(MyAgent)                      # start
run(MyAgent, background: true)    # background: Coming soon
stop(MyAgent)                     # graceful shutdown
```

`run` and `stop` are prelude functions re-exported at the top level.

> **Status:** `run(Agent)` and `stop(Agent)` are wired. `run(Agent, background: true)` <span class="badge badge-soon">Coming soon</span> — v0.1 treats every `run` as foreground and uses the event loop for non-blocking behavior.

## Composition over monoliths

Top-level tasks are reusable and testable. Prefer small agents that call shared top-level tasks over large agents with inline logic:

```keel
# Top-level, testable
task triage(email: EmailInfo) -> Urgency {
  Ai.classify(email.body, as: Urgency, fallback: Urgency.medium)
}

# Agent stays focused
agent EmailAssistant {
  @role "Triage and respond"

  on message(msg: Message) {
    urgency = triage(msg)
    Io.show({urgency: urgency, subject: msg.subject})
  }
}
```

Tasks defined *inside* an agent are scoped to that agent and can access `self`. Use them only when you genuinely need agent state access.
