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
- [x] `keel lsp` — diagnostics (lex + parse + type-check). Hover and completion are stubs pending a follow-up.

**Deferred post-v0.1:**
- `keel build` bytecode compiler. The tree-walking interpreter is fast enough for alpha workloads (~8ms cold start), and a real VM has to re-solve async dispatch, closure capture across event-loop boundaries, and runtime-pluggable namespaces — costly without matching user payoff. Revisit when there's a concrete motivator (LLVM/WASM backend, embeddable runtime).

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
