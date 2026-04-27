# Release Notes

> **Alpha.** Keel is v0.1. Breaking changes are expected between 0.x releases.

---

## v0.1.4 — 2026-04-27
### Parser hardening — every expression-level feature now works

#### `if`-as-expression

`if` can now appear on any right-hand side, not just as a standalone statement. `else if` chains are fully supported.

```keel
label = if score > 0.8 { "high" } else if score > 0.5 { "medium" } else { "low" }
```

#### `let` type annotations validated

Declaring a type on a binding now produces a compile-time error when the value type doesn't match.

```keel
x: int = "hello"   # error: expected int, got str
y: str = "hello"   # ok
```

#### `list + list` and `list.push(item)`

Lists now support concatenation with `+` and a functional `push` that returns a new list (no mutation).

```keel
all  = ["a", "b"] + ["c", "d"]   # ["a", "b", "c", "d"]
more = all.push("e")              # ["a", "b", "c", "d", "e"]
```

#### Full string interpolation

`{…}` slots now accept any expression: method calls, binary operations, chained calls.

```keel
summary = "Items: {cart.count()}, subtotal: {price * qty}"
```

#### `@on_stop` lifecycle hook

Agents can now declare a shutdown block that runs before the agent is removed from the runtime.

```keel
agent Logger {
  @on_stop { Io.show("Logger shutting down") }
}
```

#### `Agent.delegate(target, task, args)`

Posts a named task event to another agent's mailbox; the receiving agent handles it via its `on <task>` handler.

```keel
Agent.delegate(Processor, "handle", payload)
```

#### `Search`, `Db`, `Time` stub namespaces

Calling any method on these namespaces now raises a clear `"… is planned for v0.2"` error instead of a generic crash.

---

## v0.1.3 — 2026-04-26
### Declarations reach the model

#### `@rules` injected into every LLM system prompt

```keel
agent Support {
  @role "Customer support specialist"
  @rules [
    "Never reveal internal pricing logic",
    "Escalate if the user expresses frustration 3+ times"
  ]
}
```

Rules are prepended as a bullet list in the system prompt, between the role preamble and the operation instructions. Enable `KEEL_TRACE=1` to see the full prompt for every call.

#### `Ai.summarize` — `format:` and `max:` wired

```keel
summary = Ai.summarize(article, format: bullets, max: 5, unit: sentences)
```

#### `Ai.prompt` — `response_format: json` wired

Appends "Respond with valid JSON only" to the system prompt and validates the reply.

```keel
score = Ai.prompt(system: "Rate 1–10.", user: review, response_format: json)
```

#### `Ai.extract(x, as: T)` — schema derived from struct type

```keel
type Invoice { vendor: str, amount: float, date: str }
result = Ai.extract("Invoice from ACME $99.99 on 2026-01-10", as: Invoice)
```

---

## v0.1.2 — 2026-04-19
### Internal hardening

- Bumped to **Rust edition 2024** (requires rustc ≥ 1.85).
- Runtime config decoupled from environment: `--trace` / `--log-level` / `Log.set_level` now go through typed setters, removing all `unsafe` env mutation.
- Release pipeline: Homebrew tap push now uses a short-lived GitHub App installation token instead of a long-lived PAT.

---

## v0.1.1 — 2026-04-19
### Release

- Dropped prebuilt macOS Intel binaries. Apple Silicon and Linux x86_64 are shipped; Intel Macs build from source (`cargo build --release`).

---

## v0.1.0 — First public alpha
### Everything is new

First release of the language, runtime, standard library, and tooling.

**Language highlights:**
- Statically typed, inference-first — 28 reserved keywords
- `agent` is the only concurrency primitive (actor model, serial mailbox, isolated `self` state)
- Prelude-as-stdlib — `Ai`, `Io`, `Email`, `Http`, `Schedule`, `Agent`, `Log`, … in scope everywhere, no `use` needed
- Algebraic types: simple enums, rich enums with per-variant fields, structural interfaces
- Exhaustive `when` pattern matching, duration literals (`5.minutes`), nullable syntax (`T?`, `??`, `?.`)

**Tooling:**
- `keel run` — execute, with `--trace` and `--log-level`
- `keel check` — static analysis (scope, arity, enum exhaustiveness)
- `keel fmt` — idempotent AST pretty-printer
- `keel repl` — interactive, history-aware
- `keel lsp` — diagnostics over stdio (VS Code extension included)

**Distribution:**
- GitHub releases for `aarch64-apple-darwin` + `x86_64-unknown-linux-gnu`
- `curl https://keel-lang.dev/install.sh | sh`
- `brew install keel-lang/tap/keel`
