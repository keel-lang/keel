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
| `File` | ✅ | Filesystem: `read`, `write`, `exists`, `list`, `mkdir`, `remove`, `copy`, `move`, `glob`, `mktemp` |
| `Shell` | ✅ | Subprocess bridge: execute shell commands via `/bin/sh -c` and capture stdout/stderr. Requires `@tools [Shell]`. |
| `String value methods` | ✅ | Regex and formatting directly on string values: matches, extract, find_all, sub, truncate, pad |
| `Json` | 🟡 | JSON: `parse`, `stringify` |
| `Cache` | 🟡 | In-memory process-scoped cache: `set`, `get`, `delete`, `clear` |
| `Search` | 🟡 | Web search providers: `web(query)` — registered; raises "planned for v0.2" error |
| `Db` | 🟡 | SQL: `connect`, `query`, `exec` — registered; raises "planned for v0.2" error |
| `Memory` | ✅ | Per-agent key-value store: `remember`, `recall`, `forget`. Scope set by `@memory session\|persistent\|none` (default: session). Persistent mode writes `~/.keel/memory/<stem>_<hash12>/<agent>.json`. |
| `Schedule` | ✅ | Time-based scheduling: `every`, `after`, `at`, `cron`, `sleep` |
| `Async` | ✅ | Structured concurrency: `spawn`, `join_all`, `select`, `sleep` |
| `Control` | ✅ | `retry`, `with_timeout`, `with_deadline` |
| `Env` | ✅ | Environment: `get(name)`, `require(name)` |
| `Time` | 🟡 | Factories: `now()`, `now(tz: name)`, `parse(str)`, `parse(str, tz: name)`, `epoch_ms()` → `int`. Methods on value: `dt.parts()` → map, `dt.format(as: pattern)` → `str?`. Duration literals: `500.ms` … `1.week`. Operators: `dt ± dur → dt`, `dt - dt → duration`, `<`/`>` comparison. Naive strings rejected — use RFC 3339 or `tz:`. |
| `Math` | ✅ | Transcendentals: `sqrt`, `pow`, `exp`, `log` (natural), `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`. Constants: `PI()`, `E()`. All return `float`. Domain errors raise. |
| `Random` | ✅ | Pseudo-random generation: `float()`, `int(min:, max:)`, `bool()`. Use `Crypto` for security-sensitive randomness. |
| `Uuid` | ✅ | UUID values: `v4`, `v7`, deterministic `v5`, `parse`, `uuid()` alias, and value methods `version`, `format`, `to_str`. |
| `Crypto` | ✅ | Cryptographic primitives: fixed safe SHA-2 hash/HMAC methods, `token`, `random_bytes`. |
| `Log` | ✅ | Structured logging: `info`, `warn`, `error`, `debug`, plus `set_level`, `level`. Threshold default is `info`; raise via `--log-level debug`, `KEEL_LOG_LEVEL=debug`, or `Log.set_level("debug")` at runtime. |
| `Agent` | ✅ | `run`, `stop`, `send`, `delegate`, `broadcast` |

`run` and `stop` are re-exported at the top level so programs can end with `run(MyAgent)` without the namespace prefix.

## Free Functions

A small set of functions live directly in the global scope — no namespace prefix needed.

| Function | Signature | Returns | Notes |
|---|---|---|---|
| `run(agent)` | `(agent) -> none` | `none` | Start an agent |
| `stop(agent)` | `(agent) -> none` | `none` | Stop an agent |
| `uuid()` | `() -> Uuid` | `Uuid` | Alias for `Uuid.v4()` |
| `min(...)` | `(...items: T, by: (T -> any)? = none) -> T?` | `T?` | Minimum; `none` on empty |
| `max(...)` | `(...items: T, by: (T -> any)? = none) -> T?` | `T?` | Maximum; `none` on empty |

`min` and `max` accept any number of positional arguments, an optional `by:` key-selector, and return `none` when called with no items.

```keel
min(3, 1, 4)                           # 1
max("banana", "apple", "cherry")       # cherry

scores = [4, 9, 2, 7]
min(scores)                            # 2 — single list auto-spread
max(...scores, 99)                     # 99 — explicit spread

people = [{name: "Alice", age: 30}, {name: "Bob", age: 25}]
min(people, by: p => p.age)            # {name: "Bob", age: 25}
max(people, by: p => p.age)            # {name: "Alice", age: 30}
```

## Random

`Random` produces non-cryptographic pseudo-random values for simulation, sampling, games, and tests where security is not involved. Use `Crypto` for tokens, secrets, signatures, or key material.

| Function | Signature | Returns | Notes |
|---|---|---|---|
| `Random.float()` | `() -> float` | `float` | Uniform in `[0.0, 1.0)` |
| `Random.int(min:, max:)` | `(min: int, max: int) -> int` | `int` | Inclusive range; raises if `min > max` |
| `Random.bool()` | `() -> bool` | `bool` | 50/50 boolean |

```keel
roll = Random.int(min: 1, max: 6)
sample = Random.float()
enabled = Random.bool()
```

## Uuid

`Uuid` is a distinct value type, not a `str`. It displays and interpolates as a lowercase hyphenated UUID, and it can be converted explicitly with `.to_str()`.

| Function | Signature | Returns | Notes |
|---|---|---|---|
| `uuid()` | `() -> Uuid` | `Uuid` | Alias for `Uuid.v4()` |
| `Uuid.v4()` | `() -> Uuid` | `Uuid` | Random UUID |
| `Uuid.v7()` | `() -> Uuid` | `Uuid` | Time-ordered UUID |
| `Uuid.v5(ns:, name:)` | `(ns: Uuid, name: str) -> Uuid` | `Uuid` | Deterministic UUID from namespace + name |
| `Uuid.parse(s)` | `(str) -> Uuid?` | `Uuid?` | `none` if invalid |

Namespace constants `Uuid.DNS`, `Uuid.URL`, `Uuid.OID`, and `Uuid.X500` are available for `Uuid.v5`.

| Method | Returns | Notes |
|---|---|---|
| `.version()` | `int` | UUID version number |
| `.format(as:)` | `str` | `"hyphenated"`, `"simple"`, or `"urn"` |
| `.to_str()` | `str` | Lowercase hyphenated string |

```keel
id: Uuid = uuid()
trace = Uuid.v7()
site = Uuid.v5(ns: Uuid.DNS, name: "keel-lang.dev")
parsed = Uuid.parse("f47ac10b-58cc-4372-a567-0e02b2c3d479")
simple = id.format(as: "simple")
```

## Crypto

`Crypto` provides security-grade primitives backed by the operating system CSPRNG. It is distinct from `Random`; use `Crypto` for tokens, secrets, signatures, digests, and other security-sensitive work.

| Function | Signature | Returns | Notes |
|---|---|---|---|
| `Crypto.sha224(data)` | `(str) -> str` | `str` | SHA-224 hex digest |
| `Crypto.sha256(data)` | `(str) -> str` | `str` | SHA-256 hex digest |
| `Crypto.sha384(data)` | `(str) -> str` | `str` | SHA-384 hex digest |
| `Crypto.sha512(data)` | `(str) -> str` | `str` | SHA-512 hex digest |
| `Crypto.sha512_224(data)` | `(str) -> str` | `str` | SHA-512/224 hex digest |
| `Crypto.sha512_256(data)` | `(str) -> str` | `str` | SHA-512/256 hex digest |
| `Crypto.hmac_sha224(data, key:)` | `(str, key: str) -> str` | `str` | HMAC-SHA-224 hex signature |
| `Crypto.hmac_sha256(data, key:)` | `(str, key: str) -> str` | `str` | HMAC-SHA-256 hex signature |
| `Crypto.hmac_sha384(data, key:)` | `(str, key: str) -> str` | `str` | HMAC-SHA-384 hex signature |
| `Crypto.hmac_sha512(data, key:)` | `(str, key: str) -> str` | `str` | HMAC-SHA-512 hex signature |
| `Crypto.hmac_sha512_224(data, key:)` | `(str, key: str) -> str` | `str` | HMAC-SHA-512/224 hex signature |
| `Crypto.hmac_sha512_256(data, key:)` | `(str, key: str) -> str` | `str` | HMAC-SHA-512/256 hex signature |
| `Crypto.token(bytes: 32)` | `(bytes: int = 32) -> str` | `str` | CSPRNG-backed hex token |
| `Crypto.random_bytes(n)` | `(int) -> list[int]` | `list[int]` | CSPRNG bytes as integers `0..255` |

```keel
digest = Crypto.sha256("hello")
wide = Crypto.sha384("hello")
sig = Crypto.hmac_sha256("message", key: secret)
token = Crypto.token(bytes: 32)
bytes = Crypto.random_bytes(16)
```

`Crypto` intentionally exposes fixed safe SHA-2 methods only. Legacy hashes such as MD5 and SHA-1, and string-selected hash algorithms, are not exposed.

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
