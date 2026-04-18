# Changelog

All notable changes to Keel.

> **Alpha.** Keel is v0.1. Breaking changes are expected between 0.x releases. Do not build production systems on 0.x.

---

## [Unreleased]

Nothing yet.

---

## [0.1.0] — Alpha

First public release. The language, standard library, and tooling are all new.

### Language

- **Small core.** 27 reserved keywords total. Everything else (AI calls, scheduling, I/O, HTTP, email, memory, search) is a stdlib function, not syntax.
- **Prelude-as-stdlib.** The namespaces `Ai`, `Io`, `Email`, `Http`, `Schedule`, `Memory`, `Async`, `Control`, `Env`, `Log`, `Agent` are in scope in every program with no `use` needed.
- **Interfaces.** Structural protocol declarations (`interface LlmProvider { ... }`); any type with matching methods satisfies the interface — no explicit `implements`.
- **Attributes.** `@role`, `@model`, `@tools`, `@memory`, `@rules`, `@limits`, `@on_start`, `@on_stop`, `@team` on agent bodies. `@role` and `@model` are core; the rest are stdlib-registered plugin handlers.
- **Named arguments** on calls: `Ai.classify(body, as: Urgency, fallback: Urgency.medium)`.
- **Algebraic data types.** Simple enums (`type Urgency = low | medium | high`) and rich enums with per-variant fields (`type Action = reply { to: str, tone: str } | archive`). Construction: `Action.reply { to: "x", tone: "y" }`. Destructuring: `when a { reply { to, tone } => ... }`.
- **Type aliases.** `type Timestamp = datetime`.
- **Triple-quoted strings.** `"""..."""` preserves newlines and internal quotes; still supports `{expr}` interpolation.
- **Nullable safety.** `T?` is a distinct type; `?.`, `??`, `fallback:` are the handling tools.
- **Exhaustive pattern matching.** `when` on an enum must cover every variant or use `_`; compile-time error if missing.
- **`as` cast.** `Ai.prompt(...) as MyType` narrows; required for `Ai.prompt` to avoid accidental `dynamic`.
- **Duration literals.** `5.minutes`, `2.hours`, `1.day`. `Schedule.*` and `Async.sleep` accept them directly.

### Runtime

- **Tree-walking interpreter on Tokio.** ~8ms cold start. Agents are the sole concurrency primitive: per-agent serial mailbox, isolated mutable state via `self.`, handlers run one at a time.
- **Event loop.** `mpsc`-driven. Scheduled ticks and cross-agent messages flow through it; `Ctrl+C` and `KEEL_ONESHOT=1` both exit cleanly.
- **Recurring `Schedule.every`.** Spawns a tokio interval that posts `FireClosure` events at each tick; `Schedule.after` is the one-shot variant; `Schedule.at(datetime_str, fn)` accepts RFC 3339 / ISO 8601.
- **Message dispatch.** `Agent.send(target, data)` posts a `Dispatch` event that fires the target agent's `on <event>` handler in its own `self` context.
- **Rich enum runtime values.** `Value::EnumVariant(type, variant, Option<fields>)`; pattern destructuring binds fields by name.
- **Prelude dispatch.** Every namespace resolves through a runtime registry (`Ai.classify` is a method lookup on a registered namespace value). Swappable at startup.

### Standard library

- **`Ai`** — Ollama backend only in v0.1 (via `LlmProvider` interface; Anthropic and other providers are follow-up work).
  - `Ai.classify(input, as: T, fallback: V, considering: { "hint": Variant })` returns `Ty::Enum(T)`.
  - `Ai.summarize(text, in: N, unit: sentences, fallback: ...)`, `Ai.draft(prompt, tone: …, guidance: …)`, `Ai.extract(from: …, schema: …)`, `Ai.translate(text, to: …)`, `Ai.decide(input, options: […])`, `Ai.prompt(system: …, user: …) as T`.
  - Model resolution: `using:` arg ≻ enclosing agent's `@model` ≻ `KEEL_OLLAMA_MODEL` catch-all. `KEEL_MODEL_<ALIAS>` maps custom aliases to Ollama tags.
  - `KEEL_LLM=mock` short-circuits every call — used by the integration test suite and `keel run` in offline mode.
- **`Email`** — real IMAP fetch + SMTP send via env vars `IMAP_HOST`, `SMTP_HOST`, `EMAIL_USER`, `EMAIL_PASS`. Gracefully degrades to empty list / no-op when credentials aren't set.
- **`Http`** — `reqwest`-backed. `Http.get`, `Http.post`, `Http.request` return a `{status, body, headers, is_ok}` map.
- **`Io`** — terminal-backed `ask`, `confirm`, `notify`, `show`.
- **`Env`** — `Env.get(name)` returns `str?`, `Env.require(name)` errors if unset.
- **`Log`**, **`Memory`**, **`Search`**, **`Db`**, **`Async`**, **`Control`** — stub implementations shipping with the binary; real backends land alongside usage.

### Tooling

- `keel run <file>` — execute.
- `keel check <file>` — static analysis: undefined identifiers, `self` outside an agent, non-exhaustive `when` on enums, missing `_` on non-enum `when`, `if` condition / `for` iterator types, task argument arity, `Ai.classify` enum inference.
- `keel repl` — interactive; persists bindings and declarations across prompts; brace-balance-aware multi-line input; `~/.keel_history`.
- `keel fmt <file>` — idempotent AST pretty-printer. Two-space indent, multi-line lambda block bodies, automatic string-key quoting for map keys with spaces.
- `keel lsp` — language server over stdio (tower-lsp). Publishes lex/parse/type-check diagnostics on `did_open` / `did_change`. Hover/completion are placeholders.
- `keel build` — **deferred post-v0.1.** The tree-walking interpreter is the supported execution path. A real bytecode VM has to re-solve async dispatch, closure capture across event-loop re-entry, and runtime-pluggable namespace dispatch; none of those have a matching user payoff yet.

### Distribution

- **GitHub release workflow** builds three targets on tag push: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`. Computes SHA-256 per tarball and embeds them in the release notes.
- **`install.sh`** served at `https://keel-lang.dev/install.sh` — one-liner that fetches the latest release for your OS + arch.
- **Homebrew tap** (`keel-lang/homebrew-tap`): the release workflow writes `Formula/keel.rb` with the new version + URLs + SHAs on every tag. `brew install keel-lang/tap/keel`.
- **GitHub Pages** deploys the mdBook documentation to `keel-lang.dev` on every push to `main`, with `install.sh` and `uninstall.sh` copied into the root.

### Documentation

- `SPEC.md` — authoritative language specification, v0.1.
- `ROADMAP.md` — v0.1 checklist + deferred items.
- `VISION.md` — design principles and target audience.
- `docs/src/` (mdBook, 24 pages): getting started, language guide, stdlib namespace reference, CLI reference, configuration, examples.
- 15 example `.keel` programs in `examples/` covering scheduling, agents, AI primitives, message dispatch, HTTP, rich enums, multi-agent preview.

### Tests

101 green across: lexer (39), parser (24), type checker (18), formatter (5), LSP (5), integration (10 end-to-end program runs via `keel run`).

### Versioning

- Semver is **not** respected between 0.x minor versions.
- 0.1.x — breaking changes allowed in patch releases.
- 0.2+ scope is deliberately un-planned until 0.1 lands in the wild.
- 1.0 — first API-stable release.
