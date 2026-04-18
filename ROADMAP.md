# Keel Roadmap

> Keel is in **alpha** (v0.1). Expect breaking changes. Do not build production systems yet.

---

## Principles

1. **Small core, deep stdlib.** Everything that can be a library is one. The core earns its keep through the type system, the compiler, or the actor runtime.
2. **Rust from day one.** Single-binary distribution, async via Tokio, no runtime dependencies on other language ecosystems.
3. **Prelude-as-stdlib.** Users never write `use keel/ai`. Namespaces like `Ai`, `Io`, `Schedule`, `Http` are auto-imported. Implementation is swappable via interfaces.
4. **No silent fallbacks.** Configuration mistakes surface as errors at startup, not as silent mock responses at runtime.

---

## v0.1 — Alpha

**Goal:** a runnable language where agents can be declared, type-checked, and executed end-to-end with a real LLM provider.

### Design
- [x] Reserved keyword set: 27 words (see [SPEC.md §10](SPEC.md))
- [x] Prelude + interfaces + attributes ([SPEC.md](SPEC.md))
- [x] Documentation: installation, language guide, stdlib namespace pages, examples

### Implementation

Legend: **[x]** complete · **[~]** partial (works, but with caveats below) · **[ ]** stub or missing.

#### Core compiler
- [x] Lexer
- [x] Parser: `@attributes`, `interface` / `extern` / `use` declarations, named arguments, `as T` cast, rich enum variants, triple-quoted strings, duration literals
- [x] Interpreter: namespace dispatch, agent lifecycle with `@on_start`, `self.` state, pattern matching (simple + rich enums), closures, async execution
- [x] Examples: 11 `.keel` programs parse and execute end-to-end
- [~] **Type checker.** Implemented: undefined identifiers, exhaustive `when`, `self.` outside agents, `if` / `for` condition types, arg-count checks, basic enum inference, rich-variant field checks.
  - [ ] Nullable safety enforcement (`T?` not distinguished from `T` at call sites)
  - [ ] Full return-type matching against declared `-> T`
  - [ ] Struct/map subtyping checks
  - [ ] Generic type parameter inference (`list[T]`, `map[K, V]`)

#### Agent model
- [x] Agent declaration + `run(Agent)` / `Agent.run` / `Agent.stop`
- [x] `@on_start` block executed at boot (the only lifecycle hook currently wired)
- [x] Per-agent serial mailbox; `on <event>` handlers dispatched via the mpsc event loop
- [x] `Agent.send(target, data, event:)` — posts an event to another agent's mailbox
- [x] `self.` state read/write from handlers and tasks
- [ ] `Agent.delegate(target, task, args)` — referenced in docs, not registered in the runtime
- [ ] `Agent.broadcast(team, data)` — referenced in docs, not registered; no `@team` handling either

#### Attributes

Two tiers — core attributes drive language behavior, stdlib attributes are plugin handlers.

| Attribute | Tier | Status | Notes |
|---|---|---|---|
| `@model "ollama:..."` | core | [x] | Read by `Ai.*` to pick the Ollama model |
| `@role "..."` | core | [x] | Prepended as `"You are {role}.\n\n..."` to every `Ai.*` system prompt; the LLM gets the agent identity on every call |
| `@on_start { ... }` | stdlib | [x] | Block runs once when the agent starts |
| `@on_stop { ... }` | stdlib | [ ] | Parsed, never executed |
| `@tools [...]` | stdlib | [ ] | Parsed, no capability gating yet |
| `@memory persistent\|session\|none` | stdlib | [ ] | Parsed, no effect (Memory namespace is itself a stub — see below) |
| `@rules [...]` | stdlib | [ ] | Parsed, never forwarded to the LLM |
| `@limits { ... }` | stdlib | [ ] | Parsed as struct literal, no enforcement (no cost/token/timeout caps) |
| `@team [...]` | stdlib | [ ] | Parsed, no team routing |
| `@provider MyProvider` | stdlib | [ ] | Parsed, no per-agent LLM-provider swap |

#### Prelude namespaces

| Namespace | Status | Implemented ops | Gaps |
|---|---|---|---|
| `Ai` | [~] | `classify`, `summarize`, `draft`, `extract`, `translate`, `decide`, `prompt` | `embed` returns `[]`; `Ai.install(provider)` not registered; `classify(considering:)` accepted but not sent to LLM; `summarize(format: bullets)` ignored; `decide` returns a plain `{choice, reason, confidence: 1.0}` map instead of a `Decision[T]` type; `prompt(response_format: json)` ignored |
| `Io` | [x] | `notify`, `show`, `ask`, `confirm` | — |
| `Schedule` | [x] | `every`, `after`, `at` (RFC 3339 / ISO 8601), `sleep` | — |
| `Email` | [~] | `fetch` (IMAP), `send` (SMTP) via env vars | `archive` is a no-op placeholder (no IMAP folder move) |
| `Http` | [x] | `get`, `post`, `request` (via reqwest) | — |
| `Env` | [x] | `get`, `require` | — |
| `Log` | [x] | `info`, `warn`, `error`, `debug` | — |
| `Agent` | [~] | `run`, `stop`, `send` | `delegate`, `broadcast` missing |
| `Memory` | [ ] | — | `remember` / `recall` / `forget` all no-op stubs (no vector store, no embeddings) |
| `Control` | [ ] | — | `retry` / `with_timeout` / `with_deadline` all no-op stubs |
| `Async` | [~] | `sleep` | `spawn` / `join_all` / `select` all no-op stubs — structured concurrency not yet real |
| `Search` | [ ] | — | Documented in `docs/src/guide/prelude.md`; **not registered in the runtime** (calling `Search.web(...)` errors) |
| `Db` | [ ] | — | Documented; **not registered** — no SQL client |
| `Time` | [ ] | — | Documented; **not registered** — use `now` keyword instead |

#### LLM providers
- [x] Ollama backend wired into every `Ai.*` call
- [ ] Provider-swapping story (`Ai.install(MyProvider)` / `@provider MyProvider`) — `LlmProvider` interface exists in docs, no pluggable runtime registry yet

#### CLI
- [x] `keel run` / `keel check` / `keel init`
- [x] `keel fmt` — idempotent round-trip against the AST
- [x] `keel repl` — multi-line input, persistent environment
- [~] `keel lsp` — publish-diagnostics (lex + parse + type-check) over tower-lsp
  - [ ] Hover
  - [ ] Completion
  - [ ] Go-to-definition / rename
- [ ] `keel build` — bytecode compiler + VM

**Deferred post-v0.1 with rationale:**
- `keel build` bytecode compiler. The tree-walking interpreter is fast enough for alpha workloads (~8ms cold start), and a real VM has to re-solve async dispatch, closure capture across event-loop boundaries, and runtime-pluggable namespaces — costly without matching user payoff. Revisit when there's a concrete motivator (LLVM/WASM backend, embeddable runtime).
- Pluggable LLM provider registry. v0.1 ships with Ollama only; adding a second provider is the forcing function that justifies the registry plumbing.
- `Memory` / `Control` / `Async` beyond their current stubs. Each needs its own interface design (vector store, retry policy, task graph). Punted to v0.2 so we don't paint ourselves into a corner during alpha.

**Docs ↔ implementation reconciliation (short-term cleanup):**
- [x] Tag every unimplemented or partial stdlib page in `docs/src/guide/*.md` with a `Coming soon` badge and a `> Status:` callout that links back to this roadmap
- [x] Mark `Search` / `Db` / `Time` as `⏳` in the `docs/src/guide/prelude.md` namespace table with an explicit v0.1-scope callout
- [ ] Register `Search` / `Db` / `Time` as stub namespaces so calls raise a clear "planned for v0.2" error instead of a generic "unknown method"

### Release
- [x] Release workflow builds macOS (arm + x86) + Linux x86 tarballs, computes SHA-256s, and writes `Formula/keel.rb` into the `keel-lang/homebrew-tap` repo (needs `HOMEBREW_TAP_TOKEN` secret on this repo with `contents: write` on the tap)
- [x] `install.sh` fetches the latest tag; served by Pages at `https://keel-lang.dev/install.sh`
- [x] Homebrew install via `brew install keel-lang/tap/keel`
- [ ] First v0.1.0 tag cut + release validated end-to-end

---

## Beyond v0.1

v0.2 and later milestones are **deliberately un-planned** until v0.1 ships. Pre-planning scope before the core is landed would pre-commit us to things we haven't yet felt the weight of.

- **v1.0** is the first API-stable release. Semver begins at v1.0. Scope will be defined only after real usage feedback from v0.1.

One ship at a time.

---

## How to Get Involved

- **Read the spec.** If something reads wrong, open an issue.
- **Try an example.** Find the gap between spec and implementation; report it.
- **Write an interface implementation.** Custom LLM provider, memory store, scheduler backend — those are exactly the right things to prototype right now.
- **Do not build production systems on v0.1.**
