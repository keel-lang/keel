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
  - [x] Nullable safety enforcement (`T?` not assignable to `T`; `!` unwraps; `??` coalesces)
  - [x] Return-type matching for `return expr` against declared `-> T`
  - [x] Struct/map subtyping checks (missing fields caught; extra fields allowed)
  - [~] Generic type parameter inference — list/string method return types inferred; `map[K, V]` still opaque
- [~] **Parser corners.** Known limits surfaced by the inbox_assistant example:
  - [x] `if`-as-expression on the RHS of a binding (`x = if cond { a } else { b }`)
  - [x] Type annotations on `let` bindings (`x: T = ...`) — checker validates declared vs inferred type
  - [ ] Nested `"..."` inside `{interp}` without escaping (escaped `\"` already works)
  - [x] String interpolation now routes through the real expression parser (function calls, binary ops, etc.)
  - [x] `!` postfix unwrap operator
  - [x] `list + list` / `list.push` concatenation

#### Agent model
- [x] Agent declaration + `run(Agent)` / `Agent.run` / `Agent.stop`
- [x] `@on_start` block executed at boot (the only lifecycle hook currently wired)
- [x] Per-agent serial mailbox; `on <event>` handlers dispatched via the mpsc event loop
- [x] `Agent.send(target, data, event:)` — posts an event to another agent's mailbox
- [x] `self.` state read/write from handlers and tasks
- [x] `Agent.delegate(target, task, args)` — posts a named task event to another agent's mailbox
- [ ] `Agent.broadcast(team, data)` — referenced in docs, not registered; no `@team` handling either

#### Attributes

Two tiers — core attributes drive language behavior, stdlib attributes are plugin handlers.

| Attribute | Tier | Status | Notes |
|---|---|---|---|
| `@model "ollama:..."` | core | [x] | Read by `Ai.*` to pick the Ollama model |
| `@role "..."` | core | [x] | Prepended as `"You are {role}.\n\n..."` to every `Ai.*` system prompt; the LLM gets the agent identity on every call |
| `@on_start { ... }` | stdlib | [x] | Block runs once when the agent starts |
| `@on_stop { ... }` | stdlib | [x] | Block runs once when the agent stops |
| `@tools [...]` | stdlib | [ ] | Parsed, no capability gating yet |
| `@memory persistent\|session\|none` | stdlib | [ ] | Parsed, no effect (Memory namespace is itself a stub — see below) |
| `@rules [...]` | stdlib | [x] | Injected as a bullet list into the system prompt of every `Ai.*` call inside the agent |
| `@limits { ... }` | stdlib | [ ] | Parsed as struct literal, no enforcement (no cost/token/timeout caps) |
| `@team [...]` | stdlib | [ ] | Parsed, no team routing |
| `@provider MyProvider` | stdlib | [ ] | Parsed, no per-agent LLM-provider swap |

#### Prelude namespaces

| Namespace | Status | Implemented ops | Gaps |
|---|---|---|---|
| `Ai` | [~] | `classify` (with `considering:`), `summarize` (with `format:` and `max:` params), `draft`, `extract` (with `as: T` struct derivation), `translate`, `decide`, `prompt` (with `response_format:`) | `embed` returns `[]`; `Ai.install(provider)` not registered; `decide` returns a plain `{choice, reason, confidence: 1.0}` map instead of a `Decision[T]` type (all wired as of v0.1.3) |
| `Io` | [x] | `notify`, `show`, `ask`, `confirm` | — |
| `Schedule` | [x] | `every`, `after`, `at` (RFC 3339 / ISO 8601), `sleep` | — |
| `Email` | [~] | `fetch` (IMAP), `send` (SMTP) via env vars | `archive` is a no-op placeholder (no IMAP folder move) |
| `Http` | [x] | `get`, `post`, `request` (via reqwest) | — |
| `Env` | [x] | `get`, `require` | — |
| `Log` | [x] | `info`, `warn`, `error`, `debug`, `set_level`, `level`. Threshold controlled via `KEEL_LOG_LEVEL`, `--log-level`, or `Log.set_level("...")` at runtime (default `info`). | — |
| `Agent` | [~] | `run`, `stop`, `send`, `delegate` | `broadcast` missing |
| `Memory` | [ ] | — | `remember` / `recall` / `forget` all no-op stubs (no vector store, no embeddings) |
| `Control` | [ ] | — | `retry` / `with_timeout` / `with_deadline` all no-op stubs |
| `Async` | [~] | `sleep` | `spawn` / `join_all` / `select` all no-op stubs — structured concurrency not yet real |
| `Search` | [~] | — | Registered; all methods raise a clear "planned for v0.2" error |
| `Db` | [~] | — | Registered; all methods raise a clear "planned for v0.2" error |
| `Time` | [~] | — | Registered; all methods raise a clear "planned for v0.2" error (use `now` keyword) |

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

#### Dependencies
- [x] Rust edition 2024 (min rustc 1.85)
- [x] Patch/minor `cargo update` is applied continuously; no version-pin drift tolerated.
- [ ] **Major bumps deferred to v0.2** (each is its own mini-project, kept out of alpha so regressions are bisectable):
  - `chumsky 0.9 → 1.0` — API rewrite; the parser in `src/parser.rs` would need reworking. Low payoff during alpha.
  - `imap 2 → 3` — also resolves the `imap-proto` future-incompat warning `cargo` emits today.
  - `colored 2 → 3`, `lettre 0.11 → 0.12` (and similar) — ergonomic bumps, batched with the two above.

**Deferred post-v0.1 with rationale:**
- `keel build` bytecode compiler. The tree-walking interpreter is fast enough for alpha workloads (~8ms cold start), and a real VM has to re-solve async dispatch, closure capture across event-loop boundaries, and runtime-pluggable namespaces — costly without matching user payoff. Revisit when there's a concrete motivator (LLVM/WASM backend, embeddable runtime).
- Pluggable LLM provider registry. v0.1 ships with Ollama only; adding a second provider is the forcing function that justifies the registry plumbing.
- `Memory` / `Control` / `Async` beyond their current stubs. Each needs its own interface design (vector store, retry policy, task graph). Punted to v0.2 so we don't paint ourselves into a corner during alpha.

**Docs ↔ implementation reconciliation (short-term cleanup):**
- [x] Tag every unimplemented or partial stdlib page in `docs/src/guide/*.md` with a `Coming soon` badge and a `> Status:` callout that links back to this roadmap
- [x] Mark `Search` / `Db` / `Time` as `⏳` in the `docs/src/guide/prelude.md` namespace table with an explicit v0.1-scope callout
- [x] Register `Search` / `Db` / `Time` as stub namespaces so calls raise a clear "planned for v0.2" error instead of a generic "unknown method"

### Release
- [x] Release workflow builds macOS (Apple Silicon) + Linux x86_64 tarballs, computes SHA-256s, and writes `Formula/keel.rb` into the `keel-lang/homebrew-tap` repo. Auth uses a short-lived installation token minted from the `keel-release-bot` GitHub App (installed on both repos; secrets `TAP_APP_ID` + `TAP_APP_PRIVATE_KEY` on this repo). Intel Macs build from source — prebuilt Intel binaries are not shipped.
- [x] `install.sh` fetches the latest tag; served by Pages at `https://keel-lang.dev/install.sh`
- [x] Homebrew install via `brew install keel-lang/tap/keel`
- [x] First v0.1.0 tag cut
- [ ] v0.1.0 release validated end-to-end (tarballs downloadable, `brew install keel-lang/tap/keel` works, `install.sh` one-liner works on both targets)
- [ ] **Manual-trigger release workflows** — two `workflow_dispatch` jobs in `.github/workflows/`:
  - `release-patch.yml` — reads the latest `v*` tag, increments the third digit (`v0.1.N` → `v0.1.N+1`), creates and pushes the tag, which fires the existing release pipeline.
  - `release-minor.yml` — increments the second digit and resets the third (`v0.1.N` → `v0.2.0`), same downstream effect.
  - Neither writes to `main`; both only push a tag. Run from the Actions UI so the "when" stays a deliberate human decision.

---

## v0.1.5 — Type checker hardening

**Theme:** Close the gap between what the spec declares and what the type checker actually enforces. After this release, every type annotation a user writes is checked — nullables, return types, struct fields, generics.

**Status:** shipped.

### Goals

1. **[x] Nullable safety at call sites** — `T?` is now a distinct, enforced type. Passing a `str?` where a `str` is expected is a type error. The `!` unwrap operator strips the nullable in the checker; `??` coalesces to a non-nullable value. `NullAssert` (`!`) now correctly returns the inner type.

2. **[x] Return-type matching** — `return expr` inside a task declared with `-> T` is now checked against the declared type. Bare `return` with no value skips the check.

3. **[x] Struct / map field checks** — Named struct types (e.g., `type Foo { a: int, b: str }`) now resolve to `Ty::Struct(fields)` in the checker. Passing a struct literal where a named type is expected catches missing fields; extra fields are allowed (structural subtyping).

4. **[x] Generic type parameter inference** — List method return types are now inferred: `.push` / `.filter` return the same `list[T]`; `.len()` returns `int`; `.first()` / `.last()` return `T?`; `.contains()` returns `bool`. `list + list` also infers the element type. String methods (`.len`, `.upper`, `.split`, `.contains`, etc.) are similarly typed.

5. **[ ] Nested string literals inside `{interp}`** — `"outer {"inner"}"` is still a parse error. The fix requires a context-aware string lexer that understands interp nesting depth. Deferred to a follow-on patch; escaped inner strings (`\"`) already work via the backslash escape path.

6. **[x] `NullAssert` type-strips nullable** — Bonus fix: `expr!` now returns the unwrapped inner type instead of preserving `T?`.

---

## v0.1.6 — Wiring & Ergonomics

**Theme:** Close the remaining small gaps that are documented as working but aren't — plus the one parser corner that still bites users.

**Status:** shipped.

### Language / Parser

- [x] **Nested string literals inside `{interp}`** — `"outer {"inner"}"` now parses. The lexer scans the slot body with brace depth tracking and recursively handles nested `"..."`.
- [x] **`map[K, V]` type inference** — `map.get(k)` returns `V?`, `keys()` returns `list[K]`, `values()` returns `list[V]`, `count`/`len`/`size` return `int`, `is_empty`/`contains`/`has` return `bool`. Same methods now exist at the runtime level too. The checker also accepts a `{k: v, ...}` literal where `map[str, V]` is expected.

### Agent model

- [x] **`Agent.broadcast(team, data)`** — registered in the runtime dispatcher. Fans out to every live agent whose `@team [...]` attribute contains the target team name.

### Stdlib wiring

- [x] **`Control.retry(n, fn)`** — invokes `fn` up to `n` times; surfaces the last error if all attempts fail.
- [x] **`Control.with_timeout(duration, fn)`** — wraps `fn` in `tokio::time::timeout`; raises `TimeoutError` on expiry.
- [x] **`Control.with_deadline(datetime, fn)`** — parses an RFC 3339 / ISO 8601 timestamp and times out at the absolute deadline; raises `DeadlineError`.
- [x] **`Email.archive(message)`** — IMAP UID MOVE with COPY+\Deleted+EXPUNGE fallback. Target folder honours `IMAP_ARCHIVE_FOLDER` (default `Archive`). `Email.fetch` now returns each message's UID under the `uid` key.

### Tests

- [x] **Integration tests** — `tests/integration_tests.rs` adds 11 tests under "v0.1.6 — wiring & ergonomics" covering nested interp (single & double layer), `Control.retry`, `with_timeout` (fast path + abort), `Agent.broadcast` (team filter), `Email.archive` (graceful no-op), `map.get` / `map.keys`, and LSP hover (let-binding + namespace).
- [x] **Example programs** — `examples/retry_on_failure.keel`, `examples/broadcast_team.keel`, `examples/nested_interp.keel`, `examples/map_methods.keel`. All four are wired into `examples_all_parse` and run under `KEEL_LLM=mock`.

### Developer tooling

- [x] **LSP hover** — `textDocument/hover` returns the inferred type of the identifier under the cursor. The checker exposes `type_at(text, offset)` which walks declarations to gather name → type bindings and special-cases prelude namespaces and built-in types.
- [x] **Manual-trigger release workflows** — `release-patch.yml` and `release-minor.yml` are in `.github/workflows/`. Each reads the latest `v*` tag, bumps the appropriate digit, and pushes the new tag without writing to `main`.

---

## v0.1.7 — Structured Concurrency & Agent Constraints

**Theme:** Make multi-agent programs composable with real structured concurrency, give agents the ability to embed content semantically, and enforce the agent attribute constraints that have been parsed-but-ignored since v0.1.0.

**Status:** planned.

### Stdlib — new namespaces

- [ ] **`File.read(path)`** — return the file contents as `str`; raise `FileError` if not found.
- [ ] **`File.write(path, content)`** — write `str` to a file; create intermediate directories if needed.
- [ ] **`File.exists(path)`** — return `bool`.
- [ ] **`File.list(dir)`** — return `list[str]` of entry names in the directory.
- [ ] **`Json.parse(str)`** — deserialize a JSON string into a Keel map / list / scalar value; raise `JsonError` on invalid input.
- [ ] **`Json.stringify(value)`** — serialize a Keel value to a JSON string.
- [ ] **`Schedule.cron(expr, fn)`** — schedule `fn` using a standard 5-field cron expression (e.g. `"0 9 * * 1-5"`). Currently not registered despite being documented.

### Stdlib — structured concurrency

- [ ] **`Async.spawn(fn)`** — spawn `fn` as an independent Tokio task; returns a handle that can be awaited.
- [ ] **`Async.join_all(handles)`** — await a list of task handles; returns a list of results in the same order.
- [ ] **`Async.select(handles)`** — resolve to the first handle that completes; cancel the rest.

### Agent attributes — enforcement

- [ ] **`@tools [...]` capability gating** — restrict which prelude namespaces (e.g. `Http`, `Email`) are accessible inside the agent body. Calls to unlisted namespaces raise a runtime `CapabilityError`.
- [ ] **`@limits { max_tokens, max_cost, timeout }` enforcement** — apply the declared caps to every `Ai.*` call made inside the agent. `timeout` wraps the call via `Control.with_timeout` (shipping in v0.1.6); `max_tokens` / `max_cost` are passed as Ollama request parameters where supported.

### Developer tooling

- [ ] **LSP completion** — suggest prelude namespace methods, declared identifiers, and enum variants at the cursor position. Builds on the symbol table already populated by the type-checker pass.

### Tests

- [ ] **Integration tests** — end-to-end `keel run` tests covering: `File` read/write/exists/list, `Json` parse/stringify, `Schedule.cron`, `Async.spawn` / `join_all` / `select`, `@tools` capability error, `@limits` timeout enforcement. Every item shipped in v0.1.7 has at least one integration test.
- [ ] **Example programs** — add a `.keel` file in `examples/` showcasing each new feature (e.g. `file_processing.keel`, `parallel_agents.keel`, `cron_digest.keel`).

---

## v0.1.8 — Reactive Agents & Text Processing

**Theme:** Let agents listen for external events over HTTP, give programs rich string and regex tools, and add a lightweight shared cache. Completes the LSP feature set planned for v0.1.

**Status:** planned.

### Stdlib — reactive HTTP

- [ ] **`Http.serve(port, handler)`** — start an HTTP listener on `port`; invoke `handler(request)` for each incoming request. `request` is a map with `method`, `path`, `headers`, `body`. `handler` returns a map with `status` and `body`. Enables agents that react to webhooks rather than only polling.

### Stdlib — string processing

- [ ] **`Str.match(text, pattern)`** — return `bool`; true if `pattern` (regex) matches anywhere in `text`.
- [ ] **`Str.extract(text, pattern)`** — return the first capture group as `str?`; `none` if no match.
- [ ] **`Str.truncate(text, max)`** — truncate to `max` characters, appending `"…"` if cut.
- [ ] **`Str.pad(text, width, char?)`** — left-pad to `width` with `char` (default `" "`).

### Stdlib — shared cache

- [ ] **`Cache.set(key, value, ttl?)`** — store a value in the process-scoped in-memory cache; optional TTL as a duration literal.
- [ ] **`Cache.get(key)`** — return `str?` (`none` if missing or expired).
- [ ] **`Cache.delete(key)`** — evict a key.
- [ ] **`Cache.clear()`** — flush all entries.

> `Cache` is process-scoped and not persisted across restarts. It fills the gap between `self.` (per-agent) and `Memory` (persistent vector store, planned for v0.2).

### Developer tooling

- [ ] **LSP go-to-definition** — jump to the declaration of any identifier, task, agent, or type under the cursor.
- [ ] **LSP rename** — rename a symbol across the file.

### Tests

- [ ] **Integration tests** — end-to-end `keel run` tests covering: `Http.serve` (loopback request), `Str` regex ops, `Cache` set/get/ttl expiry. Every item shipped in v0.1.8 has at least one integration test.
- [ ] **Example programs** — add a `.keel` file in `examples/` showcasing each new feature (e.g. `webhook_agent.keel`, `text_pipeline.keel`).

---

## v0.1.9 — Tooling

**Theme:** Make the day-to-day development experience first-class — a polished VS Code extension, a linter, a Tree-sitter grammar for other editors, and sharper error output from `keel check`.

**Status:** planned.

### VS Code extension (`keel-lang/vscode-keel`)

> The extension currently lives in `editors/vscode/` in this repo. It ships a TextMate grammar, `language-configuration.json`, and an LSP config. v0.1.9 moves it to its own repo and completes it into a publishable extension.

- [ ] **Scaffold `keel-lang/vscode-keel`** — move `editors/vscode/` into the new repo; set up its own CI, `package.json` scripts, and `CHANGELOG.md`. Remove `editors/` from this repo.
- [ ] **Snippets** — `agent`, `task`, `type`, `interface`, `on`, `@on_start` scaffolds with tab stops.
- [ ] **Format-on-save** — register `keel fmt` as the VS Code document formatter (`editor.formatOnSave` respects it).
- [ ] **Run / Check commands** — `Keel: Run File` and `Keel: Check File` in the command palette; both shell out to `keel run` / `keel check` and pipe output to a dedicated output channel.
- [ ] **Extension icon** — use the brand logo from `brand/`.
- [ ] **Marketplace publish** — build and publish the `.vsix` to the VS Code Marketplace under the `keel-lang` publisher. Publish pipeline is independent of the language release workflow.

### Tree-sitter grammar (`keel-lang/tree-sitter-keel`)

- [ ] **Scaffold `keel-lang/tree-sitter-keel`** — standalone repo following Tree-sitter conventions (`grammar.js`, `src/`, `bindings/`). Enables syntax highlighting and basic navigation in Neovim, Helix, Zed, and any editor with Tree-sitter support.

### Linter (`keel lint`)

- [ ] **`keel lint <file>`** — style and best-practice checks beyond type errors:
  - Unused variables and task arguments.
  - Tasks declared but never called or registered.
  - `Ai.*` calls outside an agent (no `@role` / `@model` context).
  - Agent state fields (`self.x`) written but never read, or read before first write.
- [ ] **`--fix` flag** — auto-apply safe single-line fixes (e.g. remove unused `let` bindings).

### `keel check` error quality

- [ ] **Source spans in all diagnostics** — every error and warning from `keel check` includes a line:column pointer and an underlined excerpt, matching the style of `rustc` / `tsc`.
- [ ] **Suggestion hints** — where the fix is unambiguous (e.g. missing `!` on a nullable, wrong argument count), append a `hint:` line with the suggested correction.

### Tests

- [ ] **Integration tests** — at least one test per lint rule: unused variable, uncalled task, `Ai.*` outside agent, unread state field.
- [ ] **Example programs** — add `.keel` files in `examples/` that demonstrate correct patterns the linter validates against (doubles as living documentation of best practices).

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
