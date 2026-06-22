# Non-Goals

Ideas that have been considered and **deliberately set aside** — recorded with their reasoning so they don't get re-proposed from scratch. Most are "not now"; a few are "not ever." This is institutional memory, not a backlog — actionable work lives in [GitHub Issues](https://github.com/keel-lang/keel/issues).

## Language & type system

- **Separate `none` from a distinct unit type.** `none` doubles as the unit value and the nullable-empty value. Splitting them is theoretically cleaner but would break every existing `.keel` program for marginal gain — nullable types and typed errors already distinguish the cases that matter.
- **Type-predicate / refined types** (`type ValidInvoice = Invoice where amount > 0`). Dependent-types-lite is hard to tool and hard to check at compile time. Runtime validation with existing patterns covers the need.
- **Attribute schemas with lifecycle phases.** A parse/check/init schema system for attributes is premature while only a handful of attributes exist. Revisit if the attribute count grows past ~10.
- **`Json[T]` typed indexing.** Tracking an `as`-cast type through field access needs a stronger checker than we have today; `data["key"]` works now. Revisit when inference is stronger.
- **A register pattern for the hardcoded symbol-identifier array.** It's fine at its current size; a register pattern would add more ceremony than it removes.

## Agents & AI

- **LLM-driven tool dispatch / "autopilot."** Letting the model discover and call tasks is inherently hard to debug, audit, and test deterministically. Explicit `when`-on-enum dispatch is clearer and safer.
- **An `Ai.reason` scratchpad block.** Would tie a language feature to model-specific "thinking" support with no universal fallback. Fragile.
- **Structural agent matching** (an agent satisfies an interface when its `on` handlers line up). Adds implicit coupling for marginal gain; explicit delegation is clearer.
- **`Knowledge[T]` typed RAG.** A vector database, embedding model, index, and retrieval ABI inside the language is enormous scope. RAG belongs behind an MCP server that Keel calls, not embedded in the language.
- **Durable human-in-the-loop waits** (persisting a pending `Io.ask` across restarts). Durable execution — checkpointing, state serialization, restart safety — is a deep systems problem. v1.0+ territory, and only after journal replay is proven.

## Observability

- **Journal overlays in the IDE** (ghost-text last LLM response, inline trace overlays). Significant LSP work for unclear adoption; most debugging happens via CLI replay.
- **A `@journal persistent` attribute.** A CLI flag already enables journaling; an attribute for it is pure ceremony.

## Tooling, CI & ecosystem

- **Tests for the stub VM / bytecode path.** Don't write tests for dead code. The VM either gets built or stays deferred alongside the `keel build` direction.
- **A benchmark harness.** Cold-start performance isn't a user-facing promise yet; not essential for alpha.
- **Swapping `chrono` / `axum` / `reqwest` for lighter crates.** They work, with no bugs, perf issues, or advisories. Churn without a concrete problem to solve.
- **WASM `extern namespace` plugins.** Embedding a WASM runtime — sandboxing, memory model, component model — is a massive undertaking. Let ecosystem demand prove itself first.
- **A package manager with URL-based imports** (`keel.toml`, `use "github.com/…"`). The module system this depended on has since shipped, so it's no longer blocked — but it's still not committed work. Revisit when there's real demand for third-party package distribution.
- **`Io.form(as: T)` schema-driven UI.** Cross-platform form rendering is a UI framework, not a language feature. Belongs in tooling.
- **A `@capability` system for shell.** There's no `System` namespace to gate yet; build the namespace before a capability system for it.
- **Ambient `context { … }` propagation.** Thread-local implicit data flow is hard to debug; passing values like `trace_id` through explicit task parameters is clearer and auditable.
