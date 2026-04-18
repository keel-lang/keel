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

| Namespace | Purpose |
|---|---|
| `Ai` | LLM operations: `classify`, `extract`, `summarize`, `draft`, `translate`, `decide`, `prompt`, `embed` |
| `Io` | Human interaction: `ask`, `confirm`, `notify`, `show` |
| `Http` | HTTP client: `get`, `post`, `request` |
| `Email` | IMAP/SMTP: `fetch`, `send`, `archive` |
| `Search` | Web search providers: `web(query)` |
| `Db` | SQL: `connect`, `query`, `exec` |
| `Memory` | Persistent semantic memory: `remember`, `recall`, `forget` |
| `Schedule` | Time-based scheduling: `every`, `after`, `at`, `cron` |
| `Async` | Structured concurrency: `spawn`, `join_all`, `select`, `sleep` |
| `Control` | Control combinators: `retry`, `with_timeout`, `with_deadline` |
| `Env` | Environment and config: `get(name)`, `require(name)` |
| `Time` | Time utilities: `now`, `parse`, `format` |
| `Log` | Structured logging: `info`, `warn`, `error`, `debug` |
| `Agent` | Lifecycle: `run`, `stop`, `delegate`, `broadcast` |

`run` and `stop` are re-exported at the top level so programs can end with `run(MyAgent)` without the namespace prefix.

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
Ai.install(MyAnthropicProvider)

# Or per-agent, via a stdlib attribute
agent Specialist {
  @provider MyOllamaProvider
  @role "..."
}

# Or per-call
urgency = Ai.classify(body, as: Urgency, using: MyFinetunedProvider)
```

The language doesn't know what an LLM is. It dispatches through `LlmProvider`. Any value with `complete` and `embed` methods of the right shape works.

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
