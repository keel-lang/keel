# Changelog

All notable changes to Keel.

> **Alpha.** Keel is v0.1. Breaking changes are expected between 0.x releases. Do not build production systems on 0.x.

---

## [Unreleased]

Nothing yet.

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
- **Attributes.** `@role`, `@model` are core attributes. `@on_start`, `@on_stop`, `@tools`, `@memory`, `@rules`, `@limits`, `@team`, `@provider` are stdlib attributes, parsed by the grammar. Wiring status: `@role`, `@model`, and `@on_start` are executed at runtime — `@role` is prepended as `"You are {role}.\n\n..."` to every `Ai.*` system prompt so the agent's identity reaches the LLM on every call. The remaining stdlib attributes are parsed but have no runtime effect in v0.1 (tracked in [ROADMAP](ROADMAP.md)).
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
  - Partial: `Ai.classify(..., considering: {...})` — argument parsed but not forwarded to the LLM yet; `Ai.summarize(..., format: ...)` — ignored; `Ai.prompt(..., response_format: json)` — ignored; `Ai.decide` returns a plain `{choice, reason, confidence: 1.0}` map instead of a typed `Decision[T]`; `Ai.extract` accepts the map-form schema only.
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
