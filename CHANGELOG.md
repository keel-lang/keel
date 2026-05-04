# Changelog

All notable changes to Keel.

> **Alpha.** Keel is v0.1. Breaking changes are expected between 0.x releases. Do not build production systems on 0.x.

> **Doc-update rule.** Any feature or spec change — added, updated, or removed — must update the docs in the same release. At minimum: `docs/src/release-notes.md` plus every guide page in `docs/src/guide/` (and `docs/src/examples/`, `docs/src/cli/`, `docs/src/config/` where applicable) that the change touches. `SPEC.md` and `ROADMAP.md` are part of this rule. A release is not shipped until `mdbook build` runs clean over the updated pages.

---

## [0.1.10] — 2026-05-03

### Memory namespace

`Memory.remember`, `Memory.recall`, and `Memory.forget` are now real operations. They were no-op stubs since v0.1.0.

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

#### `@memory` — scope selection

Three modes, selected with the `@memory` attribute:

- **`session`** (default when the attribute is omitted) — in-process store. Values persist across handler calls within a single program run and are cleared on restart.
- **`persistent`** — file-backed JSON store at `~/.keel/memory/<agent-name>.json`. Values survive restarts.
- **`none`** — disables `Memory.*` entirely for the agent. Any call raises `CapabilityError`.

```keel
agent Bot {
  @memory persistent   # remembers across runs

  @on_start {
    last = Memory.recall("last_user")
    if last != none {
      Io.show("Welcome back, {last}!")
    }
    Memory.remember("last_user", "Alice")
    stop(self)
  }
}
```

The persistent store is a simple JSON key-value file — one file per agent name. There is no vector search in v0.1; semantic recall is planned for v0.2 via the `VectorStore` interface.

---

## [0.1.9] — 2026-05-03

### `keel init` — three fixes

**In-place initialization.** Running `keel init` with no argument now scaffolds directly into the current directory instead of creating a subdirectory named after the parent folder.

```bash
mkdir myproject && cd myproject
keel init          # writes main.keel here, not myproject/myproject/
keel run main.keel
```

**Absolute/relative path argument.** `keel init /some/path/myproject` now uses the basename (`myproject`) as the project and agent name. Previously the full path was used, producing an invalid agent name.

```bash
keel init /tmp/mybot   # agent MyBot { ... }  ✓  (was: agent /tmp/mybot { ... })
```

**Runnable template.** The generated `main.keel` now prints and exits cleanly instead of scheduling a timer. The old template used `Schedule.every` which kept the process alive forever without any visible output.

```keel
agent MyBot {
  @role "Describe what this agent does"

  @on_start {
    Io.show("Hello from MyBot!")
    stop(self)
  }
}

run(MyBot)
```

### `stop(self)` — self-stop from inside an agent

Bare `self` now resolves to an `AgentRef` for the current agent, so an agent can stop itself without repeating its own name.

```keel
agent Worker {
  @on_start {
    Io.show("done")
    stop(self)     # was: stop(Worker)
  }
}
```

`self` as an expression is valid anywhere inside an agent body where an `AgentRef` is expected. It errors at runtime if used outside an agent context.

### Tooling — Linter & Sharper Errors

#### `keel lint` command

New command that checks a Keel program for style and best-practice issues beyond type correctness.

```bash
keel lint agent.keel
keel lint --fix agent.keel
```

Four rules:

**Unused variable** — warns when a local binding is assigned but never read.

```keel
agent Noisy {
  @on_start {
    temp = "debug"        # ⚠ unused — prefix with _temp to suppress
    Io.show("hello")
  }
}
```

**Uncalled task** — warns when a `task` is declared but never invoked anywhere in the program.

```keel
task helper(x: int) -> int { x + 1 }   # ⚠ never called

agent Main {
  @on_start { Io.show("hi") }
}
```

**`Ai.*` outside agent** — warns when `Ai.classify`, `Ai.summarize`, etc. are called from a plain task or top-level statement without `@role` / `@model` context.

```keel
task process(text: str) -> str {
  Ai.summarize(text)   # ⚠ Ai.* called outside agent context
}
```

**State written, never read** — warns when an agent state field is assigned but `self.field` is never referenced.

```keel
agent Counter {
  state { total: int = 0 }
  @on_start {
    self.total = self.total + 1   # ⚠ self.total is written but never read
  }
}
```

`--fix` removes unused variable assignments automatically. Prefix a variable with `_` to suppress the warning:

```keel
_debug = expensive_call()   # suppressed — no warning
```

#### `keel check` — source spans in all diagnostics

Every error from `keel check` now includes a line:column pointer and an underlined source excerpt rendered via `miette`. Previously, many errors printed only a message with no location. Arity errors now list the expected parameter names as a correction hint:

```
  × Type error
   ╭─[agent.keel:8:5]
 7 │   @on_start {
 8 │     greet()
   ·     ───┬───
   ·        ╰── task `greet` takes 1 argument(s), got 0 — expected: name
 9 │   }
   ╰────
```

#### New example: `examples/lint_best_practices.keel`

Demonstrates variable reuse, state tracking, and task calls — the patterns the linter validates as correct.

---

## [0.1.8] — 2026-04-30

### Reactive Agents & Text Processing

This release adds HTTP webhook handling, in-memory caching, regex and string tools, and LSP go-to-definition and rename.

#### Cache namespace

Process-scoped in-memory cache with optional TTL:

```keel
Cache.set("key", "value", ttl: 60.seconds)
val = Cache.get("key")          // returns value or none
Cache.delete("key")
Cache.clear()
```

#### Str namespace

Regex matching and string manipulation:

```keel
if Str.match(text, "\\d+") {
  number = Str.extract(text, "(\\d+)")
}
text = Str.truncate("hello world", 5)      // "hello…"
text = Str.pad("42", 5)                    // "   42"
text = Str.pad("42", 5, char: "0")         // "00042"
```

#### Http.serve

HTTP listener for webhooks and reactive agents:

```keel
Http.serve(8080, (request) => {
  method = request["method"]
  path = request["path"]
  body = request["body"]
  { status: 200, body: "OK" }
})
```

The handler receives a `request` map with `method`, `path`, `body` and returns a response map with `status`, `body`.

#### LSP go-to-definition & rename

Jump to task/agent/type declarations and rename symbols across a file in VS Code and other LSP clients.

---

## [0.1.7] — 2026-04-30

### Structured Concurrency & Agent Constraints

This release adds file I/O, JSON processing, async task spawning, cron scheduling, and agent capability restrictions.

#### File namespace

Read, write, and list files on disk:

```keel
File.write("data.txt", "Hello Keel")
content = File.read("data.txt")

if File.exists("data.txt") {
  Io.show(content)
}

entries = File.list("data/")
```

`File.read` raises `FileError` if the file doesn't exist; `File.write` creates intermediate directories automatically.

#### Json namespace

Parse JSON strings and serialize Keel values:

```keel
data = Json.parse("{\"name\": \"Alice\", \"age\": 30}")
name = data["name"]

user = { name: "Bob", age: 25 }
json_str = Json.stringify(user)
```

#### Schedule.cron

Schedule recurring tasks using 5-field cron expressions:

```keel
Schedule.cron("0 9 * * 1-5", () => {
  Io.show("Morning digest")
})

Schedule.cron("*/5 * * * *", () => {
  Io.show("Every 5 minutes")
})
```

Supports standard cron syntax: minute, hour, day, month, weekday.

#### Async.spawn / join_all / select

Spawn independent Tokio tasks and await their completion:

```keel
task1 = Async.spawn(() => {
  result1 = Http.get("https://api1.example.com")
  Io.show(result1)
})

task2 = Async.spawn(() => {
  result2 = Http.get("https://api2.example.com")
  Io.show(result2)
})

handles = [task1, task2]
results = Async.join_all(handles)
Io.show("All tasks done")
```

`Async.select` returns the first handle to complete (race condition).

#### @tools capability gating

Restrict namespace access within an agent:

```keel
agent RestrictedAgent {
  @tools [Io, File]

  @on_start {
    Io.show("Allowed")
    File.write("x.txt", "Allowed")
    # Http.get would raise CapabilityError
  }
}
```

If no `@tools` attribute is specified, all namespaces are accessible.

#### @limits enforcement infrastructure

Extract and enforce per-agent resource limits:

```keel
agent LimitedAgent {
  @limits { timeout: 30s, max_tokens: 1000, max_cost: 5.0 }

  @on_start {
    response = Ai.prompt("...")
  }
}
```

The `timeout` limit wraps calls via `Control.with_timeout`. The `max_tokens` and `max_cost` are passed to Ollama where supported.

#### LSP completion

`textDocument/completion` now suggests prelude namespaces, method names, and keywords. Triggered on "." or manual invocation.

#### New example programs

- `file_processing.keel` — File read/write/exists/list
- `json_processing.keel` — JSON parse/stringify
- `cron_schedule.keel` — Cron expression scheduling
- `parallel_execution.keel` — Async task spawning and joining
- `capability_gating.keel` — @tools attribute enforcement

---

## [0.1.6] — 2026-04-28

### Wiring & ergonomics

This release closes long-standing gaps in the standard library and the
language server. Every primitive that was a stub in 0.1.5 now does what
its name promises.

#### Nested string literals inside `{interp}`

The lexer used to terminate a string at the first `"` it saw, even one
hiding inside a `{...}` slot. The lexer now scans the slot body with brace
depth tracking and recursively handles nested `"..."`, so this works:

```keel
agent A {
  @on_start {
    name = "world"
    Io.show("hi {"there {name}"}")  # prints: hi there world
  }
}
```

#### `Control.retry` / `with_timeout` / `with_deadline`

The three control-flow combinators now have real implementations.

```keel
# Re-invoke until success or the budget is spent.
result = Control.retry(5, () => Ai.prompt(system: "...", user: "..."))

# Abort if the closure runs past the duration.
fast = Control.with_timeout(2.seconds, () => slow_call())

# Abort once the absolute deadline has passed.
done = Control.with_deadline("2026-12-31T23:59:00Z", () => long_task())
```

`with_timeout` and `with_deadline` raise `TimeoutError` / `DeadlineError`
on expiry; `retry` surfaces the last attempt's error if every attempt
fails.

#### `Agent.broadcast(team, data)`

Tag agents with `@team [...]` and dispatch a single event to every
member of a named team:

```keel
agent Alpha { @team ["frontline"]  on alert(m: str) { ... } }
agent Beta  { @team ["frontline"]  on alert(m: str) { ... } }
agent Gamma { @team ["backoffice"] on alert(m: str) { ... } }

agent Coordinator {
  @on_start {
    Agent.run(Alpha); Agent.run(Beta); Agent.run(Gamma)
    Agent.broadcast("frontline", "incident", event: "alert")
    # Alpha and Beta fire; Gamma does not.
  }
}
```

#### `Email.archive(message)`

`Email.archive` now performs an IMAP folder move (`UID MOVE`, falling back
to `COPY` + `\Deleted` + `EXPUNGE` for servers without the MOVE
extension). The destination folder is `Archive` by default and can be
overridden with the `IMAP_ARCHIVE_FOLDER` env var. `Email.fetch` now
returns each message's UID under the `uid` key so `archive` can target
the right message.

#### `map[K, V]` method inference

Map literals support the common operations on both the type checker and
runtime side:

| Expression | Inferred type |
|---|---|
| `map.get(k)` | `V?` |
| `map.keys()` | `list[K]` |
| `map.values()` | `list[V]` |
| `map.len()` / `map.count()` / `map.size()` | `int` |
| `map.is_empty()` | `bool` |
| `map.contains(k)` / `map.has(k)` | `bool` |

The checker also accepts a `{k: v, ...}` struct literal in any position
that expects `map[str, V]`, so the same surface syntax serves both
struct and map construction.

#### LSP hover

`textDocument/hover` returns the inferred type of the identifier under
the cursor, looking through `let`-bindings, function parameters, agent
state fields, and prelude namespaces (`Io`, `Ai`, `Control`, …).

#### Examples

`examples/retry_on_failure.keel`, `examples/broadcast_team.keel`,
`examples/nested_interp.keel`, and `examples/map_methods.keel` exercise
the new primitives end-to-end and run under `KEEL_LLM=mock`.

---

## [0.1.5] — 2026-04-27

### Type checker hardening

#### Nullable safety at call sites

`T?` is now a distinct, enforced type. Passing a nullable value where a
non-nullable is expected is a type error.

```keel
task t() {
  x: str = Env.get("KEY")   # error: expected str, got str?
  y: str = Env.get("KEY")!  # ok — ! unwraps, throws NullError if none
  z: str = Env.get("KEY") ?? "default"  # ok — ?? coalesces
}
```

`NullAssert` (`!`) now returns the unwrapped inner type in the checker, so
`x: str = some_nullable!` is accepted without a false positive.

#### Return-type matching

`return expr` inside a task with a declared `-> T` is now verified against
that type.

```keel
task greet() -> str {
  return 42   # error: return value: expected str, got int
}
```

Bare `return` (no value) is still accepted inside any task.

#### Struct field checks

Named struct types now resolve to their field list in the checker. A struct
literal passed where a named type is expected is checked for missing fields.
Extra fields are allowed (structural subtyping).

```keel
type Person { name: str, age: int }

task t() {
  p: Person = { name: "Alice" }         # error: missing field `age`
  q: Person = { name: "Bob", age: 30 }  # ok
  r: Person = { name: "Eve", age: 25, extra: true }  # ok — extra fields allowed
}
```

#### Generic list and string method type inference

List and string method calls now return typed results instead of `unknown`:

| Expression | Inferred type |
|---|---|
| `list.push(x)` / `list.filter(fn)` | `list[T]` (same element type) |
| `list.len()` / `list.count()` | `int` |
| `list.contains(x)` / `list.is_empty()` | `bool` |
| `list.first()` / `list.last()` | `T?` |
| `list.map(fn)` | `list[unknown]` (lambda return deferred) |
| `listA + listB` | `list[T]` |
| `str.len()` / `str.count()` | `int` |
| `str.upper()` / `str.lower()` / `str.trim()` / `str.replace()` | `str` |
| `str.split(sep)` | `list[str]` |
| `str.contains(s)` / `str.starts_with(s)` / `str.ends_with(s)` | `bool` |

```keel
task t() {
  items = ["a", "b", "c"]
  n: int   = items.len()           # ok
  more     = items.push("d")       # list[str]
  short    = items.filter(x => true)  # list[str]
  for s in short { Io.notify(s) }  # s: str
}
```

---

## [0.1.4] — 2026-04-27

### Parser hardening — every expression-level feature now works

#### `if`-as-expression

`if` can now appear on any right-hand side, not just as a standalone statement.

```keel
label = if score > 0.8 { "high" } else if score > 0.5 { "medium" } else { "low" }
```

#### `let` type annotations validated

Declaring a type on a `let` binding now produces a type error when the
inferred type of the value doesn't match. The check is skipped for
user-defined named types that the checker can't fully resolve yet.

```keel
x: int = "hello"   # type error: `x` expected int, got str
y: str = "hello"   # ok
```

#### `!` postfix unwrap — retroactive

`expr!` (null-assert; throws `NullError` if the value is `none`) was already
fully implemented across the parser, interpreter, and type checker. The ROADMAP
marker was stale — no code change, just updated docs.

#### `list + list` and `list.push(item)`

Lists support concatenation with `+` and a functional `push` that returns a
new list.

```keel
base  = ["a", "b"]
extra = ["c", "d"]
all   = base + extra               # ["a", "b", "c", "d"]
more  = all.push("e")              # ["a", "b", "c", "d", "e"]
```

`push` does not mutate in place — reassign the result: `items = items.push(x)`.

#### Full string interpolation

String interpolation slots (`{...}`) now run through the real expression
parser. Function calls, binary expressions, and method chains all work.

```keel
summary = "Items: {cart.count()}, total: {price * qty}"
```

#### `@on_stop` lifecycle hook

Agents with `@on_stop { ... }` now have their block executed before the agent
is removed from the runtime.

```keel
agent Logger {
    @on_stop { Log.info("Logger shutting down") }
}
# Agent.stop(Logger) → prints "Logger shutting down"
```

#### `Agent.delegate(target, task, args)`

Posts a named task event to another agent's mailbox. The receiving agent
handles it via its `on <task>` handler.

```keel
Agent.delegate(Processor, "handle", payload)
# Processor's `on handle` fires with payload
```

#### `Search`, `Db`, `Time` stub namespaces

Calling any method on these namespaces now raises a clear
`"... is planned for v0.2 and is not available in v0.1."` error instead of the
generic `"Unknown namespace"` panic.

```keel
Search.web("keel language")
# Error: Search is planned for v0.2 and is not available in v0.1.
```

---

## [0.1.3] — 2026-04-26

### Declarations reach the model

#### `@rules` injected into every LLM system prompt

`@rules` was previously parsed but silently dropped. Every `Ai.*` call made
inside an agent with `@rules` now prepends the rules as a bullet list in the
system prompt, between the role preamble and the operation-specific instructions.

```keel
agent Support {
    @role "Customer support specialist"
    @rules [
        "Never reveal internal pricing logic",
        "Escalate if the user expresses frustration 3+ times"
    ]

    @on_start {
        reply = Ai.draft("Welcome message", tone: "friendly")
    }
}
```

System prompt shape sent to the LLM:

```
You are Customer support specialist.

Rules:
- Never reveal internal pricing logic
- Escalate if the user expresses frustration 3+ times

You are a text drafter. Draft the following with a friendly tone.
```

Enable `KEEL_TRACE=1` to see the full system prompt for every call.

#### `Ai.summarize` — `format:` and `max:` wired

`format:` and `max:` were previously parsed but ignored. They now emit
directives in the system prompt:

```keel
summary = Ai.summarize(article, format: bullets, max: 5, unit: sentences)
```

- `format: bullets` → "Format your response as a bulleted list."
- `format: prose` → "Format your response as flowing prose."
- `max: N` with `unit: sentences` → "Use at most N sentences."
- `max: N` without a unit → "Use at most N items."

#### `Ai.prompt` — `response_format: json` wired

`response_format: json` was previously ignored. It now:
1. Appends "Respond with valid JSON only. No prose, no markdown fences." to the system prompt.
2. Validates the LLM reply — if the reply cannot be parsed as JSON, a runtime error is raised.

```keel
score = Ai.prompt(
    system: "Rate sentiment on a 1-10 scale.",
    user: "Text: {review}",
    response_format: json
)
```

#### `Ai.extract(x, as: T)` — schema derived from declared struct type

`as: T` was previously parsed but fell through to an empty schema. A
`struct_types` registry is now populated during program evaluation, and
`Ai.extract(x, as: T)` derives the field schema from it.

```keel
type Invoice { vendor: str, amount: float, date: str }

agent Extractor {
    @on_start {
        result = Ai.extract("Invoice from ACME $99.99 on 2026-01-10", as: Invoice)
    }
}
```

The outgoing system prompt now contains `vendor`, `amount`, and `date` as
extraction targets. Using `as: UnknownType` raises a runtime error with an
actionable message.

---

## [0.1.2] — 2026-04-19

Internal hardening. No user-facing language or stdlib changes.

### Build

- **Rust edition 2024** — bumped from 2021. Minimum supported rustc is now 1.85. Contributors building from source need a recent toolchain.
- **Runtime config decoupled from the environment block.** Previously, `--trace` / `--log-level <lvl>` / `Log.set_level("...")` mutated `KEEL_TRACE` and `KEEL_LOG_LEVEL` at runtime. Edition 2024 made `std::env::set_var` unsafe, and the underlying pattern was always a data race against concurrent env reads on POSIX. The env vars remain the startup input (seeded once into process-global atomics); runtime mutation now goes through typed setters instead, so no `unsafe` is required.
- Dependency patch bumps via `cargo update` (tokio 1.52.0 → 1.52.1 and transitive).

### Release infrastructure

- **Homebrew tap push now uses a GitHub App installation token** (`keel-release-bot`) instead of a long-lived Personal Access Token. Token is minted per-run with a 1-hour lifetime, scoped to `contents:write` on `keel-lang/homebrew-tap` only, and not tied to any user account.

---

## [0.1.1] — 2026-04-19

### Release

- **Dropped prebuilt macOS Intel binaries.** Release tarballs now cover only macOS Apple Silicon (`aarch64-apple-darwin`) and Linux x86_64 (`x86_64-unknown-linux-gnu`). Intel Mac users can still build from source via `cargo build --release`.

---

## [0.1.0] — Alpha

First public release. The language, standard library, and tooling are all new.

### Language

- **Small core.** 28 reserved keywords total. Everything else (AI calls, scheduling, I/O, HTTP, email) is a stdlib function, not syntax.
- **Prelude-as-stdlib.** Namespaces `Ai`, `Io`, `Email`, `Http`, `Schedule`, `Memory`, `Async`, `Control`, `Env`, `Log`, `Agent` are in scope in every program with no `use` needed. (Documented namespaces `Search`, `Db`, `Time` are planned but not yet registered — see [ROADMAP](ROADMAP.md).)
- **Interfaces.** Structural protocol declarations (`interface LlmProvider { ... }`); any type with matching methods satisfies the interface — no explicit `implements`.
- **Attributes.** `@role`, `@model` are core attributes. `@on_start`, `@on_stop`, `@tools`, `@memory`, `@rules`, `@limits`, `@team`, `@provider` are stdlib attributes, parsed by the grammar. Wiring status: `@role`, `@model`, `@on_start`, and `@rules` are executed at runtime. `@role` is prepended as `"You are {role}.\n\n..."` to every `Ai.*` system prompt; `@rules` injects a bullet list of rules between the role preamble and the operation-specific instructions (wired in v0.1.3). The remaining stdlib attributes are parsed but have no runtime effect yet (tracked in [ROADMAP](ROADMAP.md)).
- **Named arguments** on calls: `Ai.classify(body, as: Urgency, fallback: Urgency.medium)`.
- **Algebraic data types.** Simple enums (`type Urgency = low | medium | high`) and rich enums with per-variant fields (`type Action = reply { to: str, tone: str } | archive`). Construction: `Action.reply { to: "x", tone: "y" }`. Destructuring: `when a { reply { to, tone } => ... }`.
- **Type aliases.** `type Timestamp = datetime`.
- **Triple-quoted strings.** `"""..."""` preserves newlines and internal quotes; still supports `{expr}` interpolation.
- **Nullable syntax.** `T?` is a distinct type; `?.`, `??`, `fallback:` are the handling tools. *Full nullable-safety enforcement at call sites is still in progress in the type checker — see [ROADMAP](ROADMAP.md).*
- **Exhaustive pattern matching.** `when` on a simple enum must cover every variant or use `_`; compile-time error if missing.
- **`as` cast.** `Ai.prompt(...) as MyType` narrows the dynamic return shape.
- **Duration literals.** `5.minutes`, `2.hours`, `1.day`. `Schedule.*` and `Async.sleep` accept them directly.

### Runtime

- **Tree-walking interpreter on Tokio.** ~8ms cold start. Agents are the sole concurrency primitive: per-agent serial mailbox, isolated mutable state via `self.`, handlers run one at a time.
- **Event loop.** `mpsc`-driven. Scheduled ticks and cross-agent messages flow through it; `Ctrl+C` and `KEEL_ONESHOT=1` both exit cleanly.
- **Recurring `Schedule.every`.** Spawns a tokio interval that posts `FireClosure` events at each tick; `Schedule.after` is the one-shot variant; `Schedule.at(datetime_str, fn)` accepts RFC 3339 / ISO 8601.
- **Message dispatch.** `Agent.send(target, data)` posts a `Dispatch` event that fires the target agent's `on <event>` handler in its own `self` context.
- **Rich enum runtime values.** `Value::EnumVariant(type, variant, Option<fields>)`; pattern destructuring binds fields by name.
- **Prelude dispatch.** Every namespace resolves through a runtime registry (`Ai.classify` is a method lookup on a registered namespace value). Per-call model override via `using:` is wired; provider-level swapping (`Ai.install`, `@provider`) is planned for v0.2.

### Standard library

- **`Ai`** — Ollama backend only in v0.1 (via `LlmProvider` interface).
  - Wired: `Ai.classify(input, as: T, fallback: V)`, `Ai.summarize(text, in: N, unit: sentences, fallback: ...)`, `Ai.draft(prompt, tone: …, guidance: …, max_length: …)`, `Ai.extract(from: …, schema: {field: "type"})`, `Ai.translate(text, to: …)`, `Ai.decide(input, options: […])`, `Ai.prompt(system: …, user: …) as T`.
  - Partial: `Ai.classify(..., considering: {...})` — argument parsed but not forwarded to the LLM yet; `Ai.decide` returns a plain `{choice, reason, confidence: 1.0}` map instead of a typed `Decision[T]`. (As of v0.1.3: `Ai.summarize(format:, max:)`, `Ai.prompt(response_format: json)`, and `Ai.extract(as: T)` are all fully wired.)
  - Stubs: `Ai.embed` returns `[]`.
  - Missing: `Ai.install(provider)` is not registered in the runtime.
  - Model resolution: `using:` arg ≻ enclosing agent's `@model` ≻ `KEEL_OLLAMA_MODEL` catch-all. `KEEL_MODEL_<ALIAS>` maps custom aliases to Ollama tags.
  - `KEEL_LLM=mock` short-circuits every call — used by the integration test suite and `keel run` in offline mode.
- **`Io`** — terminal-backed `ask`, `confirm`, `notify`, `show`. Fully wired.
- **`Http`** — `reqwest`-backed. `Http.get`, `Http.post`, `Http.request` return a `{status, body, headers, is_ok}` map. Fully wired.
- **`Email`** — real IMAP fetch + SMTP send via env vars `IMAP_HOST`, `SMTP_HOST`, `EMAIL_USER`, `EMAIL_PASS`. Gracefully degrades to empty list / no-op when credentials aren't set. `Email.archive` is a no-op placeholder in v0.1 (IMAP folder-move not yet implemented).
- **`Schedule`** — `every`, `after`, `at` (RFC 3339 / ISO 8601), `sleep`. The `at:` calendar-alignment argument on `Schedule.every` is parsed but not enforced; `Schedule.cron` is not registered. Tracked in [ROADMAP](ROADMAP.md).
- **`Env`** — `Env.get(name)` returns `str?`, `Env.require(name)` errors if unset. Fully wired.
- **`Log`** — `info`, `warn`, `error`, `debug` print to stderr. Level gated: threshold comes from `KEEL_LOG_LEVEL` / `--log-level <level>` / `Log.set_level("...")`; default is `info` so `Log.debug` is silent until raised. `Log.level()` returns the active threshold as a string.
- **`Agent`** — `Agent.run(A)` / `Agent.stop(A)` / `Agent.send(target, data, event:)` wired. `Agent.delegate` and `Agent.broadcast` are referenced in the docs but not yet registered in the runtime.
- **`Memory`** — `remember`, `recall`, `forget` are no-op stubs in v0.1. No vector-store backend yet.
- **`Control`** — `retry`, `with_timeout`, `with_deadline` are no-op stubs in v0.1.
- **`Async`** — `sleep` is wired. `spawn`, `join_all`, `select` are no-op stubs; real structured concurrency is planned for v0.2.
- **`Search`**, **`Db`**, **`Time`** — documented in the prelude guide but **not registered in the runtime**. Calling these namespaces raises an "unknown method" error; they are planned for v0.2.

### Tooling

- `keel run <file>` — execute. Global flags: `--trace` (`KEEL_TRACE=1` — surface LLM call metadata, input previews, per-call results, provider banner) and `--log-level <debug|info|warn|error>` (`KEEL_LOG_LEVEL=<level>` — threshold for the program's `Log.*` calls, default `info`). `Ctrl+C` exits immediately regardless of what the runtime is blocked on, with exit code `130`.
- `keel check <file>` — static analysis: undefined identifiers, `self` outside an agent, non-exhaustive `when` on simple enums, missing `_` on non-enum `when`, `if` condition / `for` iterator types, task argument arity, basic `Ai.classify` enum inference, rich-variant field checks. **Not yet enforced:** full nullable safety at call sites, return-type matching against declared `-> T`, generic parameter inference.
- `keel repl` — interactive; persists bindings and declarations across prompts; brace-balance-aware multi-line input; `~/.keel_history`.
- `keel fmt <file>` — idempotent AST pretty-printer. Two-space indent, multi-line lambda block bodies, automatic string-key quoting for map keys with spaces.
- `keel lsp` — language server over stdio (tower-lsp). Publishes lex/parse/type-check diagnostics on `did_open` / `did_change`. Hover/completion are placeholders.
- `keel build` — **deferred post-v0.1.** The tree-walking interpreter is the supported execution path. A real bytecode VM has to re-solve async dispatch, closure capture across event-loop re-entry, and runtime-pluggable namespace dispatch; none of those have a matching user payoff yet.

### Distribution

- **GitHub release workflow** builds two targets on tag push: `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`. Computes SHA-256 per tarball and embeds them in the release notes. Intel Macs are not shipped as prebuilt binaries — users on that platform build from source (`cargo build --release`).
- **`install.sh`** served at `https://keel-lang.dev/install.sh` — one-liner that fetches the latest release for your OS + arch.
- **Homebrew tap** (`keel-lang/homebrew-tap`): the release workflow writes `Formula/keel.rb` with the new version + URLs + SHAs on every tag. `brew install keel-lang/tap/keel`.
- **GitHub Pages** deploys the mdBook documentation to `keel-lang.dev` on every push to `main`, with `install.sh` and `uninstall.sh` copied into the root.

### Documentation

- `SPEC.md` — authoritative language specification, v0.1.
- `ROADMAP.md` — v0.1 checklist + deferred items.
- `VISION.md` — design principles and target audience.
- `docs/src/` (mdBook, 29 pages): getting started, language guide, stdlib namespace reference, CLI reference, configuration, examples. Partial / missing features are flagged in-page with a "Coming soon" badge and cross-linked to the roadmap.
- 15 example `.keel` programs in `examples/` covering scheduling, agents, AI primitives, message dispatch, HTTP, rich enums, multi-agent preview.

### Tests

101 green across: lexer (39), parser (24), type checker (18), formatter (5), LSP (5), integration (10 end-to-end program runs via `keel run`).

### Versioning

- Semver is **not** respected between 0.x minor versions.
- 0.1.x — breaking changes allowed in patch releases.
- 0.2+ scope is deliberately un-planned until 0.1 lands in the wild.
- 1.0 — first API-stable release.
