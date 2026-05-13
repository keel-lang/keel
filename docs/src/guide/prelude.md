# The Prelude & Interfaces

> **Alpha (v0.1).** Breaking changes expected.

Keel's standard library is auto-imported into every program. You never write `use keel/ai` to get `Ai.classify`. The namespace is already in scope.

This page explains how the prelude works, why it exists, and where custom implementations fit into the design. v0.1 ships Ollama as the only LLM backend; broad runtime swapping is planned.

## Why a Prelude

- **Small core.** The compiler doesn't know about `classify`, `fetch`, or `every`. Those are library function calls that happen to always be in scope. Parser, lexer, and type checker stay free of domain-specific special cases.
- **Keyword feel.** You still write `Ai.classify(...)` without ceremony. The namespace qualifier is short; autocomplete does the work.
- **Interface boundary.** Prelude namespaces are designed around interfaces so custom LLM providers, memory stores, schedulers, or HTTP clients can be installed in a later runtime. v0.1 exposes `using:` model aliases for `Ai.*`; it does not yet expose a general provider registry.
- **No grammatical ambiguity.** Every stdlib call is an ordinary function call. No `fetch X where Y` special parsing.

## The Namespaces

Status legend: ✅ shipping · 🟡 partial · ⏳ <span class="badge badge-soon">Coming soon</span>

| Namespace | Status | Purpose |
|---|---|---|
| `Ai` | 🟡 | LLM operations: `classify`, `extract`, `summarize`, `draft`, `translate`, `decide`, `prompt` · `embed` ⏳ |
| `Io` | ✅ | Human interaction: `ask`, `confirm`, `notify`, `show` |
| `Http` | ✅ | HTTP client + server: `get`, `post`, `request`, `serve` |
| `Email` | 🟡 | IMAP/SMTP: `fetch`, `send`, `archive` |
| `File` | ✅ | Filesystem: `read`, `write`, `exists`, `list` |
| `Json` | ✅ | JSON: `parse`, `stringify` |
| `Cache` | ✅ | In-memory process-scoped cache: `set`, `get`, `delete`, `clear` |
| `Str` | ✅ | Regex & string tools: `match`, `extract`, `truncate`, `pad` |
| `Search` | 🟡 | Web search providers: `web(query)` — registered; raises "planned for v0.2" error |
| `Db` | 🟡 | SQL: `connect`, `query`, `exec` — registered; raises "planned for v0.2" error |
| `Memory` | ✅ | Per-agent key-value store: `remember`, `recall`, `forget`. Scope set by `@memory session\|persistent\|none` (default: session). Persistent mode writes `~/.keel/memory/<stem>_<hash12>/<agent>.json`. |
| `Schedule` | ✅ | Time-based scheduling: `every`, `after`, `at`, `cron`, `sleep` |
| `Async` | ✅ | Structured concurrency: `spawn`, `join_all`, `select`, `sleep` |
| `Control` | ✅ | `retry`, `with_timeout`, `with_deadline` |
| `Env` | ✅ | Environment: `get(name)`, `require(name)` |
| `Time` | ✅ | Factories: `now()`, `now(tz: name)`, `parse(str)`, `parse(str, tz: name)`. Methods on value: `dt.parts()` → map, `dt.format(as: pattern)` → `str?`. Duration literals: `500.ms` … `1.week`. Operators: `dt ± dur → dt`, `dt - dt → duration`, `<`/`>` comparison. Naive strings rejected — use RFC 3339 or `tz:`. |
| `Log` | ✅ | Structured logging: `info`, `warn`, `error`, `debug`, plus `set_level`, `level`. Threshold default is `info`; raise via `--log-level debug`, `KEEL_LOG_LEVEL=debug`, or `Log.set_level("debug")` at runtime. |
| `Agent` | ✅ | `run`, `stop`, `send`, `delegate`, `broadcast` |

`run` and `stop` are re-exported at the top level so programs can end with `run(MyAgent)` without the namespace prefix.

> **v0.1 scope.** Anything marked ⏳ is reserved in the grammar but not yet wired. 🟡 means partial: something works, but not everything. `Search` and `Db` are registered and raise clear "planned for v0.2" errors; `Ai.embed` returns an empty list. Track the full status in [ROADMAP.md](../../ROADMAP.md).

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

The planned custom-provider flow looks like this:

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

The language doesn't know what an LLM is. The design dispatches through `LlmProvider`; once provider installation is wired, any value with `complete` and `embed` methods of the right shape can satisfy it.

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
