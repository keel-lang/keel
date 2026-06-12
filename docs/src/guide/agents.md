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
use std/ai
use std/db
use std/email
use std/io
use std/schedule

agent EmailBot {
  # --- Attributes ---
  @role "Professional email triage"
  @tools [email, Calendar]
  @memory persistent
  @rules [
    "Never reveal internal pricing",
    "Always disclaim medical advice"
  ]
  @limits {
    max_cost_per_request: 0.50
    max_tokens_per_request: 4096
    timeout: 30.seconds
    require_confirmation: [email.send, db.exec]
  }

  # --- Mutable state (only via self.) ---
  state {
    processed: int = 0
    last_run: datetime? = none
  }

  # --- Agent tasks (can access self) ---
  task greet(name: str) -> str {
    ai.draft("greeting for {name}", tone: "warm") ?? "Hello!"
  }

  # --- Event handlers ---
  on message(msg: Message) {
    response = self.greet(msg.from)
    email.send(response, to: msg)
    self.processed = self.processed + 1
  }

  # --- Lifecycle hooks ---
  @on_start {
    schedule.every(1.day, at: @9am, () => {
      io.notify("Processed {self.processed} messages yesterday")
    })
  }
}

run(EmailBot)
```

## Attributes

Attributes are identifier-prefixed metadata clauses. They declare agent identity, capabilities, and lifecycle behavior without needing dedicated keywords. Only **two** attributes are built into the core language:

| Attribute | Core? | Status | Purpose |
|---|---|---|---|
| `@role` | Yes | ✅ | The agent's identity string. In v0.1 it's prepended as `"You are {role}.\n\n..."` to every `ai.*` system prompt, so the LLM sees the agent's directive on every call |
| `@model` | Yes | ✅ | The model name string; overrides the global default for this agent |

Everything else — `@tools`, `@memory`, `@rules`, `@limits`, `@on_start`, `@on_stop`, and user-defined attributes — is **stdlib-registered**. Adding a new attribute requires a library, not a language change.

> As of v0.1.10, `@on_start`, `@on_stop`, `@rules`, `@tools`, `@memory`, and `@limits` (timeout) are fully wired. `@team` is used by `broadcast` routing. `@provider` is parsed but has no runtime effect yet — <span class="badge badge-soon">Coming soon</span>. Individual sections note the status explicitly.

### `@tools` — capability list

```keel
@tools [email, http]      # allowlist
@tools all                # explicit unrestricted form
```

Binds stdlib modules as the agent's declared capabilities. The runtime uses this list to:
- Allow/deny which std module entry points the agent can call
- Report the agent's capabilities to the LLM (for tool-use style prompting, planned)

> **Status:** capability gating is enforced, **deny-by-default**, for **effectful** modules. A capability guards authority over the world outside the process — `ai`, `io`, `http`, `email`, `file`, `shell`, `db`, `search`, `env` — and an agent with no `@tools` attribute may call none of them. Pure-compute and internal modules (`json`, `math`, `time`, `schedule`, …) are never gated; they only need their import. `@tools all` is the explicit, greppable opt-out for trusted agents.

Enforcement is two-layered. Direct std calls in the agent body that `@tools` does not cover are **compile-time errors** that name the fix:

```text
agent `NoTools` calls `io.show` but @tools does not allow it —
declare `@tools [io]` on the agent, or use `@tools all`
```

Calls reached through helper tasks (in this file or imported modules) are checked **at runtime** per turn and raise `CapabilityError` with the same guidance. `@tools` must therefore cover the transitive *effectful* needs of the helpers an agent calls — if your agent calls `validation.load()` and that helper reads files, the agent needs `file` in its list. Helpers that only compute (json, math, strings) never require declarations.

Gating applies to effectful std module entry points inside agent turns only. Pure-compute modules, value methods (`conn.query(...)`), the built-in agent verbs, local module tasks, top-level statements, and `test` blocks are never gated.

#### Conditional guards with `if`

Entries can include an `if` guard — a boolean expression evaluated at the start of each handler turn. Guards can access `self.*` state and call tasks that return `bool`:

```keel
use std/db
use std/email

agent SupportBot {
  state {
    confirmed: bool = false
    admin: bool = false
  }

  @tools [
    email.fetch,                           # always allowed
    email.send if self.confirmed,           # only after confirmation
    db.query,                              # always allowed
    db.exec   if self.admin,                # admin only
    http,                                  # whole namespace, always
  ]
}
```

Calling a blocked method raises a `CapabilityError` at runtime. The guard is re-evaluated on every handler turn, so capabilities can be dynamically gated based on agent state.

### `@memory` — agent memory scope

```keel
@memory persistent    # | session | none
```

- `persistent` — survives restarts; stored in `~/.keel/memory/<stem>_<hash12>/<agent>.json` (JSON key-value store). The `<hash12>` is a SHA-256 fingerprint of the canonical source file path, so two programs with the same filename in different directories never share data.
- `session` — lives for the life of the process; cleared on restart. This is the **default** when `@memory` is omitted.
- `none` — disables `memory.*` for this agent; any call raises `CapabilityError`.

```keel
use std/io
use std/memory

agent Counter {
  @tools [io]
  @memory session

  @on_start {
    prev = memory.recall("count")
    count = if prev == none { 1 } else { prev + 1 }
    memory.remember("count", count)
    io.show("Visit {count}")
    stop(self)
  }
}
```

The `Memory` namespace provides three operations:

| Call | Description |
|---|---|
| `memory.remember(key, value)` | Store any Keel value under `key` |
| `memory.recall(key)` | Return the stored value, or `none` if absent |
| `memory.forget(key)` | Delete the key |

> **Note:** The persistent backend is a simple JSON file, not a vector store. Semantic search (`ai.embed` + nearest-neighbour recall) is planned for v0.2 via the `VectorStore` interface.

### `@rules` — natural-language guardrails

```keel
@rules [
  "Never reveal internal pricing logic",
  "Escalate if the user expresses frustration 3+ times"
]
```

Rules are injected into every LLM prompt this agent makes as a bullet list under a `Rules:` heading, placed between the role preamble and the operation-specific instructions. They are **LLM-interpreted** — compliance is best-effort. For deterministic constraints, use `@limits`.

> **Status:** fully wired as of v0.1.3. Every `ai.*` call inside an agent with `@rules` forwards the rules to the system prompt.

### `@limits` — deterministic constraints <span class="badge badge-soon">Coming soon</span>

```keel
@limits {
  max_cost_per_request: 0.50
  max_tokens_per_request: 4096
  timeout: 30.seconds
  require_confirmation: [email.send, db.exec]
}
```

Enforced by the runtime with deterministic logic. Violations raise errors; they don't just ask the LLM nicely.

> **Status:** `timeout` is enforced via `control.with_timeout`. Cost, token, and confirmation gates are parsed but not enforced at the Ollama level yet.

### `@on_start` / `@on_stop` — lifecycle hooks

```keel
@on_start { schedule.every(5.minutes, () => { heartbeat() }) }
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
- Cross-agent messaging: `send(Target, data)` (v0.1), `delegate(Target, task, args)` (v0.1.4), `broadcast(team, data, event:)` (v0.1.6). See [Agent Communication](./agent-communication.md).

### Readonly fields

Add `readonly` between the colon and the type to make a field **compiler-enforced read-only**. Any `self.field = ...` assignment is a compile-time error; reading is always allowed.

```keel
use std/io

agent SessionBot {
  @tools [io]
  state {
    turns:      int          = 0
    session_id: readonly str = "default-session"
  }

  on message(msg: str) {
    self.turns = self.turns + 1          # ok — writable
    # self.session_id = "x"             # compile error
    io.show(self.session_id)             # reading is fine
  }
}
```

Use readonly fields for runtime-provided context (session IDs, request metadata) that the agent must observe but never modify.

## Lifecycle

```keel
run(MyAgent)                      # start
run(MyAgent, background: true)    # background: Coming soon
stop(MyAgent)                     # graceful shutdown
stop(self)                        # self-stop from inside the agent
```

`run` and `stop` are prelude functions re-exported at the top level. Inside an agent body, bare `self` resolves to the current agent reference, so `stop(self)` is equivalent to `stop(MyAgent)` without hard-coding the name.

The `Agent` namespace exposes the same operations with an explicit prefix:

| Function | Equivalent | Notes |
|---|---|---|
| `run(name)` | `run(name)` | Start a named agent |
| `stop(name)` | `stop(name)` | Gracefully stop a running agent |
| `send(name, data)` | — | Post a message to an agent's mailbox |
| `delegate(symbol, data)` | — | Invoke a typed handler and await the result |
| `broadcast(team, data)` | — | Fan out an event to all agents on a team |

Use the `Agent.*` form when you need to start or stop agents whose names are only known at runtime (e.g., dynamically spawned workers).

> **Status:** `run(Agent)` and `stop(Agent)` are wired. `run(Agent, background: true)` <span class="badge badge-soon">Coming soon</span> — v0.1 treats every `run` as foreground and uses the event loop for non-blocking behavior.

## Composition over monoliths

Top-level tasks are reusable and testable. Prefer small agents that call shared top-level tasks over large agents with inline logic:

```keel
use std/ai
use std/email
use std/io

# Top-level, testable
task triage(email: EmailInfo) -> Urgency {
  ai.classify(email.body, as: Urgency) ?? Urgency.medium
}

# Agent stays focused
agent EmailAssistant {
  @tools [io]
  @role "Triage and respond"

  on message(msg: Message) {
    urgency = triage(msg)
    io.show({urgency: urgency, subject: msg.subject})
  }
}
```

Tasks defined *inside* an agent are scoped to that agent and can access `self`. Use them only when you genuinely need agent state access.
Invoke them as `self.task(...)`; bare `task(...)` remains a lexical/top-level call.
`MyAgent.task(...)` is not the cross-agent composition model — use
`Agent.send`, `Agent.delegate`, or `Agent.broadcast` instead.
