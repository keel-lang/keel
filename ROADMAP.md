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
- [x] Lexer
- [x] Parser: `@attributes`, `interface` / `extern` / `use` declarations, named arguments, `as T` cast
- [x] Interpreter: namespace dispatch, agent lifecycle with `@on_start`, `self.` state, pattern matching, closures
- [x] Prelude wiring: `Ai`, `Io`, `Schedule`, `Email`, `Http`, `Memory`, `Async`, `Control`, `Env`, `Log`, `Agent`
- [x] Examples: 11 `.keel` programs parse and execute
- [x] Type checker: undefined identifiers, exhaustive `when`, `self.` outside agents, `if`/`for` condition types, arg-count checks, basic enum inference (nullable safety + full return-type matching deferred)
- [x] Ollama LLM backend wired into `Ai.*`
- [x] Real email backends wired into `Email.*` (IMAP fetch, SMTP send)
- [x] Real HTTP client wired into `Http.*` (reqwest)
- [x] Recurring `Schedule.every` / `Schedule.after` via the agent event loop
- [x] `Schedule.at` (absolute-time scheduling via ISO 8601 / RFC 3339 parse)
- [x] Message dispatch to `on <event>` handlers via `Agent.send`
- [x] Rich enum variant construction: `Action.reply { to: "..." }`
- [x] Triple-quoted strings
- [x] `keel fmt` rewrite against the new AST (idempotent round-trip)
- [x] `keel repl` on the new interpreter
- [ ] `keel lsp` with diagnostics, completion, hover
- [ ] `keel build` bytecode compiler against the new AST

### Release
- [ ] First binary release (macOS + Linux), Homebrew tap, `curl | sh` installer

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
