# The Prelude & Interfaces

> **Alpha (v0.1).** Breaking changes expected.

Keel's standard library is auto-imported into every program. You never write `use keel/ai` to get `Ai.classify`. The namespace is already in scope.

This page explains how the prelude works, why it exists, and how to swap in your own implementations.

## Why a Prelude

- **Small core.** The compiler doesn't know about `classify`, `fetch`, or `every`. Those are library function calls that happen to always be in scope. Parser, lexer, and type checker stay free of domain-specific special cases.
- **Keyword feel.** You still write `Ai.classify(...)` without ceremony. The namespace qualifier is short; autocomplete does the work.
- **Swappable implementations.** Every prelude function dispatches through an **interface**. Users install custom LLM providers, memory stores, schedulers, or HTTP clients at startup.
- **No grammatical ambiguity.** Every stdlib call is an ordinary function call. No `fetch X where Y` special parsing.

## The Namespaces

Status legend: ✅ shipping · 🟡 partial · ⏳ <span class="badge badge-soon">Coming soon</span>

| Namespace | Status | Purpose |
|---|---|---|
| `Ai` | 🟡 | LLM operations: `classify`, `extract`, `summarize`, `draft`, `translate`, `decide`, `prompt` · `embed` ⏳ |
| `Io` | ✅ | Human interaction: `ask`, `confirm`, `notify`, `show` |
| `Http` | ✅ | HTTP client: `get`, `post`, `request` |
| `Email` | 🟡 | IMAP/SMTP: `fetch`, `send` · `archive` ⏳ |
| `Search` | ⏳ | Web search providers: `web(query)` |
| `Db` | ⏳ | SQL: `connect`, `query`, `exec` |
| `Memory` | ⏳ | Persistent semantic memory: `remember`, `recall`, `forget` (stubbed — no vector store yet) |
| `Schedule` | ✅ | Time-based scheduling: `every`, `after`, `at`, `sleep` · `cron` ⏳ |
| `Async` | 🟡 | `sleep` shipping · `spawn`, `join_all`, `select` ⏳ |
| `Control` | ⏳ | `retry`, `with_timeout`, `with_deadline` (stubbed) |
| `Env` | ✅ | Environment: `get(name)`, `require(name)` |
| `Time` | ⏳ | Time utilities: `now`, `parse`, `format` (use the `now` keyword for now) |
| `Log` | ✅ | Structured logging: `info`, `warn`, `error`, `debug`, plus `set_level`, `level`. Threshold default is `info`; raise via `--log-level debug`, `KEEL_LOG_LEVEL=debug`, or `Log.set_level("debug")` at runtime. |
| `Agent` | 🟡 | `run`, `stop`, `send` · `delegate`, `broadcast` ⏳ |

`run` and `stop` are re-exported at the top level so programs can end with `run(MyAgent)` without the namespace prefix.

> **v0.1 scope.** Anything marked ⏳ is reserved in the grammar but not yet wired — calls will either return `none` (stubs) or raise an "unknown method" error (missing namespaces). Track the full status in [ROADMAP.md](../../ROADMAP.md).

## Interfaces

An **interface** declares a set of method signatures. Any type with matching methods structurally satisfies the interface — no explicit `implements`.

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

Every prelude namespace dispatches through one or more interfaces:

| Namespace | Interface(s) |
|---|---|
| `Ai` | `LlmProvider` |
| `Memory` | `VectorStore`, `Embedder` |
| `Http` | `HttpClient` |
| `Email` | `EmailTransport` |
| `Search` | `SearchProvider` |
| `Log` | `Tracer` |

## Swapping Implementations

Install a custom implementation at startup:

```keel
# Use a custom LLM provider for the whole program
Ai.install(MyCustomProvider)                 # Coming soon

# Or per-agent, via a stdlib attribute
agent Specialist {
  @provider MyFinetunedProvider              # Coming soon
  @role "..."
}

# Or per-call
urgency = Ai.classify(body, as: Urgency, using: "smart")
```

The language doesn't know what an LLM is. It dispatches through `LlmProvider`. Any value with `complete` and `embed` methods of the right shape works.

> **Status:** `using:` is wired in v0.1 (resolves via `KEEL_MODEL_*` env vars and Ollama tags). `Ai.install(...)` and `@provider` <span class="badge badge-soon">Coming soon</span> — v0.1 ships with Ollama only.

## Shadowing the Prelude

`Ai`, `Io`, and other namespaces are identifiers, not keywords. A program can shadow them:

```keel
Ai = my_custom_module     # legal, though usually a bad idea
```

The compiler will warn on shadowing a built-in name. Use deliberately.

## Adding Your Own Prelude Module

Not yet in v0.1. A future release will expose this via the package system: a library declares itself "prelude-eligible," and users opt in once in `keel.config` to include it in their prelude globally.

## Namespaces, Not Keywords

Operations like `classify`, `draft`, `every`, `fetch`, `ask`, `confirm`, and `send` are prelude functions on the `Ai`, `Io`, `Email`, `Schedule`, and `Http` namespaces — not reserved words. The core language stays small (~27 keywords), and the stdlib is a normal Rust crate that anyone can extend or replace.
