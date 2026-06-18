# Keel TODO

Consolidated from `.claude/plans/` — every non-implemented idea, critique, audit finding, and design proposal. Each item verified against the current codebase. Done items excluded.

Verdict: **plan** (worth doing — aligned with Keel's purpose, no redundant overlap, achievable without new keywords) | **skip** (doesn't make sense, redundant, too much complexity for the gain, or blocked on prerequisites that aren't themselves planned)

---

## Language Design & Type System

| Ref | Short | Description | Verdict | Why |
|-----|-------|-------------|---------|-----|
| K-01 | Close type-checker promise gap | Spec promises "full inference"; checker falls back to `Ty::Unknown` for generics, closures, prelude return types. Either complete the inference engine or downgrade the spec. `--strict` surfaces the gap but doesn't close it. | **plan** | An agent language must be trustworthy. If users believe something is statically checked but it isn't, the language trains them to overtrust AI-facing code. |
| K-02 | Unify failure model | `T?`, `try/catch`, `??`, `!` clean rule: nullable for absence, thrown typed errors (with reasons) for fallible ops, thrown errors for programmer faults. (`Result[T,E]`/`fallback:` were never live; not revived.) | **done** ([#38](https://github.com/keel-lang/keel/issues/38)) | Shipped: `ai.*` provider failures now throw `AiError` with a `reason` (`"unavailable"`/`"provider"`) instead of silently returning `none`; `none` strictly means absence; mock = deterministic absence; `AiSchemaError` unchanged. Other fallible namespaces (file/http/json/email) already threw typed errors. |
| K-03 | Separate `none` from unit type | `none` is both the unit value and nullable-empty value. A task returning "nothing meaningful" vs a lookup that failed are different states. | **skip** | IDEAS.md DROP: "High breaking cost; Result + nullable already distinguish the relevant cases." Would break every existing `.keel` program for a theoretical purity gain. |
| K-04 | Compile-time capability checking | `@tools` is runtime-only (`CapabilityError` at runtime). Add compile-time checking so misconfigured agents fail before execution. Default to least-privilege. Track capability flow through task delegates. | **plan** | Agents combine LLM output with side effects. A runtime error is too late if a task unexpectedly has access to `File`, `Http`, or `Email`. No new keywords needed — uses existing `@tools` attribute. |
| K-05 | Opaque/newtype wrappers | `opaque type EmailAddress = str` for domain safety. Structural typing makes it easy to cross-wire security-sensitive strings (API keys, file paths, URLs, agent names) into dangerous calls. | **plan** | Agents handle lots of sensitive strings. Can be done as `@opaque type` attribute — no new keyword, just an attribute on existing `type` syntax. |
| K-06 | Type-predicate refined types | `type ValidInvoice = Invoice where amount > 0` — compile-time vs runtime evaluation unclear. Dependent types lite. | **skip** | IDEAS.md Triage. Dependent types are hard to tool and hard to check at compile time. Simpler alternative already exists: `Validate.require(val, fn) -> Result` using current runtime patterns. |
| K-07 | Tiered prelude + shadowing error | All stdlib namespaces are auto-imported and can be silently shadowed. Split into core (`Ai`, `Io`, `Agent`, `Log`, `Env`, `Time`) vs optional capabilities (`Email`, `Http`, `File`, `Db`, `Search`). Make shadowing a compile error. | **plan** | Zero imports are great for demos, dangerous for production agents. A typo or local binding shouldn't silently replace `Http` or `Email`. No new keywords. |
| K-08 | Prelude signature table | A first-class prelude signature table used by checker, LSP, docs, and runtime. Would make prelude return types known to the checker instead of falling back to `Unknown`. | **plan** | Prerequisite for K-01. Once the checker knows what `Ai.classify` returns, half the "unknown type" falls away. |
| K-09 | Attribute schemas with lifecycle phases | Define parse-time, check-time, and init-time schemas so the checker and LSP can validate attribute shapes. | **skip** | IDEAS.md Triage: "Premature until @tools, @limits, @reasoning shapes are stable." Only ~5 attributes exist today — adding a schema system for them is abstraction before evidence. Revisit when attribute count passes ~10. |
| K-10 | AI primitives formal contract | All `Ai.*` calls should share one contract: prompt input shape, provider request, schema validation, retry/fallback, error type, trace/journal event, token accounting, determinism controls for tests. | **plan** | The language's main purpose is AI agents. The `Ai` namespace is the trust boundary between typed code and probabilistic output. A consistent contract is table stakes. |
| K-11 | `Json[T]` typed indexing | `Json.parse(raw) as Order` then `data.items.map(...)` — requires checker to track `as`-cast type through field access. | **skip** | Blocked on K-01/K-08. Requires the checker to do what it currently can't. Meanwhile `data["items"]` works fine. Revisit when the type checker is stronger. |
| K-12 | Remove `to_display_string` call sites | `impl fmt::Display for Value` exists at `value.rs:105`. ~60 call sites still use the old `to_display_string()` method. Finish the migration. | **plan** | Pure cleanup. Zero risk, zero design. The Display impl is already there. |
| K-13 | `SYMBOL_IDENTS` hardcoded array | 19 string values in a `&[&str]` constant bound as bare identifiers. Consider a register pattern. | **skip** | Works fine for 19 items. A register pattern would be more ceremony than the hardcoded array it replaces. Revisit if the list grows past ~30. |

---

## Interface / Provider System

| Ref | Short | Description | Verdict | Why |
|-----|-------|-------------|---------|-----|
| K-14 | Ship one real interface implementation | `interface` declarations parse but don't dispatch. `@provider` parses but Ollama is hardcoded. Implement `LlmProvider` with two concrete backends (Ollama + mock/test) sharing the interface ABI. | **plan** | THE central architectural bet of Keel: swappable providers. Without this, Keel is a fixed framework with language syntax, not a language with replaceable backends. The single biggest gap between spec and reality. |
| K-15 | Interface conformance checks | **Done** (#45). `impl Interface for Type` conformance is enforced in both checker and runtime via the shared `types/interface.rs` — presence, arity, parameter types, return type, and extra-method rejection. `dynamic` in the interface position is a wildcard on both parameters and return. Provider *dispatch* (the `@provider` wiring) is tracked separately in K-14/K-16. | **done** | Method-signature conformance is complete; the static checker and the runtime apply identical rules through `signature_satisfies`. |
| K-16 | Typed provider ABI | Define the runtime contract for provider dispatch: per-call, per-agent, per-program scoping; method resolution; error propagation. | **plan** | Follows from K-14. The interface system needs a concrete dispatch contract. |

---

## Agent Features

| Ref | Short | Description | Verdict | Why |
|-----|-------|-------------|---------|-----|
| K-17 | `@reasoning` block | Three-phase agent turn: `before` (deterministic pre-processing), `instructions` (LLM prompt generation), `after` (state updates). `Ai.*` calls enforced to only appear in instructions phase. | **plan** | Makes agent logic readable. Enables phase-specific lint rules. Uses existing `@` attribute system — no new keywords. IDEAS.md: planned near-term. |
| K-18 | Agent lifecycle hardening | Mailbox backpressure over unbounded channels, `@on_stop` drain vs cancel semantics, cancellation propagation to spawned tasks, supervision/restart policy. | **plan** | Agent systems fail in lifecycle edges. Erlang, Akka, and Orleans all prove that supervision and cancellation semantics aren't optional once programs go multi-agent. No new keywords expected. |
| K-19 | Durable HITL waits | `Io.ask(..., resume: webhook)` persists the pending continuation across restarts. Human-in-the-loop agents may wait hours/days. | **skip** | IDEAS.md Triage. Design after `.keelj` replay is proven. This is v1.0+ territory — durable execution is a deep systems problem (checkpointing, state serialization, restart safety). |
| K-20 | `Ai.autopilot` — interface-driven tool dispatch | Let the LLM discover and call tasks via an `interface`. `Ai.autopilot(msg, using: ToolBelt)`. | **skip** | IDEAS.md Triage, blocked on K-14. Beyond that: autopilot is inherently "magical" — hard to debug, hard to audit, hard to test deterministically. Explicit dispatch via `when` on enums is clearer and safer. |
| K-21 | `Ai.reason` block | LLM reasoning scratchpad before acting. `Ai.reason { "Analyze tone", "Check urgency" }`. | **skip** | IDEAS.md Triage. Requires native thinking support (Claude, o1, Gemini). No universal fallback for models that don't support it. Tying a language feature to specific model capabilities is fragile. |
| K-22 | Structural agent matching | An agent satisfies an interface if its `on` handlers match. | **skip** | IDEAS.md Triage, blocked on K-14. Adds implicit coupling for marginal gain — explicit delegation already works and is clearer. |
| K-23 | `Knowledge[T]` typed RAG | `Knowledge.absorb("./handbook.pdf")`, `Knowledge.query(Message, "pto") -> list[Message]`. | **skip** | IDEAS.md Triage. A vector database inside the language: embedding model, index, typed retrieval, provider ABI. Massive scope. RAG is better served by MCP servers (K-59) — let Keel call them, not embed them. |

---

## Observability & Debugging

| Ref | Short | Description | Verdict | Why |
|-----|-------|-------------|---------|-----|
| K-24 | `.keelj` journal — record | Append-only JSONL execution trace: `Ai.*`, `Io.ask`, agent lifecycle, state writes, schedule fires, HTTP/email ops. `keel run --journal`. | **plan** | Fundamental for debugging agent behavior. No new keywords, CLI-driven, well-designed in `journal-codex.md`. |
| K-25 | `.keelj` journal — replay | Strict replay: re-runs the `.keel` program substituting recorded outputs, fails on divergence. `keel replay`. | **plan** | Makes real incidents reproducible as regression tests. Companion to K-24. |
| K-26 | `.keelj` journal — fork | Derived trace with patch events. `keel fork --replace 42='{...}'`. Enables "what-if" debugging. | **plan** | Companion to K-24. Enables answering "what would this agent have done if `Ai.classify` returned X instead of Y?" |
| K-27 | OpenTelemetry OTLP spans | Runtime emits OTLP spans for agent handoffs, LLM calls, tool invocations. Use existing tools (Jaeger, Honeycomb). | **plan** | Industry standard, no new Keel concepts. Makes Keel enterprise-observable from day one. After K-24 ships; shares boundary-recording logic. |
| K-28 | Journal LSP integration | Last LLM response shown as IDE ghost text, trace overlays on code. | **skip** | Fancy but niche. Most debugging happens via CLI replay, not IDE overlay. Significant LSP work for a feature with unclear adoption. |
| K-29 | `@journal persistent` attribute | Syntax sugar for `--journal` as an agent attribute. | **skip** | CLI flag already covers this. Adding an attribute for something a flag does is ceremony. |

---

## Testing

| Ref | Short | Description | Verdict | Why |
|-----|-------|-------------|---------|-----|
| K-30 | Test blocks + `mock` keyword | Built-in language testing: `test "triage" { mock Ai.classify => Severity.critical; assert classify(...) == Severity.critical }`. | **plan** | Agent languages are hard to trust without deterministic tests. IDEAS.md: Ship. Resolve keyword vs prelude via `design-lang` first. |
| K-31 | Fake providers + test helpers | `Time.freeze`, mailbox test helpers, prompt snapshot tests. Prelude extensions in test context; no new keywords. | **plan** | Essential companion to K-30. Makes AI workflows testable by construction. |
| K-32 | Split integration test file by namespace | `tests/integration_tests.rs` is a 4,352-line monolith covering 158 tests. Split into `tests/integration/` sub-files (smoke, agent, ai, schedule, language, namespaces, net, util, memory, lint, lsp, tools, strict) under a single test binary, with shared helpers in `tests/common/mod.rs`. Full plan in `.claude/plans/integration-test-split.md`. | **plan** | 4,352 lines in one file makes it hard to find tests, review changes, or run a focused subset. Splitting by area makes maintenance tractable without changing how tests execute. |
| K-33 | Namespace unit tests | 7 of 18 namespace files (`agent`, `ai`, `asynchronous`, `control`, `http`, `io`, `json`) have zero dedicated unit tests. | **plan** | Coverage gap. Integration tests alone don't exercise error paths. |
| K-34 | Interpreter error path tests | `eval_binary_op` (~200 lines) has no tests for overflow, divide-by-zero, or unsupported type combinations. | **plan** | Interpreter is the execution heart. Error paths must be tested. |
| K-35 | Formatter edge case tests | 5 tests for a 997-line formatter. No tests for nested interpolation, rich enum patterns, duration literals, or deeply nested blocks. | **plan** | Formatter must be idempotent for all constructs. Thin coverage risks regressions. |
| K-36 | LSP feature tests | 5 tests for 491 lines. Diagnostics only; no tests for completion, hover, go-to-definition, or rename. | **plan** | LSP is the primary developer interface. Diagnostics-only testing is insufficient. |
| K-37 | VM module tests | `src/vm/` (bytecode, compiler, machine) — three files, zero tests. VM returns `Err("unimplemented")` for any real opcode. | **skip** | Don't write tests for dead code. Remove the VM module (K-41) or wait until the VM is actually built. |
| K-38 | Doc-tests | `cargo test --doc` reports 0 tests. Adding doc-tests to `lib.rs`, `pipeline.rs`, and key modules serves as both documentation and regression tests. | **plan** | Low effort, high value. Doc-tests kill two birds: they document the API and catch regressions. |

---

## Structural Refactoring

| Ref | Short | Description | Verdict | Why |
|-----|-------|-------------|---------|-----|
| K-39 | Split `interpreter/mod.rs` | 2,390-line monolith: agent lifecycle, event loop, namespace dispatch, binary ops, closure management, pattern matching, struct construction, all statement/expression evaluation. Split into `eval.rs`, `agent.rs`, `dispatch.rs`. | **plan** | Largest single refactor target. Required before structured concurrency or cancellation. Every feature that lands before the split increases the cost. |
| K-40 | Split `runtime/context.rs` | 811 lines: `RuntimeContext`, four backend traits, all native impls, all test doubles, atomic-write persistence. Split per concern. | **plan** | Discovery is harder than it needs to be. Traits and test doubles shouldn't live in the same file. |
| K-41 | Remove or gate `.keelc` dead path | `pipeline.rs` has a `.keelc` branch that loads bytecode and calls `VM::new().execute()` — but the VM returns unimplemented for any real opcode. | **plan** | Dead code that implies a working compiler when none exists. Either delete or wrap behind a compile-time feature flag. |
| K-42 | Module-level doc for `context.rs` | Only module in the codebase without a `//!` module-level doc. | **plan** | Trivial fix. Every other major module has one. |
| K-43 | Next-cron-execution synchronous scan | `next_cron_execution` does a synchronous minute-by-minute scan within the event loop. | **plan** | Blocks the runtime. Move to `spawn_blocking` or use a mathematical next-fire calculation. Audit2 accepted follow-up. |

---

## CI & Release Infrastructure

| Ref | Short | Description | Verdict | Why |
|-----|-------|-------------|---------|-----|
| K-44 | macOS CI runner | Primary target platform has no CI coverage. Binary is built on tag push only. | **plan** | Primary platform. CI catches macOS-specific issues (linker, file paths, signal handling). |
| K-45 | `cargo audit` in CI | No automated dependency advisory detection. `imap-proto` future-incompatibility warning will become a hard build failure. | **plan** | Catches advisory regressions before they hit `main`. |
| K-46 | Coverage measurement in CI | No `cargo tarpaulin` or `cargo llvm-cov` step. Coverage trends are invisible. | **plan** | Makes coverage gaps (K-33 through K-36) visible and trackable over time. |
| K-47 | Release workflow runs `rust_audit.sh` | Release tags could ship with clippy warnings or test failures since the audit script isn't in the release workflow. | **plan** | One-line YAML addition. Prevents shipping known-bad builds. |
| K-48 | Benchmark harness | `cargo bench` doesn't exist. Cold-start claim (~8ms) is untracked. | **skip** | Nice to have but not essential for alpha. Cold-start performance isn't a user-facing promise yet. |
| K-49 | Dependency audit (chrono, axum, reqwest) | `chrono` + 27 transitive crates could be replaced by `time`; `axum` used only for a webhook listener; `reqwest` is a full HTTP client. | **skip** | These work fine. No bugs, no perf issues, no advisories — just "lighter alternatives exist." Churn without clear benefit. Revisit if a concrete problem emerges. |
| K-50 | `cargo deny` config | No license auditing, duplicate detection, or advisory database integration. | **plan** | License auditing prevents legal surprises for a language that will be embedded in other projects. |

---

## Documentation

| Ref | Short | Description | Verdict | Why |
|-----|-------|-------------|---------|-----|
| K-51 | Architecture doc | No document explains pipeline flow, interpreter event loop, or namespace registration. A new contributor must reverse-engineer from 14k lines of Rust. | **plan** | Onboards contributors. The pipeline (lexer → parser → checker → interpreter) and event loop deserve a single canonical document. |
| K-52 | Feature status audit | `features.json` + `status_docs_tests.rs` (5 tests, all pass) enforce consistency between roadmap and status JSON. Audit whether all namespaces, language features, and docs pages are fully reflected. | **plan** | Small effort to close remaining gaps in an already-working system. |

---

## Ecosystem & FFI

| Ref | Short | Description | Verdict | Why |
|-----|-------|-------------|---------|-----|
| K-53 | WASM `extern namespace` | `extern namespace Pg from "./postgres.wasm"` — WASM as universal plugin format. | **skip** | IDEAS.md Triage. WASM runtime embedding is a massive engineering undertaking (sandboxing, memory model, component model). Let the ecosystem prove demand before committing to this. |
| K-54 | `keel.toml` + URL-based imports | Package manager: `use "github.com/keel-lang/stdlib/slack" as Slack`. | **skip** | Blocked on K-55. Package management without a module system is putting a roof on a house with no walls. Revisit after modules ship. |
| K-55 | Module system | `use std/<name>` + `use "./file.keel"` resolve end-to-end: namespaced imports, `as` aliasing, symbol imports, implicit main, per-file tests, cycle errors, ambient PascalCase prelude removed with tombstone diagnostics. | **Done** | Shipped — see [#66](https://github.com/keel-lang/keel/issues/66) and SPEC §20. |
| K-56 | `Io.form(as: T)` — schema-driven UI | Cross-platform form rendering from a type schema. | **skip** | This is a UI framework, not a language feature. "The CLI renders a form; a web UI renders a React component" — belongs in tooling, not in the language spec. |
| K-57 | `@capability` system for shell | `@capability [System.read_env, System.exec("git")]` — capability-based security for shell commands. | **skip** | `System` namespace doesn't exist. Building a capability system for a namespace that doesn't exist is backwards. Add `System` first, then gate it. |
| K-58 | `@limits` — `max_turns` | `@limits { timeout, max_tokens, max_cost }` already implemented. Add `max_turns` (per-agent turn cap, interpreter-counted, feasible now). | **plan** | `max_turns` is interpreter-counted and cheap. `max_memory`/`max_cpu` need OS-level controls — too platform-specific for v0.1. Scope to `max_turns` only. |
| K-59 | URI-scheme `extern` — `mcp://` | `extern task search from "mcp://context7/query-docs"` — typed MCP tool calls. Real value is `mcp://` only. | **plan** | MCP is increasingly the standard for AI tool integration. Start small: just `mcp://` scheme, auto-discover signatures from MCP schema. Low implementation cost (extern already exists), high ecosystem leverage. |
| K-60 | Ambient `context` block | Thread-local propagation: `context { user_id: "123" } { run(MyAgent) }`. | **skip** | IDEAS.md Triage. Adds implicit data flow that's hard to debug and reason about. Passing `trace_id` through explicit task parameters is clearer and auditable. |

---

## Summary

| Category | Plan | Skip |
|----------|-----:|-----:|
| Language Design & Type System | 8 | 5 |
| Interface / Provider System | 3 | 0 |
| Agent Features | 2 | 5 |
| Observability & Debugging | 4 | 2 |
| Testing | 7 | 1 |
| Structural Refactoring | 6 | 0 |
| CI & Release Infrastructure | 5 | 2 |
| Documentation | 2 | 0 |
| Ecosystem & FFI | 3 | 5 |
| **Total** | **40** | **20** |

Sources consolidated: `agentscript_inspired.md`, `architecture.md`, `audit2.md`, `claude_codex_parity.md`, `critique.md`, `ecosystem_refined.md`, `general_design_critiques.md`, `ideas_old.md`, `ideas_refined.md`, `IDEAS.md`, `integration-test-migration.md`, `journal-codex.md`, `runtime-split-plan.md`.
