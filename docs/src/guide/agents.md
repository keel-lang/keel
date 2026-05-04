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

> As of v0.1.10, `@on_start`, `@on_stop`, `@rules`, `@tools`, `@memory`, and `@limits` (timeout) are fully wired. `@team`, `@provider` are parsed but have no runtime effect yet — <span class="badge badge-soon">Coming soon</span>. Individual sections note the status explicitly.

### `@tools` — capability list <span class="badge badge-soon">Coming soon</span>

```keel
@tools [Email, Calendar, Http]
```

Binds stdlib modules as the agent's declared capabilities. The runtime uses this list to:
- Allow/deny which stdlib namespaces the agent can use
- Report the agent's capabilities to the LLM (for tool-use style prompting)

> **Status:** parsed in v0.1, no capability gating enforced yet.

### `@memory` — agent memory scope

```keel
@memory persistent    # | session | none
```

- `persistent` — survives restarts; stored in `~/.keel/memory/<stem>_<hash12>/<agent>.json` (JSON key-value store). The `<hash12>` is a SHA-256 fingerprint of the canonical source file path, so two programs with the same filename in different directories never share data.
- `session` — lives for the life of the process; cleared on restart. This is the **default** when `@memory` is omitted.
- `none` — disables `Memory.*` for this agent; any call raises `CapabilityError`.

```keel
agent Counter {
  @memory session

  @on_start {
    prev = Memory.recall("count")
    count = if prev == none { 1 } else { prev + 1 }
    Memory.remember("count", count)
    Io.show("Visit {count}")
    stop(self)
  }
}
```

The `Memory` namespace provides three operations:

| Call | Description |
|---|---|
| `Memory.remember(key, value)` | Store any Keel value under `key` |
| `Memory.recall(key)` | Return the stored value, or `none` if absent |
| `Memory.forget(key)` | Delete the key |

> **Note:** The persistent backend is a simple JSON file, not a vector store. Semantic search (`Ai.embed` + nearest-neighbour recall) is planned for v0.2 via the `VectorStore` interface.

### `@rules` — natural-language guardrails

```keel
@rules [
  "Never reveal internal pricing logic",
  "Escalate if the user expresses frustration 3+ times"
]
```

Rules are injected into every LLM prompt this agent makes as a bullet list under a `Rules:` heading, placed between the role preamble and the operation-specific instructions. They are **LLM-interpreted** — compliance is best-effort. For deterministic constraints, use `@limits`.

> **Status:** fully wired as of v0.1.3. Every `Ai.*` call inside an agent with `@rules` forwards the rules to the system prompt.

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
@on_stop  { flush_queue() }
```

Run when the agent starts and stops.

> **Status:** Both `@on_start` and `@on_stop` are fully wired as of v0.1.4.

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
- Cross-agent messaging: `Agent.send(Target, data)` (v0.1), `Agent.delegate(Target, task, args)` (v0.1.4), `Agent.broadcast(team, data, event:)` (v0.1.6). See [Agent Communication](./agent-communication.md).

## Lifecycle

```keel
run(MyAgent)                      # start
run(MyAgent, background: true)    # background: Coming soon
stop(MyAgent)                     # graceful shutdown
stop(self)                        # self-stop from inside the agent
```

`run` and `stop` are prelude functions re-exported at the top level. Inside an agent body, bare `self` resolves to the current agent reference, so `stop(self)` is equivalent to `stop(MyAgent)` without hard-coding the name.

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
