# Keel Language Specification — v0.1 (Alpha)

> **Status: Alpha.** Keel is in early design. The language is **not yet stable** and has **no production users**. Expect breaking changes between 0.x releases. This document is the authoritative design for v0.1.

---

## 0. The Shape of Keel

Keel is a programming language for building AI agents. Two ideas define it:

1. **The actor model is core.** An `agent` is the primitive unit of concurrency — a serial-handler mailbox with isolated mutable state. This is the only primitive that can't be a library.
2. **Everything else is a library.** AI calls, scheduling, I/O, HTTP, memory, search, tool integration — all live in a standard library that ships with the runtime and is **auto-imported** (the *prelude*). Users don't write `use keel/ai`; they just write `Ai.classify(...)` and it works.

Because the prelude is auto-imported, `Ai.classify(...)` reads as if `classify` were a keyword — but the compiler doesn't know or care what `Ai` is. That keeps the core language small (fewer keywords, fewer parser special cases, fewer type-inference rules) while keeping the ergonomics.

### Design principles

1. **Small core, deep stdlib.** Every feature that can be a library is one. The core earns its keep through the type system, the compiler, or the actor runtime — not through surface syntax convenience.
2. **Static typing with full inference.** Every expression has a known type. Mismatches are compile errors. Annotations are rarely needed.
3. **No silent fallbacks.** Operations that can fail return nullable types. The caller handles the failure explicitly with `??`, `fallback:`, or `when`.
4. **Tooling from day one.** Every feature is designed so the LSP can autocomplete, go-to-def, rename, and surface diagnostics.
5. **Escape hatches are explicit.** `dynamic`, `extern`, `prompt` exist for real needs but must be opted into visibly.

### Core vs. stdlib arbitrage test

> **For every feature, ask: can a library replicate this with identical safety, ergonomics, and performance?**
>
> - Yes → stdlib.
> - No → core.

Applied ruthlessly, this test keeps the reserved-keyword list to ~27 words.

---

## 1. Program Structure

A Keel program is a `.keel` file containing top-level declarations. No `main()` — execution begins at the first top-level statement (typically `run(MyAgent)`).

```keel
# my_agent.keel

agent Greeter {
  @role "You greet people warmly"
}

run(Greeter)
```

### File extension: `.keel`
### Comments: `#` for single-line, `## ... ##` for multi-line

### Top-level declarations

A file may contain, in any order:
- `agent` declarations
- `task` declarations (free-standing)
- `type` declarations (structs, enums, aliases)
- `interface` declarations (protocols)
- `extern` declarations
- `use` imports
- Top-level statements (`run(...)`, variable bindings, etc.)

---

## 2. Type System

Keel uses a **structural type system with full inference**. Every expression is typed. Mismatches are compile errors. Annotations are rarely needed.

### 2.1 Design principles

1. **Structural typing.** Types are shapes, not names. A value matches a type if it has the required fields. No explicit `implements`.
2. **Full inference.** Initializers, returns, and stdlib signatures drive inference. Explicit annotations override.
3. **Algebraic data types.** Enums can carry associated data per variant.
4. **Nullable safety.** Types are non-nullable by default. `?` marks a type as nullable.
5. **No implicit `any`.** `dynamic` is the one escape hatch, and it must be explicitly opted into.

### 2.2 Primitive types

| Type | Example | Notes |
|---|---|---|
| `int` | `42` | 64-bit integer |
| `float` | `3.14` | 64-bit float |
| `str` | `"hello"` | UTF-8, interpolation `{expr}`, escapes `\n \t \r \" \\ \{ \}` |
| `bool` | `true`, `false` | |
| `none` | `none` | Unit type / absence value |
| `duration` | `5.minutes`, `2.hours` | Duration literals |
| `datetime` | `@2026-04-15`, `@monday_9am` | Time literals |
| `dynamic` | — | FFI/interop boundary only |

**Built-in constants:** `true`, `false`, `none`, `now`.

**`none` semantics.** `none` is both the unit type and the nullable-empty value. `none?` is equivalent to `none`. The tuple unit `()` is equivalent to `none`.

**Multi-line strings.** Triple-quoted `"""..."""` preserves newlines and indentation. Same interpolation and escape rules.

### 2.3 Collection types

```keel
nums: list[int]      = [1, 2, 3]
info: map[str, str]  = {name: "Zied", role: "builder"}
ids: set[int]        = set[1, 2, 3]
```

Rules: `[...]` is a list. `{k: v}` is a map. `set[...]` is a set (the only place `set` is special: it's a keyword-like form, not a type application).

**Built-in collection operations** (methods, enabled by lambdas):

| Method | Signature |
|---|---|
| `.count` | `int` |
| `.first`, `.last` | `T?` |
| `.is_empty` | `bool` |
| `.map(fn)` | `list[T].(T -> U) -> list[U]` |
| `.filter(fn)` | `list[T].(T -> bool) -> list[T]` |
| `.find(fn)` | `list[T].(T -> bool) -> T?` |
| `.any(fn)`, `.all(fn)` | `list[T].(T -> bool) -> bool` |
| `.sort_by(fn)` | `list[T].(T -> U) -> list[T]` |
| `.group_by(fn)` | `list[T].(T -> U) -> map[U, list[T]]` |
| `.flat_map(fn)` | `list[T].(T -> list[U]) -> list[U]` |

Maps expose `.count`, `.keys`, `.values`. Sets expose `.count`, `.contains(v)`, `.is_empty`.

**String methods:** `.length`, `.is_empty`, `.contains`, `.starts_with`, `.ends_with`, `.trim`, `.upper`, `.lower`, `.split`, `.replace`, `.slice`.

**Conversions:** `.to_int()`, `.to_float()`, `.to_str()`. Fallible conversions return nullable (`str.to_int() -> int?`).

### 2.4 Struct types (structural records)

```keel
type EmailInfo {
  sender: str
  subject: str
  body: str
  unread: bool
}

# Inline
task triage(email: {body: str, from: str}) -> Urgency { ... }
```

**Width subtyping.** A value of type `A` is assignable to `B` if `A` has all fields of `B` with compatible types. Extra fields are allowed.

**Generic structs:**

```keel
type Paginated[T] {
  items: list[T]
  page: int
  has_more: bool
}
```

### 2.5 Enum types (algebraic data types)

```keel
# Simple variants
type Urgency = low | medium | high | critical

# Rich variants with associated data
type Action =
  | reply { to: str, tone: str }
  | forward { to: str }
  | archive
  | escalate { reason: str, urgency: Urgency }
```

**Construction:** `Action.reply { to: "x", tone: "y" }`. Data-less variants: `Action.archive`.

**Pattern matching** is exhaustive (see §8.2). Rich variant fields are destructured in `when` arms, not accessed via dot.

**Generic enums:**

```keel
type Result[T, E] =
  | ok { value: T }
  | err { error: E }
```

### 2.6 Type aliases

```keel
type Timestamp = datetime
type ContactEmail = str
type Handler = (Message) -> str
```

Aliases are structurally transparent. `ContactEmail` and `str` are interchangeable. For nominal distinction, use a wrapper struct.

### 2.7 Nullable types

```keel
name: str   = "Keel"
alias: str? = none

subject = email?.subject           # str? — none-safe field access
subject = email?.subject ?? "(none)"  # str — default via ??
subject = email!.subject           # str — throws NullError on none (unsafe)
```

Stdlib and AI operations return nullable types when they can fail. Use `??`, `fallback:`, or `when` to handle.

### 2.8 Tuple types

```keel
pair: (str, int) = ("hello", 42)
(name, count) = pair             # destructure
x = pair.0                        # positional access
```

Tuples are structural, immutable. Single-element tuples are not a thing (`(str)` is `str`). `()` is `none`.

### 2.9 The `Result` type

```keel
type Result[T, E] =
  | ok { value: T }
  | err { error: E }
```

Returned by stdlib operations that need to surface rich errors (e.g., `Http.request`). The variants are matched with `when`.

### 2.10 The `dynamic` type (FFI/interop only)

`dynamic` exists for untyped boundaries: `extern` returns, `prompt as dynamic`, raw SQL rows. It must always be explicitly written — there is no implicit path to `dynamic`.

```keel
extern task parse_legacy(data: str) -> dynamic from "legacy"

raw = Ai.prompt(...) as dynamic       # must opt in
info: MyStruct = raw as MyStruct      # narrow with runtime check
```

`dynamic` defeats autocomplete and type checking. Narrow as early as possible. The compiler warns on `dynamic` use outside the explicit escape hatches.

### 2.11 Built-in runtime types

Provided by the prelude, available without imports:

```keel
type Message {
  from: str
  body: str
  channel: str?
  timestamp: datetime?
}

type SearchResult { title: str, url: str, snippet: str }

type Memory { content: map[str, str], relevance: float, created_at: datetime }

type HttpResponse {
  status: int
  body: str
  headers: map[str, str]
}
# HttpResponse.is_ok : bool
# HttpResponse.json_as[T]() : T?

type Decision[T] { choice: T, reason: str, confidence: float }

type Error =
  | AIError { model: str, tokens_used: int }
  | NetworkError { status: int?, url: str }
  | TimeoutError { duration: duration }
  | NullError
  | TypeError { expected: str, got: str }
  | ParseError { position: int }
# All variants implicitly carry message: str, source: str?
```

### 2.12 Variable bindings and mutability

Immutable by default. `=` creates a binding. Rebinding in the same scope **shadows** the previous binding (the old value is untouched, the name now points to a new value — Rust-style).

```keel
name = "Keel"
name = "Other"    # shadowing, not mutation
```

**The one exception: agent `state` fields**, accessed via `self.`:

```keel
self.count = self.count + 1
```

`self` is only available inside agent bodies. Top-level tasks have no `self`.

---

## 3. The Prelude (the Stdlib as Keywords)

The Keel standard library lives in a set of namespaces that are **auto-imported into every program**. Users don't write `use keel/ai` to get `Ai.classify` — the name is already in scope.

### 3.1 Why a prelude

- **Small core.** The compiler doesn't know about `classify`, `fetch`, or `every`. Those are stdlib function calls that happen to always be in scope. Parser, lexer, and type checker stay free of domain-specific special cases.
- **Keyword feel.** Users still write `Ai.classify(...)` without ceremony. The namespace qualifier is short; autocomplete takes care of the rest.
- **Swappable implementations.** Stdlib functions dispatch through **interfaces** (§5). Users can install their own LLM provider, scheduler, memory store, or HTTP client without leaving the language.
- **No grammatical ambiguity.** `fetch x where y` required whole-grammar disambiguation. `Http.get(x, where: y)` is unambiguous and tool-friendly.

### 3.2 Prelude namespaces (v0.1)

| Namespace | Purpose | Key operations |
|---|---|---|
| `Ai` | LLM-backed operations | `classify`, `extract`, `summarize`, `draft`, `translate`, `decide`, `prompt`, `embed` |
| `Io` | Human interaction | `ask`, `confirm`, `notify`, `show` |
| `Http` | HTTP client | `get`, `post`, `request` |
| `Email` | IMAP/SMTP | `fetch`, `send`, `archive` |
| `Search` | Web search providers | `web(query)`, custom providers via interface |
| `Db` | SQL databases | `connect`, `query`, `exec` |
| `Memory` | Persistent semantic memory | `remember`, `recall`, `forget` |
| `Schedule` | Time-based scheduling | `every`, `after`, `at`, `cron` |
| `Async` | Structured concurrency | `spawn`, `join_all`, `select`, `sleep` |
| `Control` | Control combinators | `retry`, `with_timeout`, `with_deadline` |
| `Env` | Environment and config | `get(name)`, `require(name)` |
| `Time` | Time utilities | `now`, `parse`, `format`, duration math |
| `Log` | Structured logging | `info`, `warn`, `error`, `debug` |
| `Agent` | Agent lifecycle | `run`, `stop`, `delegate`, `broadcast` (also exposed as bare `run`/`stop` at top level) |

### 3.3 Prelude surface is identifiers, not keywords

`Ai`, `Io`, `Schedule`, etc. are **identifiers** whose bindings are installed by the runtime into the root scope. A user program can shadow them (`Ai = my_module` is legal, if unwise). They do not appear in the reserved keyword list (§10). This is the crucial difference: the language doesn't know about `Ai`. The runtime does.

### 3.4 Example: everything you need, no imports

```keel
# Zero imports. All namespaces are in scope.

type Urgency = low | medium | high | critical

agent EmailBot {
  @role "Professional email triage"
  @model "claude-sonnet"

  state {
    processed: int = 0
  }

  on message(msg: Message) {
    urgency = Ai.classify(msg.body, as: Urgency, fallback: Urgency.medium)

    when urgency {
      low, medium => {
        reply = Ai.draft("response to {msg.body}", tone: "friendly")
        if Io.confirm(reply) {
          Email.send(reply, to: msg.from)
        }
      }
      high, critical => {
        Io.notify("{urgency}: {msg.subject}")
        guidance = Io.ask("How to respond?")
        reply = Ai.draft("response to {msg.body}", guidance: guidance)
        if Io.confirm(reply) {
          Email.send(reply, to: msg.from)
        }
      }
    }

    self.processed = self.processed + 1
  }

  # Scheduling is a library call, not a keyword.
  # The block registers a recurring event on this agent's mailbox.
  @on_start {
    Schedule.every(5.minutes, () => {
      for email in Email.fetch(unread: true) {
        # deliver to this agent's message handler
        self.dispatch(message: email.as_message())
      }
    })
  }
}

run(EmailBot)
```

---

## 4. Agents

The actor model is Keel's one concurrency primitive. Agents are isolated, serial, message-driven coroutines with mutable state accessible only through `self`.

### 4.1 Minimal agent

```keel
agent Greeter {
  @role "You greet people warmly"
}
```

### 4.2 Full agent anatomy

```keel
agent AgentName {
  # --- Attributes (stdlib-defined metadata) ---
  @role "Natural language description"
  @model "claude-sonnet"             # LLM binding for Ai.* inside this agent
  @tools [Email, Calendar]           # capability bindings
  @memory persistent                 # stdlib memory binding (none | session | persistent)
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

  # --- State (mutable via self.) ---
  state {
    processed: int = 0
    last_run: datetime? = none
  }

  # --- Agent tasks (methods) ---
  task greet(name: str) -> str {
    Ai.draft("greeting for {name}", tone: "warm") ?? "Hello!"
  }

  # --- Event handlers ---
  on message(msg: Message) {
    response = greet(msg.from)
    Email.send(response, to: msg)
    self.processed = self.processed + 1
  }

  # --- Lifecycle hooks (stdlib attribute) ---
  @on_start {
    Schedule.every(1.day, at: @9am, () => {
      Io.notify("Good morning — {self.processed} messages processed yesterday")
    })
  }
}
```

### 4.3 Attributes (`@name`)

Attributes are identifier-prefixed metadata clauses inside an agent body. The core language knows only two attributes:

| Attribute | Core-defined? | Semantics |
|---|---|---|
| `@role` | Yes | The agent's identity string, bound to the installed `LlmProvider` for all `Ai.*` calls. |
| `@model` | Yes | The model name string, overrides the global default for this agent's `Ai.*` calls. |

Everything else (`@tools`, `@memory`, `@rules`, `@limits`, `@on_start`, `@on_stop`, custom attributes) is **stdlib-defined**: libraries register attribute handlers at startup, and the runtime invokes them during agent initialization to wire up capabilities.

**Why attributes and not keywords?** A keyword requires a grammar rule and couples the compiler to a specific feature. An attribute is just a name. Adding `@my_custom_attr` requires no language change — only a handler in the library that provides it. The user's file parses identically regardless of which libraries are loaded.

### 4.4 Agent lifecycle

```keel
run(MyAgent)                 # start
run(MyAgent, background: true)  # non-blocking
stop(MyAgent)                # graceful shutdown
```

`run` and `stop` are **prelude functions** in the `Agent` namespace, re-exported at top level for convenience.

### 4.5 State and thread safety

- Agent `state` fields are mutable **only via `self.`**.
- Event handlers for one agent run **sequentially**. No concurrent access to `state`.
- Different agents run concurrently but share no state.
- Cross-agent data flows through `Agent.delegate`, `Agent.broadcast`, or `Memory.*`.

### 4.6 Composition over monoliths

Top-level tasks are reusable and testable. Prefer small agents that call top-level tasks over large agents with inline logic.

```keel
task triage(email: EmailInfo) -> Urgency {
  Ai.classify(email.body, as: Urgency, fallback: Urgency.medium)
}

agent EmailAssistant {
  @role "Triage and respond"
  on message(msg: Message) {
    urgency = triage(msg)
    Io.show({urgency: urgency, subject: msg.subject})
  }
}
```

---

## 5. Interfaces (Protocols)

Interfaces declare a set of method signatures. A type satisfies an interface **structurally** — if it has all the required methods with compatible signatures, it is an instance.

### 5.1 Declaration

```keel
interface LlmProvider {
  task complete(messages: list[Message], opts: LlmOpts) -> LlmResponse?
  task embed(text: str) -> list[float]?
}

interface VectorStore {
  task put(key: str, value: map[str, str], embedding: list[float]) -> none
  task query(embedding: list[float], limit: int) -> list[Memory]
}

interface Tracer {
  task on_event(event: TraceEvent) -> none
}
```

### 5.2 Why interfaces are core

- `Ai.classify` needs to dispatch to *some* LLM implementation. Hard-coding Anthropic/OpenAI into the runtime locks users out of self-hosted, proprietary, or novel providers.
- `Memory.recall` needs a vector store — there are many, users should pick.
- `Log.info` needs a sink — users want OTel, Datadog, or plain stdout.

The language can't know about every provider. Interfaces let stdlib declare the *protocol*, ship a default implementation, and let users swap.

### 5.3 Installing implementations

Implementations are installed at runtime startup, typically in the program's top-level section:

```keel
# At startup — swap the default LLM provider
Ai.install(MyAnthropicProvider)

# Per-agent override
agent Specialist {
  @model "my-custom-model"
  @provider MyOllamaProvider      # stdlib attribute that installs for this agent
}
```

Installation is scoped: per-program, per-agent, or per-call (via an explicit `using:` argument to stdlib functions).

---

## 6. Tasks

Tasks are named, reusable operations. They are Keel's functions.

### 6.1 Basic task

```keel
task greet(name: str) -> str {
  "Hello, {name}!"
}
```

The last expression is the implicit return. Explicit `return` is supported for early exit.

### 6.2 Task with AI operations

```keel
task triage(email: {body: str}) -> Urgency {
  Ai.classify(email.body, as: Urgency, fallback: Urgency.medium)
}
```

### 6.3 Pipelines

The `|>` operator passes the left value as the first argument to the right function.

```keel
email |> triage |> respond |> log
# equivalent to:
log(respond(triage(email)))
```

With extra arguments:

```keel
email |> triage |> respond(tone: "friendly")
```

### 6.4 Signatures

```keel
task cleanup() { ... }                            # no params, returns none
task greet(name: str) { "Hello, {name}!" }        # inferred return
task triage(e: EmailInfo) -> Urgency { ... }      # annotated
task compose(e: EmailInfo, tone: str = "pro") {...}  # default params
task quick(d: {body: str}) -> str { d.body }      # structural param
```

---

## 7. Lambdas and first-class functions

```keel
# Single-param shorthand
triaged = emails.map(e => triage(e))

# Multi-param
pairs = left.zip_with(right, (a, b) => a + b)

# Block body
scored = emails.map(e => {
  urgency = triage(e)
  {email: e, urgency: urgency}
})

# Named function as a value
results = emails.map(triage)
```

### Function types

```keel
type Handler = (Message) -> str
type Predicate[T] = (T) -> bool

task process_all(emails: list[EmailInfo], handler: (EmailInfo) -> str) {
  emails.map(handler)
}
```

---

## 8. Control Flow

### 8.1 `if` / `else` (expression)

```keel
# As statement (else optional)
if urgency == Urgency.high { escalate(email) }

# As expression (else REQUIRED, branches must produce compatible types)
reply = if guidance != none {
  Ai.draft("response", guidance: guidance)
} else {
  Ai.draft("response", tone: "friendly")
} ?? "(draft failed)"
```

An `if` without `else` used as an expression is a compile error.

### 8.2 `when` (pattern matching)

Exhaustive. All enum variants must be covered, or a wildcard `_` must be present.

```keel
when urgency {
  low, medium => auto_reply(email)
  high        => flag_and_draft(email)
  critical    => escalate(email)
}
```

**Rich variant matching:**

```keel
when action {
  reply { to, tone }   => Email.send(Ai.draft("reply", tone: tone), to: to)
  forward { to }       => Email.send(email, to: to)
  archive              => Email.archive(email)
  escalate { reason, urgency }
    where urgency == Urgency.critical => page_oncall(reason)
  escalate { reason, _ } => Io.notify("Escalation: {reason}")
}
```

**Tuple and struct patterns, `where` guards** — see §21 grammar for the full form.

**Non-enum matching (primitives, strings):** wildcard `_` is **required** (the compiler can't prove exhaustiveness on unbounded types).

### 8.3 `for` loops

```keel
for email in emails { process(email) }
for email in emails where email.unread { triage(email) }
```

### 8.4 Destructuring

```keel
{urgency, category} = result                # struct
{urgency: u, category: c} = result          # rename
(urgency, summary) = triage_full(email)     # tuple

for {from, subject} in emails { ... }       # in for
task handle({body, from}: EmailInfo) { ... }  # in params
```

### 8.5 `try` / `catch`

Catches by **variant matching** — the same mechanism as `when` on any enum. `Error` is the catch-all type.

```keel
try {
  Email.send(reply, to: email.from)
} catch err: NetworkError {
  Control.retry(3, backoff: exponential, () => { Email.send(reply, to: email.from) })
} catch err: Error {
  Io.notify("Send failed: {err.message}")
}
```

---

## 9. Concurrency

### 9.1 Core primitives

The core runtime exposes exactly three concurrency primitives, surfaced via `Async`:

| Primitive | Type | Behavior |
|---|---|---|
| `Async.spawn(fn)` | `() -> T` returning `Task[T]` | Start a child task. Parent-cancels-children semantics. |
| `Task[T].await()` | `T` | Block the current handler until the task completes. |
| `Task[T].cancel()` | `none` | Cancel the task. |

Everything else is a library combinator:

```keel
Async.join_all(tasks: list[Task[T]]) -> list[T]   # all-or-nothing; cancels siblings on error
Async.select(tasks: list[Task[T]]) -> T            # first to complete wins
Async.sleep(d: duration) -> none
```

### 9.2 Structured concurrency

Cancellation is structured: when a parent task cancels or errors, all spawned children cancel. This is the one contract the runtime upholds.

### 9.3 No `parallel` / `race` keywords

Concurrent composition is expressed through library functions, not grammar:

```keel
[urgency, sentiment] = Async.join_all([
  Async.spawn(() => Ai.classify(body, as: Urgency, fallback: Urgency.medium)),
  Async.spawn(() => Ai.classify(body, as: Sentiment, fallback: Sentiment.neutral))
])
```

Trade-off: a dedicated `parallel { ... }` block would read slightly nicer. The library form is more honest about what's happening and extensible — users can write `join_first_n`, `join_settled`, etc. without a language change.

### 9.4 Agent event queue

```
  event ──>   ┌──────────────────┐
  event ──>   │  mailbox         │ ──> handler (sequential)
  timer ──>   │  (per agent)     │
              └──────────────────┘
```

Events land in the agent's mailbox. The runtime processes them one at a time. A handler that calls `Io.ask`, `Async.sleep`, or `Agent.delegate` suspends — other *agents* continue. Other events for the *same* agent queue behind.

---

## 10. Reserved Keywords

This is the complete set. If a word is not on this list, it is an identifier.

```
agent task interface type extern enum
use from
state on self
if else when where
for in
try catch return
as and or not
true false none now
```

That's it. **22 words.**

Namespaces (`Ai`, `Io`, `Http`, `Schedule`, `Async`, …) are identifiers, not keywords. Same for `run`, `stop`, `spawn`, `delegate`, `broadcast` — prelude functions.

Attribute names (`@role`, `@model`, `@tools`, …) are identifiers. Only the `@` prefix is syntax.

Duration units (`seconds`, `minutes`, `hours`, `days`, `weeks`) are **identifiers recognized by the lexer in the `INT "."` position**, not reserved words.

---

## 11. Error Handling

### 11.1 Error type

`Error` is a rich enum (§2.11). All variants carry `message: str` and `source: str?` implicitly. Catch clauses match variants.

### 11.2 Nullable-aware stdlib

AI and I/O operations that can fail return nullable types. The caller handles:

```keel
# Option 1: default via ??
summary = Ai.summarize(article, in: 3, unit: sentences) ?? "No summary available"

# Option 2: fallback parameter (for enum returns)
urgency = Ai.classify(text, as: Urgency, fallback: Urgency.medium)

# Option 3: explicit when
when Ai.classify(text, as: Urgency) {
  some(u)  => handle(u)
  none     => Io.notify("Could not classify")
}
```

### 11.3 Retry

`Control.retry` is a stdlib function:

```keel
Control.retry(3, backoff: exponential, () => {
  Email.send(reply, to: addr)
})

Control.retry(5, delay: 10.seconds, () => {
  Http.get("https://api.example.com/data")
})
```

### 11.4 Limits and rules

Both are stdlib-defined attributes (`@limits`, `@rules`):

- **`@limits`** are **deterministic** constraints enforced by the runtime: cost per request, token caps, timeouts, required-confirmation action lists. Violations are rejected.
- **`@rules`** are **natural-language instructions** the stdlib injects into LLM prompts. LLM compliance is best-effort, not guaranteed.

The separation is intentional: limits are verifiable, rules are aspirational. Mixing them would hide the difference.

---

## 12. Memory (persistent)

Memory is a stdlib module backed by a `VectorStore` interface. Agents opt in with `@memory persistent` (or `session` or `none`).

```keel
agent Support {
  @memory persistent

  on ticket(t: Ticket) {
    prior = Memory.recall("issues similar to {t.description}", limit: 5)
    resolution = resolve(t, prior)
    Memory.remember({
      contact: t.contact,
      resolution: resolution.summary,
      at: now
    })
  }
}
```

Default implementation: embedded SQLite + a local embedding model. Swap by installing a different `VectorStore` implementation at startup.

---

## 13. Escape Hatches

### 13.1 `Ai.prompt` — raw LLM access

```keel
score = Ai.prompt(
  system: "Rate sentiment 1–10.",
  user: "Text: {review}",
  response_format: json
) as SentimentScore
# score: SentimentScore? — parsing/validation may fail
```

`Ai.prompt(...)` **must be followed by `as T`**. A bare `Ai.prompt(...)` that tries to use the result is a compile error. Use `as dynamic` to explicitly opt out of typing.

### 13.2 `Http.request` — raw HTTP

```keel
r = Http.request(
  method: POST,
  url: "https://api.example.com/v2",
  headers: {Authorization: "Bearer {Env.require("API_KEY")}"},
  body: {text: review},
  timeout: 10.seconds
)
# r: HttpResponse?
```

### 13.3 `Db.query` — raw SQL

```keel
rows = Db.query(
  "SELECT * FROM interactions WHERE contact = ? AND created_at > ?",
  params: [email.from, 30.days.ago]
)
# rows: list[dynamic]
```

### 13.4 `extern` — call external code

```keel
extern task tokenize(text: str) -> list[str] from "nlp_utils"

tokens = tokenize(document.body)
```

`extern` is the one place type annotations are mandatory — the compiler can't infer across a language boundary. Runtime dispatches via a plugin ABI (shared library or subprocess+JSON).

---

## 14. Environment & Configuration

### 14.1 Environment variables

```keel
api_key = Env.require("OPENAI_API_KEY")   # fails at startup if missing
db_url  = Env.get("DATABASE_URL")          # str? — none if missing
```

`Env` is a prelude namespace backed by the host environment.

### 14.2 Configuration file

`keel.config` (YAML) is loaded by the runtime at startup and populates default attribute values:

```yaml
model: "claude-sonnet"
ai:
  default_temperature: 0.7
  cost_limit_daily: 10.00
memory:
  backend: sqlite
  path: .keel/memory.db
log:
  level: info
```

---

## 15. Modules & Imports

```keel
use "./email_utils.keel"               # import a local file
use Classifier from "./classifiers.keel"  # import a symbol
use community/crm                      # import a package
```

The prelude is always imported. `use` adds additional modules to scope.

---

## 16. Operators

| Operator | Meaning |
|---|---|
| `\|>` | Pipeline — left value as first arg to right function |
| `=>` | Case mapping in `when` |
| `->` | Return type annotation |
| `??` | Null coalesce |
| `?.` | Null-safe field access |
| `!.` | Null assertion (throws) |
| `..` | Inclusive range |
| `in` | Membership |
| `as` | Type narrowing (in `dynamic` casts, `Ai.prompt as T`, etc.) |
| `==` `!=` `<` `>` `<=` `>=` | Comparison |
| `and` `or` `not` | Boolean logic |
| `+` `-` `*` `/` `%` | Arithmetic |

---

## 17. Execution Model

Keel runs on the **Keel Runtime** (Rust, Tokio).

```
v0.1 (alpha):   .keel → Lexer → Parser → Typechecker → Interpreter
(later, TBD):   bytecode VM
(later, TBD):   LLVM AOT backend → native binary
```

### Runtime services

The runtime is intentionally small. It provides only what stdlib needs to exist on top of it:

1. **Event loop** (Tokio).
2. **Agent scheduler** — mailboxes, handler sequencing, structured cancellation.
3. **Timer primitives** — `sleep`, `deadline`. Stdlib `Schedule.*` is built on these.
4. **Interface dispatch** — registry of installed implementations per interface.
5. **Plugin ABI** — for `extern` and dynamically loaded stdlib backends.
6. **Tracer hook** — emits structured events at task/handler boundaries; stdlib `Log.*` subscribes.

Everything else — HTTP, IMAP/SMTP, LLM clients, databases, vector stores — is stdlib and ships with the runtime binary but is replaceable.

---

## 18. Compile-Time Errors

| Error | Severity |
|---|---|
| Type mismatch | Error |
| Non-exhaustive match | Error |
| Nullable access without `?.` / `??` / `when` | Error |
| Unknown identifier | Error |
| Missing `else` on `if`-expression | Error |
| Missing `_` in non-enum `when` | Error |
| `self` outside an agent | Error |
| `Ai.prompt(...)` without `as T` | Error |
| Unused variable | Warning |
| Shadowed built-in name | Warning |
| Unreachable code / catch | Warning |
| Deprecated attribute | Warning |

---

## 19. IDE Contract

Every feature is designed for tooling.

| Context | Autocomplete |
|---|---|
| After `Ai.` | `classify`, `draft`, `summarize`, etc. |
| After `Ai.classify(x, as: ` | In-scope enum types |
| After `@` inside agent body | Registered attribute names |
| After `email.` | Fields of email's structural type |
| After `when urgency { ` | Variants of the enum, marking covered/uncovered |
| After `Agent.delegate(` | In-scope agent names |
| After `using: ` | Known model strings |

**Hover:** infers and displays types, signatures, attribute docs.

**Go-to-definition:** works through types, prelude namespaces, interface implementations.

**Refactoring:** rename is variant-aware and interface-aware.

---

## 20. Formal Grammar (PEG summary, condensed)

```peg
Program     <- (Decl / Stmt)* EOF
Decl        <- Agent / TaskDecl / TypeDecl / InterfaceDecl / ExternDecl / UseStmt

Agent       <- "agent" IDENT "{" (Attribute / StateBlock / TaskDecl / OnHandler)* "}"
Attribute   <- "@" IDENT AttributeBody
AttributeBody <- STRING / Expr / Block / (IDENT "[" (Expr ",")* "]")   # flexible per handler
OnHandler   <- "on" IDENT "(" Params? ")" Block
StateBlock  <- "state" "{" (IDENT ":" Type ("=" Expr)? ","?)* "}"

TaskDecl    <- "task" IDENT "(" Params? ")" ("->" Type)? Block
InterfaceDecl <- "interface" IDENT "{" (TaskSig)* "}"
TaskSig     <- "task" IDENT "(" Params? ")" ("->" Type)?

TypeDecl    <- "type" IDENT TypeParams? "=" EnumDef                      # enum
             / "type" IDENT TypeParams? "{" FieldDef* "}"                # struct
             / "type" IDENT TypeParams? "=" Type                         # alias
EnumDef     <- EnumVariant ("|" EnumVariant)*
EnumVariant <- IDENT ("{" FieldDef* "}")?

ExternDecl  <- "extern" "task" IDENT "(" Params? ")" "->" Type "from" STRING
UseStmt     <- "use" STRING
             / "use" IDENT "from" STRING
             / "use" IDENT ("/" IDENT)+

# --- Expressions ---
Expr        <- NullCoalesce
NullCoalesce <- PipeExpr ("??" PipeExpr)?
PipeExpr    <- OrExpr ("|>" OrExpr)*
OrExpr      <- AndExpr ("or" AndExpr)*
AndExpr     <- NotExpr ("and" NotExpr)*
NotExpr     <- "not"? CompExpr
CompExpr    <- AddExpr (("==" / "!=" / "<" / ">" / "<=" / ">=") AddExpr)?
AddExpr     <- MulExpr (("+" / "-") MulExpr)*
MulExpr     <- UnaryExpr (("*" / "/" / "%") UnaryExpr)*
UnaryExpr   <- ("-" / "not")? PostfixExpr
PostfixExpr <- PrimaryExpr (FieldAccess / NullAccess / AssertAccess / Call / Index / Cast)*
FieldAccess <- "." (IDENT / INT_LIT)
NullAccess  <- "?." IDENT
AssertAccess<- "!." IDENT
Call        <- "(" Args? ")"
Index       <- "[" Expr "]"
Cast        <- "as" Type
Args        <- Arg ("," Arg)*
Arg         <- (IDENT ":")? Expr                                         # named args supported

PrimaryExpr <- Literal / "self" / IDENT / Lambda
             / TupleLit / ListLit / MapLit / SetLit
             / IfExpr / WhenExpr / TryExpr
             / "(" Expr ")"

Lambda      <- IDENT "=>" (Expr / Block)
             / "(" LambdaParams? ")" "=>" (Expr / Block)

IfExpr      <- "if" Expr Block ("else" (IfExpr / Block))?
WhenExpr    <- "when" Expr "{" WhenArm+ "}"
WhenArm     <- Pattern ("," Pattern)* ("where" Expr)? "=>" (Expr / Block)
Pattern     <- VariantPat / StructPat / TuplePat / IDENT / "_" / Literal

# --- Statements ---
Stmt        <- ReturnStmt / AssignStmt / SelfAssign / ForStmt / TryStmt / ExprStmt
AssignStmt  <- AssignTarget (":" Type)? "=" Expr
SelfAssign  <- "self" "." IDENT "=" Expr
ForStmt     <- "for" (IDENT / DestructPat) "in" Expr ("where" Expr)? Block
TryStmt     <- "try" Block CatchClause+
CatchClause <- "catch" IDENT ":" Type Block

# --- Terminals, literals, duration as before (§2.2, §9) ---
```

Notably absent: dedicated grammar for `ClassifyExpr`, `ExtractExpr`, `SummarizeExpr`, `DraftExpr`, `TranslateExpr`, `DecideExpr`, `PromptExpr`, `HttpExpr`, `SqlExpr`, `AskExpr`, `ConfirmExpr`, `NotifyStmt`, `ShowStmt`, `FetchExpr`, `SearchExpr`, `SendStmt`, `ArchiveStmt`, `RememberStmt`, `ForgetStmt`, `RecallExpr`, `EveryBlock`, `AfterStmt`, `WaitStmt`, `RetryStmt`, `ParallelExpr`, `RaceExpr`, `DelegateExpr`, `ConnectStmt`, `BroadcastStmt`, `RunStmt`, `RulesBlock`, `ConfigBlock`, `ToolsClause`, `TeamClause`, `RoleClause`, `ModelClause`, `MemoryClause`. All are ordinary function calls in the prelude or stdlib attribute handlers.

That keeps the parser small, type inference uniform (no hard-coded primitive signatures), and the IDE free of per-keyword special cases.

---

## 21. What's Next

v0.1 is the initial alpha. `keel run` accepts only the surface described in this document.

v0.2 and later are deliberately left **un-planned** until v0.1 ships and real usage reveals what to scope. See [ROADMAP.md](ROADMAP.md).

Breaking changes are expected between 0.x versions. Do not build production systems on v0.1. Play, prototype, break things, and send feedback.

---

*This specification is a living document. Syntax and semantics will evolve through alpha and beta phases.*
