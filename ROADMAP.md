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
  - [x] Generic type parameter inference — list/string method return types inferred; `map[K, V]` fully typed (shipped v0.1.5–v0.1.6)
- [x] **Parser corners** — all known gaps closed:
  - [x] `if`-as-expression on the RHS of a binding (`x = if cond { a } else { b }`)
  - [x] Type annotations on `let` bindings (`x: T = ...`) — checker validates declared vs inferred type
  - [x] Nested `"..."` inside `{interp}` — lexer tracks brace depth and handles recursively (v0.1.6)
  - [x] String interpolation now routes through the real expression parser (function calls, binary ops, etc.)
  - [x] `!` postfix unwrap operator
  - [x] `list + list` / `list.push` concatenation

#### Agent model
- [x] Agent declaration + `run(Agent)` / `Agent.run` / `Agent.stop`
- [x] `@on_start` block executed at boot
- [x] `@on_stop` block executed on agent stop (v0.1.4)
- [x] Per-agent serial mailbox; `on <event>` handlers dispatched via the mpsc event loop
- [x] `Agent.send(target, data, event:)` — posts an event to another agent's mailbox
- [x] `self.` state read/write from handlers and tasks
- [x] `Agent.delegate(target, task, args)` — posts a named task event to another agent's mailbox (v0.1.4)
- [x] `Agent.broadcast(team, data)` — fans out to every live agent in the named `@team` (v0.1.6)

#### Attributes

Two tiers — core attributes drive language behavior, stdlib attributes are plugin handlers.

| Attribute | Tier | Status | Notes |
|---|---|---|---|
| `@model "ollama:..."` | core | [x] | Read by `Ai.*` to pick the Ollama model |
| `@role "..."` | core | [x] | Prepended as `"You are {role}.\n\n..."` to every `Ai.*` system prompt |
| `@on_start { ... }` | stdlib | [x] | Block runs once when the agent starts |
| `@on_stop { ... }` | stdlib | [x] | Block runs once when the agent stops (v0.1.4) |
| `@tools [...]` | stdlib | [x] | Capability gating — unlisted namespaces raise `CapabilityError` (v0.1.7) |
| `@memory persistent\|session\|none` | stdlib | [x] | Selects memory scope; enforced at runtime (v0.1.10) |
| `@rules [...]` | stdlib | [x] | Injected as a bullet list into the system prompt of every `Ai.*` call (v0.1.3) |
| `@limits { ... }` | stdlib | [~] | `timeout` enforced via `Control.with_timeout` (v0.1.7); `max_tokens`/`max_cost` extracted but not enforced at the Ollama level |
| `@team [...]` | stdlib | [x] | Team membership used by `Agent.broadcast` routing (v0.1.6) |
| `@provider MyProvider` | stdlib | [ ] | Parsed, no per-agent LLM-provider swap |

#### Prelude namespaces

| Namespace | Status | Implemented ops | Gaps |
|---|---|---|---|
| `Ai` | [~] | `classify`, `summarize` (format/max), `draft`, `extract` (as: T), `translate`, `decide`, `prompt` (response_format: json) | `embed` returns `[]`; `Ai.install(provider)` not registered |
| `Io` | [x] | `notify`, `show`, `ask`, `confirm` | — |
| `Schedule` | [x] | `every`, `after`, `at`, `cron`, `sleep` | — |
| `Email` | [~] | `fetch` (IMAP), `send` (SMTP), `archive` (IMAP folder move with fallback) | — |
| `Http` | [x] | `get`, `post`, `request`, `serve` (webhook listener) | — |
| `Env` | [x] | `get`, `require` | — |
| `Log` | [x] | `info`, `warn`, `error`, `debug`, `set_level`, `level` | — |
| `Agent` | [x] | `run`, `stop`, `send`, `delegate`, `broadcast` | — |
| `Memory` | [x] | `remember`, `recall`, `forget` — session (default) or persistent (file-backed JSON) | Vector-store backend (semantic search) is v0.2 |
| `Control` | [x] | `retry`, `with_timeout`, `with_deadline` (v0.1.6) | — |
| `Async` | [x] | `spawn`, `join_all`, `select`, `sleep` (v0.1.7) | — |
| `Cache` | [x] | `set` (optional TTL), `get`, `delete`, `clear` — process-scoped | — |
| `Str` | [x] | `match`, `extract`, `truncate`, `pad` | — |
| `File` | [x] | `read`, `write`, `exists`, `list` | — |
| `Json` | [x] | `parse`, `stringify` | — |
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
- [x] `keel lint` — unused vars, uncalled tasks, `Ai.*` outside agent, unread state; `--fix` flag (v0.1.9)
- [x] `keel lsp` — diagnostics, hover, completion, go-to-definition, rename (v0.1.6–v0.1.8)
- [ ] `keel build` — bytecode compiler + VM (explicitly deferred post-v0.1)

#### Dependencies
- [x] Rust edition 2024 (min rustc 1.85)
- [x] Patch/minor `cargo update` is applied continuously; no version-pin drift tolerated.
- [ ] **Major bumps deferred to v0.2:**
  - `chumsky 0.9 → 1.0` — API rewrite; low payoff during alpha.
  - `imap 2 → 3` — resolves the `imap-proto` future-incompat warning.
  - `colored 2 → 3`, `lettre 0.11 → 0.12` — ergonomic bumps, batched with the above.

**Deferred post-v0.1 with rationale:**
- `keel build` bytecode compiler. The tree-walking interpreter is fast enough for alpha workloads (~8ms cold start). Revisit when there's a concrete motivator (LLVM/WASM backend, embeddable runtime).
- Pluggable LLM provider registry. v0.1 ships with Ollama only; adding a second provider is the forcing function that justifies the registry plumbing.
- Vector-store `Memory` backend. The v0.1.10 persistent store is a JSON file. Semantic search requires an embeddings pipeline and a `VectorStore` interface — that design belongs in v0.2.

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

**Status:** shipped.

### Stdlib — new namespaces

- [x] **`File.read(path)`** — return the file contents as `str`; raise `FileError` if not found.
- [x] **`File.write(path, content)`** — write `str` to a file; create intermediate directories if needed.
- [x] **`File.exists(path)`** — return `bool`.
- [x] **`File.list(dir)`** — return `list[str]` of entry names in the directory.
- [x] **`Json.parse(str)`** — deserialize a JSON string into a Keel map / list / scalar value; raise `JsonError` on invalid input.
- [x] **`Json.stringify(value)`** — serialize a Keel value to a JSON string.
- [x] **`Schedule.cron(expr, fn)`** — schedule `fn` using a standard 5-field cron expression (e.g. `"0 9 * * 1-5"`).

### Stdlib — structured concurrency

- [x] **`Async.spawn(fn)`** — spawn `fn` as an independent Tokio task; returns a handle that can be awaited.
- [x] **`Async.join_all(handles)`** — await a list of task handles; returns a list of results in the same order.
- [x] **`Async.select(handles)`** — resolve to the first handle that completes; cancel the rest.

### Agent attributes — enforcement

- [x] **`@tools [...]` capability gating** — restrict which prelude namespaces (e.g. `Http`, `Email`) are accessible inside the agent body. Calls to unlisted namespaces raise a runtime `CapabilityError`.
- [x] **`@limits { max_tokens, max_cost, timeout }` enforcement** — infrastructure added. `timeout` wraps calls via `Control.with_timeout`; `max_tokens` / `max_cost` extraction is in place for future Ollama integration.

### Developer tooling

- [x] **LSP completion** — suggest prelude namespace methods and keywords at the cursor position.

### Tests

- [x] **Integration tests** — end-to-end `keel run` tests covering: `Schedule.cron`, `Async.spawn`, `@tools` capability gating.
- [x] **Example programs** — 5 `.keel` files in `examples/` showcasing all new features: `file_processing.keel`, `json_processing.keel`, `cron_schedule.keel`, `parallel_execution.keel`, `capability_gating.keel`.

---

## v0.1.8 — Reactive Agents & Text Processing

**Theme:** Let agents listen for external events over HTTP, give programs rich string and regex tools, and add a lightweight shared cache. Completes the LSP feature set planned for v0.1.

**Status:** shipped.

### Stdlib — reactive HTTP

- [x] **`Http.serve(port, handler)`** — start an HTTP listener on `port`; invoke `handler(request)` for each incoming request. `request` is a map with `method`, `path`, `body`. `handler` returns a map with `status`, `body`. Enables agents that react to webhooks rather than only polling.

### Stdlib — string processing

- [x] **`Str.match(text, pattern)`** — return `bool`; true if `pattern` (regex) matches anywhere in `text`.
- [x] **`Str.extract(text, pattern)`** — return the first capture group as `str?`; `none` if no match.
- [x] **`Str.truncate(text, max)`** — truncate to `max` characters, appending `"…"` if cut.
- [x] **`Str.pad(text, width, char?)`** — left-pad to `width` with `char` (default `" "`).

### Stdlib — shared cache

- [x] **`Cache.set(key, value, ttl?)`** — store a value in the process-scoped in-memory cache; optional TTL as a duration literal.
- [x] **`Cache.get(key)`** — return `Value?` (`none` if missing or expired).
- [x] **`Cache.delete(key)`** — evict a key.
- [x] **`Cache.clear()`** — flush all entries.

> `Cache` is process-scoped and not persisted across restarts. It fills the gap between `self.` (per-agent) and `Memory` (persistent vector store, planned for v0.2).

### Developer tooling

- [x] **LSP go-to-definition** — jump to the declaration of `task`, `agent`, `type` identifiers under the cursor. Token-level implementation; `let` bindings not included in v0.1.
- [x] **LSP rename** — rename a symbol and all usages across the open file.

### Tests

- [x] **Integration tests** — end-to-end `keel run` tests covering: `Cache` set/get/delete/clear, `Str` regex match/extract/truncate/pad.
- [x] **Example programs** — 3 `.keel` files in `examples/` for Cache, Str, Http.serve.

---

## v0.1.9 — Tooling

**Theme:** Make the day-to-day development experience first-class — a polished VS Code extension, a linter, a Tree-sitter grammar for other editors, and sharper error output from `keel check`.

**Status:** shipped 2026-05-02.

### VS Code extension (`keel-lang/vscode-keel`)

- [x] **Scaffold `keel-lang/vscode-keel`** — standalone repo with its own CI, `package.json`, `CHANGELOG.md`.
- [x] **Snippets** — 18 snippets: `agent`, `task`, `type`, `interface`, `on`, `@on_start`, `when`, `try`, and more.
- [x] **Format-on-save** — `keel fmt` registered as the document formatter.
- [x] **Run / Check / Lint commands** — `Keel: Run File`, `Keel: Check File`, `Keel: Lint File`, `Keel: Format File` in the command palette.
- [x] **Extension icon** — brand logo from `brand/png/avatar-128.png`.
- [x] **Marketplace publish** — published to VS Code Marketplace as `keel-lang.keel-lang` v0.1.0 on 2026-05-02. CI builds on every push; `v*` tag triggers publish.

### Tree-sitter grammar (`keel-lang/tree-sitter-keel`)

- [x] **Scaffold `keel-lang/tree-sitter-keel`** — standalone repo with `grammar.js` covering the full Keel surface, highlight/locals queries, Node bindings, and test corpus. Neovim, Helix, and Zed setup documented in README.

- [ ] **Scaffold `keel-lang/tree-sitter-keel`** — standalone repo following Tree-sitter conventions (`grammar.js`, `src/`, `bindings/`). Enables syntax highlighting and basic navigation in Neovim, Helix, Zed, and any editor with Tree-sitter support.

### Linter (`keel lint`)

- [x] **`keel lint <file>`** — style and best-practice checks beyond type errors:
  - Unused variables (suppress with `_` prefix).
  - Tasks declared but never called.
  - `Ai.*` calls outside an agent (no `@role` / `@model` context).
  - Agent state fields written but never read.
- [x] **`--fix` flag** — auto-removes unused variable assignment lines.

### `keel check` error quality

- [x] **Source spans in all diagnostics** — every error from `keel check` includes a line:column pointer and an underlined source excerpt via `miette`.
- [x] **Suggestion hints** — arity errors append the expected parameter names as a `hint:` correction.

### Tests

- [x] **Integration tests** — one test per lint rule: unused variable, uncalled task, `Ai.*` outside agent, unread state field; plus span and arity-hint tests for `keel check`.
- [x] **Example programs** — `examples/lint_best_practices.keel` demonstrates the correct patterns the linter validates.

---

## v0.1.10 — Persistent Memory

**Theme:** Make the `Memory` namespace real and close out the last significant user-visible stub in the standard library.

**Status:** shipped.

### Memory namespace

- [x] **`Memory.remember(key, value)`** — store any Keel value under `key`, scoped to the current agent.
- [x] **`Memory.recall(key)`** — return the stored value, or `none` if absent.
- [x] **`Memory.forget(key)`** — remove the key.

### `@memory` attribute enforcement

- [x] **`@memory session`** — in-process store, cleared on restart (default when attribute is omitted).
- [x] **`@memory persistent`** — file-backed JSON store at `~/.keel/memory/<stem>_<hash12>/<agent>.json`; survives restarts. (v0.1.11: path updated to include canonical-path hash for isolation; flock added for cross-process safety.)
- [x] **`@memory none`** — raises `CapabilityError` on any `Memory.*` call inside the agent.

### Example

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

run(Counter)
```

### ROADMAP reconciliation

The v0.1 Alpha section has been updated to accurately reflect all items shipped in v0.1.5–v0.1.9. All stale `[ ]` markers have been corrected.

### Tests

- [x] **Integration tests** — session remember/recall, recall on missing key returns `none`, forget removes key, `@memory none` raises `CapabilityError`, default mode acts as session.
- [x] **Example program** — `examples/memory_agent.keel`.

---

## v0.1.11 — Memory Storage Safety

**Theme:** Make persistent `Memory` safe for cross-process use and give each source file a unique storage bucket.

**Status:** shipped.

### Changes

- [x] **Path identity** — directory name changed from `<stem>` to `<stem>_<sha256[:12]>`. Two `counter.keel` files in different directories now have separate storage.
- [x] **Cross-process safety** — `flock` advisory locking via `fs2`. Each `Memory.*` call acquires exclusive (write) or shared (read) lock on a sidecar `.lock` file.
- [x] **Crash durability** — `fsync` on temp file + parent dir before and after rename.
- [x] **Hard path validation** — agent names validated at storage boundary (replaces `debug_assert`).
- [x] **New dependencies** — `sha2 = "0.10"`, `fs2 = "0.4"`.

### Tests

- [x] Identity isolation: same basename, different paths → separate storage
- [x] Symlink resolves to same storage as target
- [x] `repl.keel` file uses `repl_<hash>`, not `__repl__`
- [x] Cross-process write race: both processes complete, JSON is valid
- [x] Concurrent reads: two processes with shared lock both succeed
- [x] `.lock` file exists alongside `.json` after write
- [x] Corrupt file renamed to `.bak`, error returned
- [x] Invalid agent name rejected (unit test)

### Breaking

Existing persistent memory data at `~/.keel/memory/<stem>/` is not migrated. Move manually if needed.

---

## v0.1.12 — Range Operator `..`

**Theme:** Implement the inclusive range operator `..` specified in SPEC.md §operator-table.

**Status:** shipped.

### Changes

- [x] **`DotDot` token** — lexer emits `..` before the single `Dot` token; `continues_to_next_line` updated.
- [x] **`Expr::Range`** — dedicated AST variant (like `NullCoalesce`); evaluates to `list[int]`.
- [x] **Parser** — `RangeExpr` level inserted between `AddExpr` and `CompExpr` (tighter than comparison, looser than arithmetic); `..` is non-chainable.
- [x] **Type checker** — both bounds must be `int`; emits "range start/end must be int" errors otherwise; returns `list[int]`.
- [x] **Interpreter** — `Value::Range(i64, i64)` lazy variant; `for` iterates without allocating; analytical methods (`count`, `is_empty`, `contains`, `first`, `last`) are O(1); `map`/`filter` iterate lazily and return a new list.
- [x] **Formatter** — `start..end` with no spaces.
- [x] **SPEC** — `RangeExpr` added to grammar; bare `!` postfix documented in §20.

### Tests

- [x] `range_basic_for_loop` — `for i in 1..3` prints 1, 2, 3
- [x] `range_assigned_to_variable` — `1..4` has count 4
- [x] `range_type_error_non_int` — `1.0..3.0` is a type error
- [x] `range_empty` — `5..3` produces empty list (count 0)
- [x] `range_single` — `4..4` produces `[4]` (count 1)
- [x] `examples_all_parse` — `range.keel` example added

---

## v0.1.13 — Destructuring (§8.4)

**Theme:** Implement all destructuring forms prescribed in SPEC.md §8.4.

**Status:** shipped.

### Changes

- [x] **`DestructPat` / `Binding` AST types** — `src/ast.rs` gains two new enums. `Stmt::Let.name: String` → `binding: Binding`, `Stmt::For.binding: String` → `Binding`, `Param.name: String` → `Binding`.
- [x] **Parser** — `struct_destruct_pat()` and `tuple_destruct_pat()` standalone parsers; `destruct_struct_let`, `destruct_tuple_let`, `destruct_for_stmt` added to the statement `choice`; `task_decl` and `on_handler` param parsers extended to accept struct destructure.
- [x] **Type checker** — `bind_to_scope()` helper enforces struct field existence and tuple arity; missing fields and arity mismatches are compile-time errors.
- [x] **Interpreter** — `bind_value()` / `bind_destructure()` helpers; all `let`, `for`, and task-param binding paths updated.
- [x] **Formatter** — `binding_str()` helper renders `{ a }`, `{ a: x, b }`, `(a, b)` correctly; idempotent roundtrip.
- [x] **Linter** — `check_block_unused` extracts all names from destructure bindings; destructure bindings are not auto-fixable.
- [x] **LSP** — `collect_stmt_bindings` and `collect_decl_bindings` updated; destructured names appear in hover results.

### Supported forms

```keel
{urgency, category} = result          # struct shorthand
{urgency: u, category: c} = result    # struct rename
(urgency, summary) = triage_full(email)  # tuple
for {from, subject} in emails { ... }   # in for
task handle({body, from}: Email) { ... }  # in params
```

Keyword-named fields (`from`, `state`, `in`, etc.) work in all forms.

### Tests

- [x] `destruct_struct_shorthand` — `{name, age} = val`; both bound
- [x] `destruct_struct_rename` — `{urgency: u, category: c} = val`; renamed locals bound
- [x] `destruct_tuple` — `(label, count) = pair`; positional elements bound
- [x] `destruct_in_for_loop` — `for {name, score} in items`; each element destructured per iteration
- [x] `destruct_in_task_param` — `task show_point({x, y}: Point)`; called correctly
- [x] `destruct_missing_field_type_error` — `{nonexistent} = val` → type error
- [x] `destruct_tuple_arity_mismatch_type_error` — `(a, b) = triple` (3-tuple) → type error
- [x] `destruct_keyword_field_from` — `{from, subject} = email` parses correctly
- [x] `examples_all_parse_includes_destructure` — `keel check destructure.keel` passes
- [x] `examples_all_parse` — `destructure` added to the smoke list

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
