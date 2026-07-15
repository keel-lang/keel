# Changelog

All notable changes to Keel.

> **Alpha.** Keel is v0.1. Breaking changes are expected between 0.x releases. Do not build production systems on 0.x.

> **Doc-update rule.** Any feature or spec change — added, updated, or removed — must update the docs in the same release. At minimum: `docs/src/release-notes.md` plus every guide page in `docs/src/guide/` (and `docs/src/examples/`, `docs/src/cli/`, `docs/src/config/` where applicable) that the change touches. `SPEC.md` is part of this rule. A release is not shipped until `mdbook build` runs clean over the updated pages.

## [Unreleased]

%%TAGLINE%% update this line before releasing — one sentence summary of the release

### Changed

- **Dependency upgrades.** `rustyline` 15→18, `logos` 0.14→0.16, `colored` 2→3, `rusqlite` 0.32→0.40, `sha2` 0.10→0.11, `getrandom` 0.2→0.4, `async-native-tls` 0.5→0.6, plus a routine `cargo update` of compatible transitive versions. No user-facing behavior change; internal call sites (`getrandom::fill`, digest-to-hex formatting, a logos comment-token lint annotation) were updated to match each library's new API surface.
- **Removed `Value::to_display_string`.** All call sites now go through the `Display` impl on `Value` directly (`.to_string()`), which was already functionally identical. No user-facing behavior change.
- **`send`'s non-agent-target error path now has dedicated unit test coverage.** Unlike `delegate`, `send`'s first argument isn't validated by the type checker, so a non-agent value type-checks and only fails at runtime — that branch had no test anywhere in the suite. (`control.rs`, `json.rs`, and `io.rs` were also audited for the same gap; all three already had this covered via existing integration tests, so no changes were needed there.)
- **Formatter edge cases now have dedicated round-trip and idempotency tests.** Nested string interpolation (an interpolation slot whose expression is itself an interpolated string, one and two levels deep), rich enum pattern destructuring in `when` arms (`reply { to, tone } => ...`), every `Duration` unit plus canonical-name normalization of unit aliases (`500.millis` → `500.ms`), and deeply nested control-flow blocks (`for` → `if` → `while` → `if` → `for`) were previously only exercised incidentally through integration round-trips.
- **`docs/status/features.json` now lists every installed stdlib namespace and CLI command, and a new test enforces it stays that way.** `std/csv` and `std/testing` were fully shipped (methods installed, tested, and documented in `docs/src/guide/stdlib.md`) but absent from the JSON status source, and `keel test` was missing from the `cli` array — none of that was previously caught, because the existing drift tests only checked that JSON entries had matching docs rows, not the reverse. A new test, `feature_status_source_lists_every_catalog_namespace`, now fails if a namespace installed in `keel_catalog::catalog()` has no corresponding `docs/status/features.json` entry.

### Fixed

- **`schedule.cron` could stall the runtime on rare/impossible expressions.** Computing the next fire time scanned candidate minutes one at a time — up to ~4 years' worth — synchronously inside the async task driving the cron loop. A spec that fires rarely (or never, e.g. `"0 0 31 2 *"`, which asks for February 31st) blocked that task's worker thread for the full scan. The scan now runs via `tokio::task::spawn_blocking`, off the async worker pool; the matching logic itself is unchanged.

---

## [0.2.4] — 2026-06-21

Swappable LLM providers — `ai.*` runs on built-in Ollama, OpenAI, and Anthropic backends, or a provider you write in Keel.

### Added

- **User-authored LLM providers — write a backend in Keel.** For proprietary or self-hosted models the built-in backends don't cover, any field-less type with `impl LlmProvider` is now a backend `ai.*` can dispatch through. `ai.install(MyProvider)` registers it program-wide (the lowest-precedence default, below a `provider:` prefix and `@provider`); `@provider MyProvider` selects it per agent. `complete(self, req: CompletionRequest) -> str` returns the raw model output — Keel applies its own prompt construction and output parsing (enum matching, schema validation) identically to built-in and user providers, so `??`, `when`, and the typed `AiError`/`AiSchemaError` errors behave the same. `CompletionRequest` is a built-in struct (`system`, `user`, `model`, `max_tokens`). `ai.install(X)` and `@provider X` require `X` to implement `LlmProvider` (a compile-time error otherwise), and a provider that calls `ai.*` from inside its own `complete()` is rejected with an `AiError` rather than recursing without bound.

```keel
use std/ai
use std/env
use std/http

type MyProvider {}
impl LlmProvider for MyProvider {
  task complete(self, req: CompletionRequest) -> str {
    key = env.get("MY_LLM_KEY")!
    http.post("https://my-llm.example/complete", body: { prompt: req.user })["text"]
  }
}

ai.install(MyProvider)        # program-wide, or `@provider MyProvider` per agent

task ask(q: str) -> str {
  ai.prompt(system: "Be concise.", user: q) ?? "no answer"
}
```

- **Swappable LLM backends — OpenAI and Anthropic (Claude) join Ollama.** `ai.*` no longer hard-codes Ollama. Three backends ship built in and are selected with no extra code, most-specific wins: a `provider:` prefix on a model tag (`@model "anthropic:claude-opus-4-8"`, or a `using:` argument) picks the backend per call; `@provider <name>` sets an agent's default backend for bare tags; `KEEL_PROVIDER` sets the program default (otherwise `ollama`). OpenAI and Anthropic read `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` — a missing key throws `AiError { reason: "provider" }`, never a silent fallback. `@provider` accepts the built-in names `ollama`, `openai`, `anthropic` (and now user-authored provider types — see above); anything else is a compile-time error. `@limits { max_tokens }` is now threaded into the request as the generation cap.

```keel
use std/ai

agent Researcher {
  @provider anthropic
  @model "claude-opus-4-8"
  @limits { max_tokens: 1024 }

  on ask(question: str) {
    answer = ai.prompt(system: "You are concise.", user: question)
    # …
  }
}

# Per-call override via the model-tag prefix — no @provider needed:
task quick(q: str) -> str {
  ai.prompt(system: "Answer in one line.", user: q, using: "openai:gpt-4o") ?? "no answer"
}
```

### Changed

- **`impl` conformance now verifies parameter types, not just arity.** When a struct's `impl` block satisfies an `interface`, both `keel check` and `keel run` now require each method parameter to match the interface signature's type — previously only the *number* of parameters (and the return type) was checked, so a parameter could silently have the wrong type. `dynamic` in the interface's parameter position stays a wildcard (an `impl` may narrow it to a concrete type, which is how `Comparable`/`Equatable` accept `other: dynamic`); a concrete interface parameter type now requires an exact match. The check runs through the same shared `signature_satisfies` path as the return-type check, so the checker and the runtime always agree.

```keel
use std/io
interface Fetcher {
  task fetch(self, url: str) -> str
}
type Client { id: int }
impl Fetcher for Client {
  task fetch(self, url: int) -> str { "x" }   # error: parameter `url` must be `str` but is `int`
}
task run() { io.show("x") }
run()
```

  A program with a mismatched parameter type that previously slipped past `keel check` will now report a conformance error pointing at the offending parameter. Programs whose `impl` parameter types already matched the interface (including any that narrow a `dynamic` interface parameter to a concrete type) are unaffected.

---

## [0.2.3] — 2026-06-18


### Changed

- **`ai.*` provider failures throw `AiError` instead of returning `none`.** A real provider failure — Ollama unreachable, network error, or a model that isn't mapped — now *throws* a typed `AiError` carrying a machine-readable `reason` (`"unavailable"` for network/unreachable, `"provider"` for a config/mapping fault) rather than degrading to `none`. Previously these failures returned `none`, so `ai.classify(...) ?? Urgency.medium` quietly produced `medium` whether the model genuinely had no answer **or the model was down** — an outage was indistinguishable from a real classification. Now `none` strictly means *absence* (the model returned no answer, no model is configured, or mock mode), and `??` defaults apply only to that; failures surface with a reason you can catch and act on:

```keel
use std/ai
use std/io
type Urgency = low | medium | high

task triage(body: str) -> Urgency {
  try {
    ai.classify(body, as: Urgency) ?? Urgency.medium   # ?? handles a no-answer none
  } catch err: AiError {
    io.notify("classifier {err.reason}: {err.message}") # reason: "unavailable" | "provider"
    Urgency.medium
  }
}
```

  This makes the three failure causes individually catchable, which absence-as-`none` could not express: provider unavailable → `AiError` (`reason: "unavailable"`), output unparseable → `AiSchemaError` (carries `got`), and a call that exceeds its time budget → `TimeoutError` (unchanged, via `@limits timeout` / `control.with_timeout`). Mock mode (`KEEL_LLM=mock`) is unchanged in observable behavior: every `ai.*` call still returns `none`, so existing `?? default` tests and examples keep working — mock now models deterministic *absence* rather than a simulated failure. No `.keel` program that handled absence with `??`/`when` needs updating; programs relying on a network failure silently becoming `none` must now catch `AiError`.

- **`ai.prompt(..., response_format: "json")` now raises `AiSchemaError` on unparseable output.** A JSON-mode prompt whose reply isn't valid JSON previously surfaced as a generic `AiError`; it now raises `AiSchemaError` (carrying the raw `got`), matching `ai.classify` and `ai.extract`. Code that caught this case with `catch err: AiError` should catch `AiSchemaError` instead.

- **Email IMAP now uses the maintained `async-imap` client.** The runtime previously depended on `imap 2.4.1`, which transitively pinned `imap-proto 0.10.2` — a crate that emitted a future-incompatibility warning (`trailing semicolon in macro used in expression position`, [rust#79813](https://github.com/rust-lang/rust/issues/79813)) and is slated to become a hard error in a future Rust release. `Email.fetch` and `Email.archive` now run on `async-imap 0.11` (with `async-native-tls`), which tracks the current `imap-proto 0.16`, so the warning is gone. The IMAP calls now run natively on the async interpreter instead of `tokio::task::spawn_blocking`. No language-surface change: `Email.fetch`, `Email.send`, and `Email.archive` behave exactly as before.

### Fixed

- **`ai.translate(..., to: [])` raises a clear error instead of panicking.** An empty `to:` list slipped past the namespace and reached an unchecked index in the translator, panicking the interpreter when the model reply wasn't valid JSON. `ai.translate` now rejects an empty `to:` list up front with a catchable error: `ai.translate: `to:` must contain at least one language`.

- **`examples/email_agent.keel` and the README triage snippet no longer crash on real models.** Both guarded `ai.classify` with `??` alone — but `ai.classify` (and `ai.extract`) *raise* `AiSchemaError` when the model's output matches no enum variant, and `??` only rescues a `none` result. So the `?? Urgency.medium` fallback fired only in mock/no-model mode; against a real model an HTML-heavy newsletter (reproduced on both `gemma4` and `gpt-oss:20b`) raised `AiSchemaError` and aborted the whole agent run — the opposite of what the example implied. The `triage` task now wraps the call in `try/catch AiSchemaError`, so a single non-conforming email falls back and the batch continues:

```keel
task triage(email: { body: str, from: str, subject: str }) -> Urgency {
  try {
    ai.classify(email.body, as: Urgency) ?? Urgency.medium
  } catch err: AiSchemaError {
    Urgency.medium
  }
}
```

  The README hero example was additionally rewritten to valid v0.1 syntax (the removed PascalCase prelude `Ai.`/`Io.`/`Email.`, an unimplemented `fallback:` argument, and missing `use std/…` imports), and `docs/src/guide/ai-primitives.md` now documents that `??` does not catch `AiSchemaError`. Runtime behaviour is unchanged — schema mismatch still throws by design (see `SPEC.md §11.1`).

---

## [0.2.2] — 2026-06-17


### Added

- **`examples/trading_bot` — a typed, tested, AI-augmented paper-trading bot.** A multi-file example (indicators, strategy, risk, execution, synthetic + best-effort live market data) showing agents, `@tools` capabilities, cross-module types, `keel test` blocks, and `ai.decide` as a first-class primitive that degrades to a deterministic rule when no model is configured. Run it with `KEEL_LLM=mock KEEL_ONESHOT=1 keel run examples/trading_bot/main.keel` (or `KEEL_BOT_LIVE=1` to pull live Binance klines).

### Changed

- **`keel-lang` split into a layered crate workspace.** The single ~41.6k-LOC crate — the heaviest compile unit in the dependency graph, rebuilt on every change — is now five crates that build in parallel and recompile independently: `keel-syntax` (lexer/AST/parser/formatter/linter) → `keel-catalog` (stdlib method descriptors + capability metadata, a zero-dependency leaf) → `keel-compiler` (HIR/type checker/modules/IDE queries) → `keel-runtime` (interpreter + stdlib namespaces, owns the heavy I/O deps) → `keel-lang` (published facade + `keel` binary). Moving the stdlib catalog into the neutral `keel-catalog` leaf also breaks the old `types → runtime` dependency cycle. The embedding API is unchanged: `keel-lang` re-exports `ast`, `modules`, `catalog`, `diagnostics`, and `session` under their original paths, so external consumers need no changes. Editing the parser now rebuilds its tests in ~0.8s instead of inside the ~6.9s monolith, and parser edits no longer rebuild `rusqlite`/`reqwest`/`axum`.
- **Workspace-level metadata, lints, and dependencies.** Package `version`/`edition`/`license`, the clippy lint policy, and shared dependencies are hoisted to `[workspace.package]`, `[workspace.lints]`, and `[workspace.dependencies]` so they live in one place; `parse_source` and the stdlib `catalog_method` lookup are deduplicated into their owning crates.
- **Parser upgraded to chumsky 0.13.** `keel-syntax` was ported from chumsky 0.9 to the current stable 0.13 API (`Input`/`Stream` spanned input, `Rich` errors via `extra::Err`, lazy `repeated`/`separated_by` collectors, and the new `foldl`/`foldr` and `map_with` forms). Purely internal — the parser's public entry points and all parse and error-recovery behaviour are unchanged.

### Fixed

- **`as list[T]` / `as map[K, V]` / tuple casts now work at runtime.** The type checker accepted these casts but the interpreter only implemented scalar conversions (`int`/`float`/`str`/`bool`/`Uuid`) and raised `cannot cast to List(...)` for any container target. Narrowing a dynamic value — the common `json.parse(body) as list[dynamic]` path — therefore always failed at runtime. Container casts now assert the runtime shape and recurse element-wise, raising on a shape or element mismatch (never silently passing wrong-shaped data):

```keel
use std/json

rows = json.parse(body) as list[dynamic]   # array → list
for row in rows {
  cells = row as list[dynamic]             # nested array → list
  close = (cells[4] as str).to_float() ?? 0.0
}

cfg   = json.parse(text) as map[str, dynamic]   # object → map
point = json.parse("[1,2]") as (int, int)       # array → tuple

json.parse("42") as list[dynamic]   # raises: cannot cast int to list
```

  A `dynamic` element/value type (`list[dynamic]`, `map[str, dynamic]`) is a pass-through that only asserts the top-level shape. Map keys pass through unchanged (v0.1 does not coerce keys). Tuples are written with parentheses — `(int, int)`, not `tuple[...]`. `set[T]` remains unwritable as a cast target (`set` is a reserved keyword). This is what made the `examples/trading_bot` live market-data feed fall back to synthetic on every run; it now parses live klines.

### Build

- **CI cancels superseded runs.** The CI workflow now uses a `concurrency` group keyed by workflow and branch (PR `head_ref`, otherwise `ref`) with `cancel-in-progress`, so pushing a newer commit cancels the in-flight run for that branch or PR instead of queueing a duplicate.

### Documentation

- Fixed a broken rustdoc intra-doc link in `keel_lang::run` (`std::process::exit(130)` was not a resolvable item path).

---

## [0.2.1] — 2026-06-13

Struct pattern matching in `when`, and stricter validation of struct and enum pattern field names.

### Added

- **Struct pattern matching in `when`** — bind named fields from a struct value directly in `when` arms using `{ field1, field2 }` syntax. Combine with a `where` guard to route on field values:

```keel
when signal {
  { price, volume } where price > 1000.0 and volume > 0.0 => "active"
  { price }         where price > 1000.0                  => "thin"
  _                                                        => "quiet"
}
```

  An unguarded struct arm is total when the subject is a non-nullable struct — it matches any value of that struct type and satisfies exhaustiveness without requiring a separate `_` arm. Against an enum or other non-struct subject a struct arm never matches, so it does not satisfy exhaustiveness (a missing-variant or missing-`_` error is still reported); against a nullable struct the `none` case must still be covered. Field names are validated against the subject struct — a name the struct does not declare is a compile-time error, not a silent `none` binding. Struct patterns work in both statement and expression `when` forms. Field bindings are in scope for the `where` guard and the arm body.

### Fixed

- **Enum variant patterns now reject unknown field names.** Destructuring a field a variant does not declare — e.g. `reply { to, tpo }` where the variant has `tone`, or naming any field on a data-less variant like `low { x }` — previously type-checked and silently bound the misspelled name to `none`. It is now a compile-time error that names the offending field and variant.

---

## [0.2.0] — 2026-06-12

one module system for everything — `use std/file` for the stdlib, `use "./file.keel"` for your own code — with deny-by-default agent capabilities and built-in test blocks

---

### Added

- **Deny-by-default `@tools` capability gating.** Capabilities guard *effects*: the modules with authority over the world outside the process — `ai`, `io`, `http`, `email`, `file`, `shell`, `db`, `search`, `env` — now require a declared capability, and an agent with no `@tools` attribute may call none of them. Pure-compute and internal modules (`json`, `math`, `time`, `schedule`, …) are never gated. `@tools all` is the new explicit, greppable unrestricted form. Enforcement is two-layered — direct std calls in an agent body that `@tools` does not cover are now compile-time errors naming the fix, and calls reached through helper tasks raise `CapabilityError` at runtime with the same guidance (`add \`json\` to @tools, or use \`@tools all\``). Conditional entries (`email.send if self.confirmed`) count as declared and keep their per-turn runtime guard. Value methods, agent verbs, local module tasks, top-level statements, and `test` blocks remain ungated. **Breaking:** agents that relied on the old implicit-allow default must declare `@tools [...]` or `@tools all`. Closes [#69](https://github.com/keel-lang/keel/issues/69).

  ```keel
  use std/http
  use std/io

  agent Restricted {
    @tools [io, http.get if self.allow_http]
    state { allow_http: bool = false }

    @on_start {
      io.show("declared, not guessed")
      http.get("https://example.com")   # CapabilityError: guard is false
    }
  }
  ```

- **Module system.** Keel programs can now span multiple files, and the standard library is imported with the same syntax. `use std/file` binds `file`; `use "./validation.keel"` binds `validation` (the file stem); `as` renames either; `use A, B as C from "./m.keel"` (or `from std/json`) pulls symbols unqualified. Every top-level declaration is exported under the module namespace — tasks (`validation.email(...)`), agents (`run(watchers.Watcher)`), and via symbol import, types and interfaces. Imported files are parsed once, resolve relative to the importing file, and may not form cycles (the error spells out the path). Top-level statements are the **implicit main**: they run in order, sharing one scope, only when their file is the entry file — never on import. `keel test file.keel` runs only that file's tests; imported modules contribute declarations (test helpers are plain tasks), never their tests. The REPL pre-imports the whole stdlib. Closes [#66](https://github.com/keel-lang/keel/issues/66).

  ```keel
  use std/io
  use std/file
  use "./validation.keel"
  use Urgency from "./models.keel"

  task load_config() -> str {
    file.read("config.json")
  }

  # Implicit main — runs only when this file is executed directly.
  io.show("valid: {validation.email("ada@example.com")}")
  ```

- **Breaking: the ambient PascalCase prelude is removed.** `File.read(...)`, `Ai.classify(...)`, etc. now fail with a tombstone diagnostic — `` `File` is not ambient — add `use std/file` and write `file.read(...)` `` — never a bare "undefined identifier". The `Agent` namespace dissolved into built-in verbs (`run`, `stop`, `send`, `delegate`, `broadcast` — always in scope). `Uuid` split: the type stays built in, constructors moved to `std/uuid` (`uuid.v4()`), and the bare `uuid()` free function is gone. `@tools` lists use module names (`@tools [shell, http]`). One flat global namespace per program in this release: duplicate top-level names across modules and inconsistent import bindings are compile errors with rename/alias hints.

- Built-in Keel test blocks. `keel test <path>` now runs top-level `test "name" { ... }` blocks without executing top-level program statements, and directory paths recursively discover `.keel` files with test blocks. `--filter <text>` runs only tests whose names contain the filter text, `--list` prints matching tests without running them, `--fail-fast` stops after the first failing test, `--quiet` prints only failures and the final summary, and paths with no tests report `0 tests found`. Test results color `PASS` green and `FAIL` red when color output is enabled, with muted per-test elapsed time after the test name, source locations for failed statements when available, and total suite time in the final summary. Tests support `setup { ... }` blocks, parameterized `test "name" for case in cases { ... }` blocks, `use std/testing` plus repeated `testing.mock(module.method).returns(value)` sequences scoped to a single test, mock metadata via `module.method.called`, `module.method.call_count`, and `module.method.called_with(...)`, and `assert expr` statements checked as booleans by the type checker; `assert expr, "message"` provides a custom failure message. `test`, `setup`, and `assert` are contextual syntax words, not new reserved keywords. Closes [#53](https://github.com/keel-lang/keel/issues/53).

  ```keel
  use std/ai
  use std/testing

  type Severity = low | medium | critical

  task classify(text: str) -> Severity {
      ai.classify(text, as: Severity) ?? Severity.low
  }

  test "mocked classify returns critical" {
      testing.mock(ai.classify).returns(Severity.critical)
      assert classify("payment outage") == Severity.critical, "expected critical"
  }
  ```

### Changed

- Parser internals now share the `when` block wrapper between statement and expression contexts, keeping arm parsing rules consistent across both forms.

### Fixed

- Parser spans for null-coalesced `if` statements now include the `??` operator in the synthesized `IfExpr`, so LSP diagnostics and IDE lookups cover the full conditional expression.

## [0.1.33] — 2026-06-07

All stdlib errors are now typed — distinguish causes in try/catch blocks.

### Added

- **Typed runtime errors for all stdlib namespaces.** Every stdlib namespace that can raise a catchable error now produces a named error type rather than an unclassified string. `try/catch` can now match `FileError`, `CsvError`, `DbError`, `MathError`, `MemoryError`, `EmailError`, `HttpError`, `ShellError`, `JsonError`, `EnvError`, `AiError`, `AiSchemaError`, `CapabilityError`, `TimeoutError`, `DeadlineError`, and `UserRaised` specifically, and `catch err: Error` still works as the catch-all. `raise` now produces `UserRaised` (previously an untyped plain error). Closes [#20](https://github.com/keel-lang/keel/issues/20).

  ```keel
  agent A {
      @on_start {
          try {
              data = File.read("config.json")
          } catch e: FileError {
              Io.show("file error: {e.message}")
          }

          try {
              rows = Csv.parse_records("{}")
          } catch e: CsvError {
              Io.show("CSV error: {e.message}")
          }

          try {
              resp = Http.get("https://api.example.com")
          } catch e: HttpError {
              Io.show("HTTP error: {e.message}")
          }

          try {
              Control.with_timeout(5.seconds, () => { "ok" })
          } catch e: TimeoutError {
              Io.show("timeout error: {e.message}")
          }

          try {
              raise "quota exceeded"
          } catch e: UserRaised {
              Io.show("raised: {e.message}")
          }

          stop(self)
      }
  }
  run(A)
  ```

---

## [0.1.32] — 2026-06-06


### Added

- **mdBook preprocessor for auto-generated namespace reference tables.** A new `tools/mdbook-keel-catalog` binary crate implements the mdBook preprocessor protocol. Any `{{#catalog Ns}}` directive in a `docs/src/**/*.md` file is expanded at `mdbook build` time to a `| Method | Signature | Description |` table sourced directly from `prelude::catalog()`. Adding a new method to a namespace's `SPEC` now automatically appears in the built docs with no manual table editing. Hand-written tables for `Random`, `Crypto`, `Log`, `Search`, `Async`, `Json`, `Cache`, `Uuid` (static methods), and `File` have been replaced with directives. Also fixes `Cache.set` SPEC to declare the optional `ttl: duration` parameter that was already accepted at runtime but not reflected in the catalog. Closes [#29](https://github.com/keel-lang/keel/issues/29).

  ```keel
  # docs/src/guide/prelude.md — before
  | `Random.float()` | `() -> float` | `float` | Uniform in [0.0, 1.0) |
  | `Random.int(min:, max:)` | `(min: int, max: int) -> int` | ... |
  | `Random.bool()` | `() -> bool` | `bool` | 50/50 boolean |

  # after — directive is expanded by mdbook-keel-catalog at build time
  {{#catalog Random}}
  ```

### Fixed

- **LSP `rename` blocklist is now namespace-only and dynamically derived.** Previously a hardcoded list that drifted behind the catalog (missing `Math`, `Shell`, `Csv`), and the initial fix over-corrected by using `prelude_names()` — which also includes primitive types, symbol-hint words like `json`/`text`/`session`, and built-in interface names, causing user-defined tasks with those names to be silently un-renameable. The blocklist is now derived from `prelude::catalog()` namespace names only, so only actual namespaces (`Ai`, `Io`, `Http`, …) are protected; primitives and other identifiers are handled by a separate explicit list. Closes [#22](https://github.com/keel-lang/keel/issues/22).

- **LSP hover on primitive type annotations returns `type \`int\`` instead of `namespace \`int\``.** Hovering over `int`, `str`, `bool`, `float`, `none`, `datetime`, `duration`, `list`, `map`, `set`, or `dynamic` in a type annotation now correctly labels them as `type`, not `namespace`. The regression was introduced when `prelude_names()` (which includes primitive type names) was checked before the more specific primitive-type branch in both `type_at` and the semantic index name-map builder.

- **Parser now reports all syntax errors, not just the first.** When a source file contains syntax errors in multiple declarations, all of them are now surfaced — both in the LSP (as individual diagnostics) and in the CLI (as labeled spans in a single miette report). Previously, only the first error was collected and the rest were silently discarded. Implemented via declaration-level error recovery (`skip_then_retry_until`) in the program parser, plus updating `into_miette` to build one `LabeledSpan` per chumsky error. Fixes [#26](https://github.com/keel-lang/keel/issues/26).

  ```keel
  task greet(name: str) {
    result =       # ← error 1 (incomplete expression)
  }

  task farewell(name: str) {
    reply =        # ← error 2 (incomplete expression) — now visible too
  }
  ```

- **`else if` chains now parse correctly at statement position.** Previously, writing `if cond { } else if cond2 { }` as a top-level statement produced a parse error ("found 'if' but expected '{'"). The statement-form `if` parser was not recursive, so only expression-form `if` supported `else if` chaining. Both forms now share the same `if_body` combinator in `common.rs` and are backed by a `recursive()` frame, making `else if` valid in all positions. Also deduplicates `when` arm and block grammar — `pattern`, `when_arm`, `block_with`, and `if_body` are now each defined once in `parser/common.rs` and called by both statement and expression parsers. Closes [#13](https://github.com/keel-lang/keel/issues/13).

  ```keel
  task classify(score: float) -> str {
      if score > 0.8 {
          "high"
      } else if score > 0.5 {
          "medium"
      } else {
          "low"
      }
  }
  ```

---

## [0.1.31] — 2026-06-04

Replace unbounded event queue with bounded channel and overflow policies.

### Changed

- **Interpreter event queue is now bounded.** The event channel was previously unbounded (`tokio::sync::mpsc::unbounded_channel`), meaning scheduler ticks, HTTP requests, and `Agent.send` calls could grow memory without limit under sustained load. The queue is now bounded (default 1024; configurable via `KEEL_EVENT_QUEUE_CAPACITY`). Each producer uses non-blocking `try_send` so the event loop is never directly back-pressured:
  - **Recurring scheduler ticks** (`Schedule.every`, `Schedule.cron`) — drop on overflow (coalesce). A skipped tick is harmless; the next tick fires on time.
  - **One-shot schedulers** (`Schedule.after`, `Schedule.at`) and the initial fire of `Schedule.every` — wait for queue space rather than dropping. Delivery is guaranteed as long as the event loop is still running.
  - **HTTP requests** (`Http.serve`) — return HTTP 503 to the caller when the queue is full.
  - **Agent dispatch** (`Agent.send`, `Agent.delegate`, `Agent.broadcast`) — raise a catchable `RuntimeBusy` error.

  `RuntimeBusy` is a structured error catchable in Keel `catch` clauses:

  ```keel
  try {
      Agent.send(Worker, payload)
  } catch e: RuntimeBusy {
      Io.show("queue full — dropped: {e.message}")
  }
  ```

  Set `KEEL_EVENT_QUEUE_CAPACITY=<n>` to tune the limit. The 1024 default is sufficient for typical agent workloads; lower values are useful in tests to trigger backpressure deliberately.

---

## [0.1.30] — 2026-06-03

Semantic analysis now lowers through HIR while runtime APIs enforce declared inputs.

### Changed

- **Interpreter event queue is now bounded.** The event channel was previously unbounded (`tokio::sync::mpsc::unbounded_channel`), meaning scheduler ticks, HTTP requests, and `Agent.send` calls could grow memory without limit under sustained load. The queue is now bounded (default 1024; configurable via `KEEL_EVENT_QUEUE_CAPACITY`). Each producer uses non-blocking `try_send` so the event loop is never directly back-pressured:
  - **Recurring scheduler ticks** (`Schedule.every`, `Schedule.cron`) — drop on overflow (coalesce). A skipped tick is harmless; the next tick fires on time.
  - **One-shot schedulers** (`Schedule.after`, `Schedule.at`) and the initial fire of `Schedule.every` — wait for queue space rather than dropping. Delivery is guaranteed as long as the event loop is still running.
  - **HTTP requests** (`Http.serve`) — return HTTP 503 to the caller when the queue is full.
  - **Agent dispatch** (`Agent.send`, `Agent.delegate`, `Agent.broadcast`) — raise a catchable `RuntimeBusy` error.

  `RuntimeBusy` is a structured error catchable in Keel `catch` clauses:

  ```keel
  try {
      Agent.send(Worker, payload)
  } catch e: RuntimeBusy {
      Io.show("queue full — dropped: {e.message}")
  }
  ```

  Set `KEEL_EVENT_QUEUE_CAPACITY=<n>` to tune the limit. The 1024 default is sufficient for typical agent workloads; lower values are useful in tests to trigger backpressure deliberately.

- **Named struct types are now nominally distinct.** Two declared struct types `A` and `B` with identical fields are no longer interchangeable. The checker raises a type error when a value of type `A` is passed where `B` is expected. Anonymous struct literals `{ x: 1, y: 2 }` remain structurally compatible with any named type that has the required fields, so existing patterns like `p: Point = { x: 1, y: 2 }` are unaffected.

  `impl` dispatch is now based entirely on the value's type tag. List elements are promoted to their declared struct type when assigned to a `list[TypeName]` variable — use an explicit annotation to enable `impl` method dispatch on struct collections:

  ```keel
  type Score { val: int }
  impl Comparable for Score {
    task compare(self, other: Score) -> int { self.val - other.val }
  }
  task run() {
    scores: list[Score] = [{ val: 30 }, { val: 10 }, { val: 20 }]
    sorted = scores.sort()   # uses Comparable.compare via type tag
  }
  ```

  Error messages for named struct type mismatches now include the declared type name (`expected Score, got Point`) rather than the generic `struct`.

- **Runtime namespace closures are now decoupled from the concrete interpreter.** A new `Host` trait abstracts the interpreter capabilities used by built-in namespaces (runtime backends, agent lifecycle, closure dispatch, type registries). All 23 prelude namespaces receive `&mut dyn Host` instead of `&mut Interpreter`, making each namespace independently testable and opening the door to sandboxed agents and alternate execution backends. A `MockHost` test double is gated behind the `test-util` feature for downstream testing. No user-visible language behaviour changes.

- **Checker and LSP now consume a read-only HIR index.** Parsing now lowers into a high-level intermediate representation before semantic analysis. HIR assigns `SymbolId`s to declarations and binding sites, records resolved identifier references (including `self.task(...)` and `self.field` reads/writes) for editor navigation, and classifies brace literals as structs or maps once using their expected type when available. The interpreter intentionally remains AST-backed in this first phase. This internal boundary prevents checker and LSP logic from independently re-resolving syntax as future execution backends are added.

- **Type-checker diagnostics are now structured internally.** The checker returns `TypeDiagnostic` variants instead of a string-only `TypeError`, with structured data for undefined names, type mismatches, wrong arity, and non-exhaustive `when` checks. CLI and LSP rendering keep the same user-facing messages, while diagnostics now carry expected/actual types and precise spans for IDE tooling.

### Changed

- **`TypeExpr::SelfType` replaces the `"__impl_self__"` string sentinel.** The `self` receiver parameter in `interface` and `impl` method signatures is now represented by a typed `TypeExpr::SelfType` AST variant instead of a `TypeExpr::Named("__impl_self__")` string. All six exhaustive `TypeExpr` match sites (formatter, resolver, interface conformance, AST visitor, and two display helpers) have been updated. No user-visible behaviour changes.

- **`ExprFlow` replaces `Value::EarlyReturn` for control-flow inside `eval_expr`.** Returning from inside an expression-position `if`/`when` body (e.g. `x = if cond { return 5 } else { 0 }`) now propagates through a dedicated `ExprFlow { Value(Value), Return(Value) }` type instead of a sentinel variant on the `Value` enum. `eval_expr` returns `Result<ExprFlow>`. The old in-band propagation silently dropped early returns that occurred inside list literals, tuple literals, enum variant field expressions, call arguments, and other expression arms where the `EarlyReturn` check was missing. Those paths now propagate correctly via `?`.

- **Struct/map disambiguation is consolidated in HIR lowering.** Confirmed: the type checker's `Expr::StructLit` arm reads `hir.literal_kind()` exclusively, with no independent re-classification. HIR lowering is the single site where a brace literal is classified as a struct record or a map. (First established by the HIR commit; this release closes issue #15 by verifying the invariant holds.)

### Fixed

- **`return` inside a list literal now propagates out of the enclosing task.** Previously, a `return` occurring as a sub-expression in a list literal (e.g. `nums = [1, if flag { return 42 } else { 0 }, 3]`) was stored as a stray value inside the list instead of exiting the task. The fix is a consequence of replacing `Value::EarlyReturn` with `ExprFlow` in `eval_expr`: every expression arm now propagates early returns via `?`.

- **Typed runtime errors now propagate as structured values.** `AiError` and `AiSchemaError` were stored in a mutable `Interpreter` side-channel that `try/catch` read to reconstruct the caught value. A nested `try/catch` whose inner clause did not match would clear that slot before the outer clause could read it, so the outer catch lost its typed error. Typed errors now travel inside the runtime report itself, so each catch clause receives the exact error value produced by its failing call regardless of nesting. A regression test confirms a typed error survives propagation past a non-matching inner catch, and unit tests assert the typed payload is carried in the report (`downcast_ref::<RuntimeError>`) rather than a separate field.

- **Interpolation diagnostics now point at the correct source range.** Expressions inside string interpolation slots were re-lexed from byte offset `0`, so type-checker errors such as an undefined name underlined the wrong part of the file. Slot token spans are now rebased to their absolute `.keel` source positions, including nested and triple-quoted strings.

- **Runtime APIs now enforce their declared arguments.** Older namespace methods such as `File.read`, `Cache.get`, `Json.parse`, and `Shell.run` passed values through display formatting, so dynamic calls could silently turn `42` into `"42"`. Required sleep and value-method arguments could also be omitted and silently treated as no-ops or empty strings. Shared runtime argument decoders now reject wrong or missing inputs with clear errors; display coercion remains available only for presentation-oriented APIs such as interpolation, `Io.*`, and `Log.*`.

- **Runtime errors now carry stable machine-readable diagnostic codes.** `AiError`, `AiSchemaError`, and `FileError` are classified by a new `RuntimeErrorKind` enum. `miette::Diagnostic::code()` returns a stable code of the form `keel::runtime::<TypeName>` (e.g. `keel::runtime::FileError`) which CLI renderers and future host integrations can inspect without parsing the error message. The code appears in the CLI output when a typed error propagates uncaught:

  ```
  Error: keel::runtime::FileError

    × FileError: File.read `config.json`: No such file or directory (os error 2)
  ```

  **`FileError` is now a typed runtime error** — `File.*` failures are now catchable by type name (`catch e: FileError`), matching how `AiError` and `AiSchemaError` have always worked. `catch e: Error` continues to catch all failures as before. `AiError` and `AiSchemaError` catch behaviour and error messages are unchanged.

---

## [0.1.30] — 2026-05-29


### Changed

- **AST migrated from `Spanned<T>` tuples to `Node<T>` structs (Stage 5).** Every `(T, Span)` tuple in the public AST has been replaced by `Node<T>` — a named struct with `.kind` (the wrapped value) and `.span` (the source range). The old `type Spanned<T> = (T, Span)` alias is now only used inside `src/lexer.rs` for parser-internal token pairing. All consumer code in the formatter, type checker, interpreter, runtime namespaces, IDE hover/symbols, lint, repl, and all tests has been updated to use `.kind` and `.span` field access instead of positional tuple indexing. `Node::synthetic(kind)` produces nodes with a `0..0` sentinel span for prelude builtins and test helpers. This is an internal change; no Keel source behaviour is affected. It makes AST traversal code uniform and self-documenting, and eliminates the risk of accidentally swapping `.0` (the value) and `.1` (the span).

  ```keel
  task greet(name: str) -> str { name }
  # param `name` carries Node<TypeExpr>{ kind: Named("str"), span: 11..14 }
  # instead of the old tuple (Named("str"), 11..14)
  ```

- **Expression spans and annotation-precise type-mismatch diagnostics (Stage 4).** Every expression in the AST now carries its source byte range: `SpannedExpr = Node<Expr>` is used throughout the parser, type checker, formatter, interpreter, and visitor. Previously all expressions were bare `Expr` values and diagnostics pointed at the enclosing statement span. Now, when a `let` binding has an explicit type annotation and the inferred type does not match, the error caret points at the annotation itself — e.g. `x: str = 1` underlines `str`, not the whole statement. All internal APIs (`infer_expr`, `eval_expr`, `expr_str`, `visit_expr`, `walk_expr`) were updated to accept `&SpannedExpr`; test helpers use `Node::synthetic(expr)` for synthetic AST nodes.

  ```keel
  task go() -> str {
    n: int = "hello"   # error caret points at `int`, not the whole line
  }
  ```

- **Type annotations carry source spans (Stage 3).** All type-annotation use-sites in the AST now store `Node<TypeExpr>` rather than a bare `TypeExpr`. Affected fields: `Param.ty`, `TaskDecl.return_type`, `TaskSig.return_type`, `ExternDecl.return_type`, `StateField.ty`, `Field.ty`, `TypeDef::Alias`, `Stmt::Let { ty }`, `CatchClause.ty`, `Expr::Cast { ty }`, and `LambdaParam.ty`. The parser emits precise byte ranges for every annotation via `map_with_span`. Inner recursive `TypeExpr` children (e.g. the element type inside `list[T]`) are not wrapped — only the outermost annotation at each declaration/statement site is spanned. Synthetic AST nodes produced by the prelude catalog, `state.rs` builtins, and test helpers use `Node::synthetic(...)` with a `0..0` sentinel span. This is an internal change: no Keel source behaviour is affected. It unblocks "type mismatch" diagnostics that can point the caret at the exact annotation.

- **AST declaration names now carry source spans (Stage 2).** `TaskDecl`, `AgentDecl`, `TypeDecl`, `InterfaceDecl`, `ExternDecl`, `TaskSig`, and `Param` each gain a `name_span: Span` field storing the exact byte range of their name token in the source. The parser emits these spans via `.map_with_span()` instead of dropping them. `ide::symbols::definition_of` now walks the parsed AST to return the stored `name_span` rather than re-scanning tokens on every request, and falls back to token scanning only when the source fails to parse (e.g. mid-edit in the LSP). Internal change; no user-visible behaviour difference. Unblocks expression-level diagnostics, LSP hover, and rename in future stages.

- **Name resolution extracted into a dedicated pass.** A new `types::resolve` module defines `ResolvedName` (variants: `TopTask`, `Agent`, `Enum`, `TypeName`, `PreludeNamespace`, `Unresolved`) and a standalone `build()` function that compiles the checker's declaration tables into a `NameIndex`. The `Checker::infer_expr` method now consults this index for every `Expr::Ident`, `Expr::FieldAccess` LHS, `Expr::MethodCall` receiver, and `Expr::Call` callee instead of performing ad-hoc string comparisons against `top_tasks`, `agents`, `enum_variants`, `structs`, `aliases`, and `prelude` independently. Adding a new prelude namespace now requires only a catalog entry — `infer_expr` needs no modification. Internal change; no user-visible behaviour difference.

- **`Ty::Unknown` split into four semantically distinct variants.** The bare `Unknown` variant has been replaced with `Unknown(UnknownReason)`, `Error`, `Unresolved(String)`, and the already-existing `Dynamic`. Each variant carries a precise meaning:

  | Variant | When produced | Strict-mode warning? |
  |---|---|---|
  | `Dynamic` | User-written `dynamic` annotation | Never |
  | `Unknown(ExternalDynamic)` | Namespace method whose return type depends on runtime input (LLM outputs, `Json.parse`, etc.) | Yes |
  | `Unknown(InferenceLimitation)` | Checker cannot cheaply infer the type (agent refs, unannotated lambdas, shallow dispatch fallthrough) | Yes |
  | `Unknown(UnsupportedFeature)` | Construct the checker does not yet implement | Yes |
  | `Error` | An error was already emitted at this site; suppresses cascade diagnostics | No |
  | `Unresolved(name)` | A type name was used but never declared | No |

  **User-visible change:** `keel check --strict` no longer warns on `dynamic`-annotated bindings; it only warns on `Unknown(_)` results. `let x: dynamic = Json.parse("{}")` is now always clean in strict mode; `let x = Json.parse("{}")` (unannotated) still fires the "cannot infer type of `x`" diagnostic. `Json.parse` now returns `Unknown(ExternalDynamic)` rather than `Dynamic`, so unannotated bindings are still flagged.

  Internally, the new `Ty::is_opaque()` helper replaces scattered `matches!(t, Ty::Unknown | Ty::Dynamic)` patterns across checker, binop, resolve, and IDE hover modules.

- **Single prelude catalog for checker, LSP, and docs.** All built-in namespace methods (Ai, Io, File, Http, Cache, Csv, Db, Math, Crypto, Uuid, and every other stdlib namespace — 23 in total) are now defined in one `prelude::catalog()` slice. The checker derives its return-type inference from the catalog; the LSP derives its completion list from the catalog. Prior to this change, each surface maintained an independent hand-written list that drifted silently on every new method. A `catalog_covers_all_runtime_namespaces` consistency test enforces that the runtime and the catalog remain in sync.

  This change also **improves type inference** for methods that previously returned `Unknown` — `Memory.recall`, `Log.level`, `Shell.run`, `Email.send`, `Email.archive`, `Schedule.sleep`, and every `Log.*` method now infer their correct return types.

  **Breaking:** `memory_agent.keel` example updated — `Memory.recall` now correctly types as `str?`, so the implicit `str + int` in the counter example is replaced with an explicit `prev.to_int() + 1`. Any user code that added an integer to a `Memory.recall` result without conversion will now produce a checker type error; fix by calling `.to_int()` on the recalled string.

### Fixed

- **Interface conformance is now checked identically by `keel check` and `keel run`.** The checker and runtime maintained separate `TypeExpr`-to-string helpers that had already diverged: the checker collapsed `Struct` and `Generic` return types to the string `"unknown"` and accepted anything, while the runtime stringified them fully and could reject programs the checker had passed. Both phases now delegate to a single typed `signature_satisfies` function in the new `types::interface` module. `Struct` and `Generic` return types require an exact structural match; `dynamic` remains the explicit wildcard. Type aliases are resolved before comparison so `type Timestamp = datetime` and `datetime` are treated as the same type.

- **Declaration spans are now accurate.** Every top-level declaration (`task`, `agent`, `type`, `interface`, `impl`, `use`) now stores a real source byte span instead of the `0..0` placeholder that was hard-coded in `program_parser()`. Diagnostic tools and future lint passes can now point errors at the correct source location.

### Added

- **Type-safe `Agent.delegate` symbol form.** Handler references are now first-class expressions: `Agent.delegate(Worker.process, payload)` resolves `Worker.process` at compile time. The type checker validates that (1) `Worker` is a declared agent, (2) `process` is a declared `on` handler on that agent, and (3) `payload` matches the handler's declared parameter type when one is present. A misspelled handler name — `Agent.delegate(Worker.typo, data)` — is a compile-time error, not a silent runtime no-op.

  The existing string form `Agent.delegate(Worker, "process", payload)` remains valid; it is now also validated for plain string literals, closing the same class of rename-blindness bug in existing code. Both forms are accepted indefinitely for backward compatibility. Prefer the symbol form in new code.

  ```keel
  agent Worker {
    on process(task: Task) {
      Log.info("processing {task.id}")
    }
  }

  agent Boss {
    @on_start {
      Agent.run(Worker)
      Agent.delegate(Worker.process, my_task)   # ✓ compile-time–checked
    }
  }
  ```

---

## [0.1.29] — 2026-05-27


### Added

- **`Csv` namespace — CSV serialization.** Three functions cover the full round-trip: `Csv.parse(text)` returns `list[list[str]]` (every row as a list of strings); `Csv.parse_records(text)` returns `list[map[str, str]]` using the first row as header keys; `Csv.stringify(rows)` converts `list[list[str]]` back to RFC 4180–compliant CSV text. Cells containing commas, quotes, or newlines are automatically quoted. No `@tools` annotation required.

  ```keel
  raw    = "symbol,price,volume\nBTC,67000,1234.5\nETH,3500,5678.9"
  trades = Csv.parse_records(raw)
  for trade in trades {
      Log.info("{trade["symbol"]} @ {trade["price"] as float:.2f}")
  }

  rows = [["symbol", "price"], ["BTC", "67000"], ["ETH", "3500"]]
  text = Csv.stringify(rows)
  ```

- **`Db` namespace — SQLite-backed durable storage.** `Db.connect(url)` opens a SQLite database and returns a connection value; `.query(sql, params?)` runs a SELECT and returns `list[map[str, dynamic]]`; `.exec(sql, params?)` runs INSERT/UPDATE/DELETE and returns the rows-affected count. Connection URLs use `sqlite://` prefix (`sqlite://trades.db`, `sqlite:///abs/path.db`, `sqlite://:memory:`). Parameterized queries use `?` placeholders with a list of values. SQLite is bundled into the binary — no system library required. Other schemes (`postgres://`, `mysql://`) raise a clear "only sqlite:// is supported in v0.1" error.

  ```keel
  db = Db.connect("sqlite://trades.db")

  db.exec("CREATE TABLE IF NOT EXISTS trades (id TEXT, symbol TEXT, price REAL)")
  db.exec("INSERT INTO trades VALUES (?, ?, ?)", ["t1", "BTCUSDT", 67000.0])

  rows = db.query("SELECT symbol, price FROM trades WHERE symbol = ?", ["BTCUSDT"])
  for row in rows {
      Log.info("{row["symbol"]} @ {row["price"] as float:.2f}")
  }
  ```

---

## [0.1.28] — 2026-05-26

Struct spread-update, typed map keys, Shell/Math namespaces, and string format specifiers

### Added

- **Struct spread-update expression `{ ...base, field: new }`.** Copies all fields from a base struct or map value and overrides the specified fields. The base must appear first, preceded by `...`; zero or more `field: value` overrides follow. For struct bases the type tag is preserved (`impl` dispatch continues to work) and unknown override fields are a compile-time error (also enforced at runtime on dynamic paths). For `map[K, V]` bases any key may be added or overridden freely; override values must match the map's value type. Duplicate overrides are a compile-time error.

  ```keel
  type Order { id: str, status: str, amount: float }

  o: Order = { id: "ord-1", status: "pending", amount: 9.99 }
  filled   = { ...o, status: "filled" }   # id and amount unchanged
  copy     = { ...o }                     # full copy

  type Config { host: str, port: int, debug: bool }
  base: Config = { host: "localhost", port: 8080, debug: false }
  prod = { ...base, host: "api.example.com" }
  ```

- **`Shell` namespace — subprocess bridge.** `Shell.run(cmd, stdin:?, cwd:?)` executes a shell command and returns `{ stdout: str, stderr: str, exit_code: int }`. The command is passed to `/bin/sh -c`, so pipes and redirects work as expected. Spawn failures raise; a non-zero exit code is returned in the struct and is not itself an error. Gated by `@tools [Shell]`. The subprocess runs with an isolated environment: only `PATH`, `HOME`, `SHELL`, `TMPDIR`, `USER`, and `LANG` are forwarded; secrets and other process-level variables are not exposed.

  ```keel
  agent DataPipeline {
      @tools [Shell]

      @on_start {
          r = Shell.run("wc -l < data/records.csv")
          Io.show("record count: {r.stdout.strip()}")

          out = Shell.run("python3 transform.py", cwd: "scripts")
          if out.exit_code != 0 {
              raise "transform failed: {out.stderr}"
          }

          Shell.run("echo hello world", stdin: "unused\n")
      }
  }
  run(DataPipeline)
  ```

- **`Math` namespace.** Transcendental and power functions: `sqrt`, `pow`, `exp`, `log` (natural), `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`. Constants `Math.PI()` and `Math.E()`. All functions accept `int` or `float` and return `float`. Domain errors (e.g. `Math.sqrt(-1)`, `Math.log(0)`) raise at runtime.

  ```keel
  h   = Math.sqrt(Math.pow(3, 2) + Math.pow(4, 2))   # 5.0
  deg = 45.0
  s   = Math.sin(deg * Math.PI() / 180.0)             # ≈ 0.707
  ```

- **`Time.epoch_ms()`.** Returns the current Unix timestamp as an `int` in milliseconds. Useful for JS interop (`Date.now()` equivalence), database `BIGINT` columns, and signed payloads where a raw numeric timestamp is required.

  ```keel
  ms = Time.epoch_ms()   # e.g. 1705314600500
  ```

- **`as T` coercions and `typeof()`.** `expr as T` now performs real runtime coercions instead of being a no-op. Supported: `int ↔ float` (truncates toward zero), `int`/`float`/`bool → str`, `str → int`/`float`/`bool` (raises on invalid input), `none → any` (raises), `dynamic` pass-through for `Ai.prompt`/`Json.parse` narrowing. A new prelude function `typeof(x) -> str` returns the runtime type name — `"int"`, `"float"`, `"str"`, `"bool"`, `"none"`, `"list"`, `"map"`, `"duration"`, `"Uuid"`, or the declared struct/enum name.

  ```keel
  1 as float          # 1.0
  1.7 as int          # 1  (truncated)
  42 as str           # "42"
  "3.14" as float     # 3.14
  "abc" as int        # raises: cannot cast "abc" to int

  type Signal = buy | sell | hold
  s: Signal = Signal.buy
  typeof(s)           # "Signal"
  typeof(42)          # "int"
  typeof("hello")     # "str"
  ```

- **Typed map keys — `map[K, V]` now enforces hashable key types at compile time.** Map keys must be `str`, `int`, or `bool`. Using `float` as a key type is a compile-time error (NaN violates hash equality). Nullable key types (`str?`) are rejected. Struct and enum keys are not supported in v0.1; they are deferred to v0.2 behind `interface Hashable`. The runtime map representation changed from string-only keys to a `MapKey` union, so `map[int, str]` and `map[bool, str]` maps work correctly at runtime.

  ```keel
  # Valid key types
  scores:   map[str,  int]  = {alice: 100, bob: 95}
  lookup:   map[int,  str]  = {1: "one", 2: "two"}
  flags:    map[bool, str]  = {true: "on", false: "off"}

  # Compile-time errors
  # bad1: map[float, str]  = {}    # float is not a valid map key type
  # bad2: map[str?,  int]  = {}    # nullable type cannot be a map key
  ```

- **`list.sort(by: key_fn)` — sort with a key function.** The existing `.sort()` now accepts an optional `by:` named argument, consistent with the `min(by:)` / `max(by:)` prelude pattern. Sort any list by a computed key without needing `impl Comparable`. The key function must return an `int`, `float`, or `str`. Ascending only; descending is achieved by negating numeric keys.

  ```keel
  type Product { name: str, price: float }

  products = [
    { name: "Widget",  price: 9.99 },
    { name: "Gadget",  price: 24.99 },
    { name: "Doohickey", price: 4.99 },
  ]

  by_price = products.sort(by: p => p.price)          # cheapest first
  by_exp   = products.sort(by: p => 0.0 - p.price)   # most expensive first
  by_name  = products.sort(by: p => p.name)           # alphabetical
  ```

- **String interpolation format specifiers.** Slots now accept a Python-style format spec after a colon: `{expr:spec}`. Supported forms:

  ```keel
  pi = 3.14159
  Io.show("{pi:.2f}")       # → "3.14"
  Io.show("{pi:>10.2f}")    # → "      3.14"
  Io.show("{"hi":<8}!")     # → "hi      !"
  Io.show("{"hi":^8}")      # → "   hi   "
  n = 42
  Io.show("{n:.2f}")        # → "42.00"  (int auto-promoted to float)
  Io.show("{n:6}")          # → "    42" (bare width = right-align)
  ```

  All specs may combine alignment (`<`, `>`, `^`) with a width and/or precision (`.Nf`). Named arguments inside the slot (`{f(key: v):>10}`) are not confused with the spec separator because the colon is only treated as a spec delimiter at outermost bracket depth. A malformed spec is a runtime error.

### Fixed

- **Map subscript `m["key"]` now works at runtime and passes the type checker.** `Expr::Index` was missing a `Value::Map` arm in the interpreter; the type checker also only accepted `int` indices. Both are fixed: `map[K, V][key]` returns `V?` (the nullable value type, since a missing key returns `none`), and the key is type-checked against `K`.

  ```keel
  scores: map[str, int] = {alice: 90, bob: 85}
  a    = scores["alice"]   # 90
  miss = scores["nobody"]  # none
  ```

- **`type` declarations with `map[K, V]` fields now validate the key type at compile time.** `collect_type_decl` was calling `resolve_type` (which skips map-key validation) instead of `resolve_and_check_type`. A type alias or struct field like `scores: map[float, str]` now correctly produces a compile-time error.

- **Duplicate keys in `{ ...m, k: v, k: w }` map spread-update are now a compile-time error.** The struct path already checked for duplicate overrides; the map path was missing the same guard.

- **`@limits { ...DEFAULT, max_cost: 1.0 }` no longer silently drops the base fields.** `agent_limits()` was matching `StructSpreadUpdate` but only reading the `overrides` slice, discarding any values in the base `StructLit`. Base fields are now processed first; overrides then replace them, matching the documented precedence.

- **`map[int, V]` and `map[bool, V]` literals now parse correctly.** The parser only accepted identifier and string keys, making integer and boolean keys unreachable despite the type checker and runtime supporting them. `map_key()` now also accepts `Token::Integer` and `Token::True`/`Token::False`, and `Expr::StructLit` carries a `MapLitKey` enum (`Ident | Str | Int | Bool`) instead of a plain `String`. The CHANGELOG examples in v0.1.27 (`{1: "one"}`, `{true: "on"}`) now actually parse and run.

  ```keel
  lookup: map[int,  str] = {1: "one", 2: "two"}
  flags:  map[bool, str] = {true: "on", false: "off"}

  v = lookup[1]       # "one"
  w = flags[true]     # "on"
  ```

---

## [0.1.27] — 2026-05-21


### Added

- **User-defined interfaces and `impl` blocks.** Any user can now declare a named interface and implement it for a struct type. The compiler validates conformance — missing methods, wrong arity, and wrong return types are all compile-time errors:

  ```keel
  interface Printable {
    task print(self) -> str
  }

  type Point { x: float, y: float }

  impl Printable for Point {
    task print(self) -> str { "({self.x}, {self.y})" }
  }

  p: Point = { x: 1.5, y: 2.0 }
  Io.show(p.print())    # → "(1.5, 2.0)"
  ```

  Interfaces and their `impl` blocks can appear in any order in the same file. Impl methods take priority over built-in map methods (e.g. a user-defined `size()` on a struct wins over the generic map `.size()` length accessor).

- **`impl Interface for Type` blocks — user-defined `Stringable`.** Any user-defined struct type can now participate in string interpolation by implementing the built-in `Stringable` interface. The `impl` block is explicit and unambiguous — a task named `to_str` elsewhere is not automatically a Stringable implementation:

  ```keel
  type Color { r: int, g: int, b: int }

  impl Stringable for Color {
    task to_str(self) -> str { "rgb({self.r}, {self.g}, {self.b})" }
  }

  c: Color = { r: 255, g: 128, b: 0 }
  Io.show("color: {c}")    # → "color: rgb(255, 128, 0)"
  ```

  `impl` is a reserved keyword. `self` inside an impl body receives the struct value. String interpolation calls `to_str` via the registered impl; values without a `Stringable` impl fall back to their default display representation.

- **Four new built-in interfaces: `Comparable`, `Equatable`, `Serializable`, `Iterable`.** Each is pre-declared by the runtime (cannot be redeclared) and wired into the standard library:

  - **`Comparable`** — `task compare(self, other: T) -> int`. Negative/zero/positive return convention. Wired into `list.sort()`, `list.min()`, `list.max()`, and the global `min()` / `max()` functions.
  - **`Equatable`** — `task equals(self, other: T) -> bool`. Method-only; `==` remains structural comparison.
  - **`Serializable`** — `task to_json(self) -> str`. `Json.stringify(value)` calls `to_json` instead of the default serializer when the type implements this interface.
  - **`Iterable`** — `task items(self) -> list[T]`. Allows a struct to appear in a `for` loop. Not a generator — `items()` materialises the full list before iteration.

  ```keel
  type Score { val: int }
  impl Comparable for Score {
    task compare(self, other: Score) -> int { self.val - other.val }
  }
  items = [{ val: 30 }, { val: 10 }, { val: 20 }]
  Io.show("{items.sort()}")   # sorted ascending by val

  type Range { lo: int, hi: int }
  impl Iterable for Range {
    task items(self) -> list[int] {
      result: list[int] = []
      i = self.lo
      while i <= self.hi {
      result += [i]
      i += 1
    }
      result
    }
  }
  r: Range = { lo: 1, hi: 3 }
  for n in r { Io.show("{n}") }   # → 1, 2, 3
  ```

  The `Iterable` conformance check accepts any concrete `list[T]` as the return type (not just `list[dynamic]`).

- **`while` loops.** Unbounded iteration is now supported. The condition must be `bool`;
  `break` and `continue` work identically to their `for`-loop counterparts:

  ```keel
  n = 5
  while n > 0 {
      Io.show("tick: {n}")
      n -= 1
  }

  total = 0
  i = 1
  while true {
      total += i
      i += 1
      if total > 10 { break }
  }
  ```

  Each iteration gets a fresh scope (loop-local bindings don't escape). `while` is now a
  reserved keyword.

- **Subscript access (`list[i]`, `str[i]`).**
  Lists and strings now support `expr[index]` subscript syntax. The index must
  be an `int`. Result type is `T` for `list[T]` and `str` for strings — no
  nullable wrapper. Out-of-bounds and negative indices raise a runtime error,
  so there is never ambiguity between a `none` value and a missing index:

  ```keel
  items = ["alpha", "beta", "gamma"]
  first = items[0]   # str — "alpha"
  mid   = items[1]   # str — "beta"
  # items[99]        # runtime error: index 99 out of bounds (length 3)

  word = "keel"
  ch   = word[0]     # str — "k"
  # word[10]         # runtime error: string index 10 out of bounds (length 4)
  ```

  Use `len()` to guard dynamic indices; `.first()` / `.last()` remain
  available when you want a nullable fallback for the first or last element.

### Fixed

- **Struct impl dispatch is now O(1) and unambiguous.** Previously, calling an impl method on a struct value used field-set subset matching: the interpreter scanned all registered types to find one whose declared fields were a subset of the value's keys. Two types sharing the same field names (e.g. `Point` and `Vec2`, both `{x, y}`) would match each other's impls non-deterministically. The runtime now stores a type tag directly in struct values (`Value::Struct(TypeName, fields)`) and dispatches in O(1) via direct lookup. Values produced by `Ai.extract(... as: T)` are tagged with `T` so method dispatch works on the extracted struct. Untagged map literals (no type annotation at the binding site) retain the field-set fallback for backward compatibility.

---

## [0.1.26] — 2026-05-20


### Added

- **Numeric value methods (`.abs()`, `.floor()`, `.ceil()`, `.round()`).**
  All four methods are now available on `int` and `float` values. Return type matches the
  receiver — `float` methods return `float`, `int` methods return `int`. `floor`, `ceil`, and
  `round` are identity no-ops on `int`.

  ```keel
  price = -3.75
  price.abs()           # 3.75
  price.abs().ceil()    # 4.0  — chains naturally
  count = -5
  count.abs()           # 5    — int stays int
  ```

- **`Random` namespace.**
  `Random` is now available in the prelude for non-cryptographic pseudo-random values:

  ```keel
  roll = Random.int(min: 1, max: 6)
  sample = Random.float()
  enabled = Random.bool()
  ```

  Use `Random` for simulation, sampling, games, and other non-security work. `Random.int`
  uses an inclusive `min:` / `max:` range and raises a runtime error when `min > max`.

- **`Uuid` type and namespace.**
  `Uuid` is now a distinct type with random, time-ordered, deterministic, and parsed values:

  ```keel
  id: Uuid = uuid()
  trace = Uuid.v7()
  site = Uuid.v5(ns: Uuid.DNS, name: "keel-lang.dev")
  parsed = Uuid.parse("f47ac10b-58cc-4372-a567-0e02b2c3d479")
  simple = id.format(as: "simple")
  version = id.version()
  ```

  `uuid()` is an alias for `Uuid.v4()`. UUIDs interpolate as lowercase hyphenated strings and
  support `.to_str()`, `.format(as:)`, and `.version()`.

- **`Crypto` namespace.**
  `Crypto` is now available for security-sensitive hashes, HMACs, tokens, and random bytes:

  ```keel
  digest = Crypto.sha256("hello")
  sig = Crypto.hmac_sha256("message", key: secret)
  token = Crypto.token(bytes: 32)
  bytes = Crypto.random_bytes(16)
  ```

  `Crypto` exposes fixed safe SHA-2 methods: `sha224`, `sha256`, `sha384`, `sha512`,
  `sha512_224`, and `sha512_256`, with matching `hmac_` methods. Legacy MD5 and SHA-1 are
  not exposed. `Crypto.token` returns hex, so `bytes: 16` produces a 32-character token.

---

## [0.1.25] — 2026-05-19

min/max prelude functions, variadic parameters, and File namespace completion.

### Added

- **Variadic parameters (`...param: T`) and spread call sites (`...expr`).**
  Tasks can now declare a rest-parameter by prefixing it with `...`. Inside the body it
  binds as `list[T]`; zero, one, or many positional arguments are accepted. At call sites
  any `list[T]` or `set[T]` can be expanded with `...`:

  ```keel
  task greet(...names: str) -> str {
      result = ""
      for n in names { result += n + " " }
      result
  }

  greet("Alice", "Bob")             # names = ["Alice", "Bob"]
  greet()                           # names = []

  more = ["Dave", "Eve"]
  greet("Alice", ...more)           # names = ["Alice", "Dave", "Eve"]
  greet(...more, ...more)           # merge two lists

  task labeled(prefix: str, ...items: str) -> str { ... }
  labeled("tags", "rust", "keel")  # prefix = "tags", items = ["rust", "keel"]
  ```

  Passing `list[T]` without `...` to a variadic param is a **type error** — use `...scores`
  not `scores`.

- **`min` and `max` prelude free functions.**
  `min` and `max` are now global functions that work with any number of arguments. They accept
  an optional `by:` key-selector lambda and return `none` on empty input:

  ```keel
  min(3, 1, 4)                           # 1
  max("banana", "apple", "cherry")       # cherry

  scores = [4, 9, 2, 7]
  min(scores)                            # 2 — single list is auto-spread
  max(...scores, 99)                     # 99 — explicit spread + extra value

  people = [{name: "Alice", age: 30}, {name: "Bob", age: 25}]
  youngest = min(people, by: p => p.age) # {name: "Bob", age: 25}
  oldest   = max(people, by: p => p.age) # {name: "Alice", age: 30}
  ```

  Passing a single `list[T]` is a shorthand for spreading it — `min(scores)` and
  `min(...scores)` are equivalent. Both `min()` and `max()` called with no arguments
  return `none`.

- **`File` namespace complete — `mkdir`, `remove`, `copy`, `move`, `glob`, `mktemp` added.**
  The `File` namespace now covers the full local filesystem story:

  ```keel
  File.mkdir("output/reports")           # creates directory (and parents)
  File.copy("template.txt", "out.txt")   # copies a file; creates dst parent dirs
  File.move("draft.txt", "final.txt")    # renames/moves; creates dst parent dirs
  File.remove("tmp/scratch.txt")         # deletes a file or directory (recursive for dirs)
  reports = File.glob("output/*.txt")    # returns list[str] of matching paths
  tmp = File.mktemp()                    # creates a temp file; caller removes it
  tmpdir = File.mktemp(dir: true)        # creates a temp directory
  ```

  `File.remove` auto-detects files and directories — directories are removed recursively.
  `File.glob` returns an empty list when no files match; an invalid pattern is a runtime error.
  `File.mktemp` returns the path as `str`; lifecycle is the caller's responsibility.

### Fixed

- **Type checker: spread args rejected on fixed-arity tasks.** Calling a non-variadic task with
  a spread argument (`f(...xs)`) now produces a type error instead of silently bypassing the
  arity check. Spread call syntax is only valid when the callee is variadic:

  ```keel
  task greet(name: str) -> str { name }
  xs = ["Alice", "Bob"]
  greet(...xs)   # ✗ error: spread args require a variadic callee
  ```

- **Type checker: `max(...scores, 99)` and `min(...a, ...b)` were falsely rejected.**
  The checker compared spread container types (`list[int]`) against scalar types (`int`),
  causing "arguments must all have the same type" errors on valid spread-plus-extra and
  multi-spread calls. Spread element types are now unwrapped before uniformity checking:

  ```keel
  scores = [4, 9, 2]
  hi = max(...scores, 99)          # ok — 99
  lo = min(...scores, ...scores)   # ok — 2
  ```

- **`File.read` type corrected to `str` (was `str?`).** The type checker was inferring
  a nullable `str?` for `File.read`, but the runtime always raises `FileError` when the
  file is missing — it never returns `none`. Checker and docs now reflect the true contract:
  `File.read` returns `str` and raises on failure. Guard with `File.exists` if the path
  may be absent:

  ```keel
  if File.exists("config.json") {
      cfg = File.read("config.json")
  }
  ```

- **LSP: stale `Str` namespace completions removed.** The completion list and hover handler
  still offered `Str` as a namespace and listed `match`, `extract`, `truncate`, `pad` as
  "Str methods" after the `Str` namespace was removed. Those entries are now gone.

- **Type checker: `datetime` operator types.** Comparisons (`<`, `>`, `<=`, `>=`) and arithmetic
  (`+`, `-`) on `datetime` and `duration` values were incorrectly rejected by `keel check`. Now
  `datetime > datetime`, `datetime + duration`, `datetime - duration`, and `datetime - datetime`
  all type-check correctly.

### Changed

- **String API unified on value methods — `Str` namespace removed.** All string operations are
  now called directly on the string value. `Str.match`, `Str.extract`, `Str.truncate`, and
  `Str.pad` no longer exist; use the equivalent method calls instead:

  ```keel
  # before
  if Str.match(text, "\\d+") { ... }
  short = Str.truncate(text, 20)
  col   = Str.pad("ID", 10)

  # after
  if text.matches("\\d+") { ... }
  short = text.truncate(20)
  col   = "ID".pad(10)
  ```

  Two new methods added at the same time:

  ```keel
  dates = text.find_all("\\d{4}-\\d{2}-\\d{2}")   # list[str] — all matches
  clean = text.sub("\\s+", " ")                    # str — regex replace all
  ```

  **Migration:** rename `Str.match(t, p)` → `t.matches(p)`, `Str.extract(t, p)` →
  `t.extract(p)`, `Str.truncate(t, n)` → `t.truncate(n)`, `Str.pad(t, w)` → `t.pad(w)`,
  `Str.pad(t, w, char: c)` → `t.pad(w, char: c)`.

---

## [0.1.24] — 2026-05-16

Concurrency safety fixes: lock ordering, shared HTTP client, blocking I/O handling, and async event routing.

### Fixed

- Type checker: invalid operator combinations (`"x" + 5`, `"x" < 5`, `true + 1`, etc.) are now
  caught at `keel check` time instead of failing at runtime. Augmented assignment (`+=`, `-=`,
  etc.) is checked with the same rules. `unknown`/`dynamic` operands are always accepted (gradual
  typing escape hatch).

- Runtime: fixed potential deadlock in `agents_in_team` by snaphotting agent Arc handles before
  acquiring per-instance locks. Prevents lock-ordering inversion if future code paths acquire
  locks in opposite order.

- Runtime: `Http.*` now reuse a process-wide HTTP client with connection pooling instead of
  constructing a new client on every call. Eliminates redundant TCP+TLS handshakes and improves
  throughput for high-frequency HTTP operations.

- Runtime: `File.*` and `Memory.*` operations now run on the tokio blocking thread pool instead
  of blocking the main executor. Fixes stalls when many tasks perform concurrent file or memory
  I/O; blocking operations no longer starve the event loop.

- Runtime: fixed async spawning by sharing event channels and closure registries with spawned
  interpreters. Previously, `Async.spawn` created orphaned event channels that silently dropped
  `Schedule.*`, `Agent.send/broadcast`, and `Http.serve` events. Now all spawned tasks route
  events to the parent interpreter's event loop.

- Runtime: `llm.truncate()` no longer allocates on the happy path — returns `Cow<str>` so short
  strings borrow the input, reducing GC pressure.

- Runtime: `show_table` column deduplication now uses a HashSet for O(1) membership testing
  instead of O(n) linear search, fixing quadratic behaviour on wide tables.

---

## [0.1.23] — 2026-05-16

Augmented assignment, raise statement, break/continue, and list.zip()

### Added

- Language: augmented assignment operators `+=`, `-=`, `*=`, `/=` mutate an existing variable in
  its enclosing scope; they do not shadow. Works on local variables and `self.field`.
  ```keel
  total = 0
  for i in 1..5 {
      total += i
  }
  // total is 15

  self.counter += 1
  ```

- Language: `raise expr` throws a runtime error from any expression value. Strings become the
  error message directly; other values are converted with their display representation. Caught by
  `try/catch err: Error` like any other error.
  ```keel
  task validate(n: Int) {
      if n < 0 {
          raise "n must be non-negative"
      }
      return n
  }
  ```

- Language: `break` and `continue` for `for` loops. `break` exits the nearest enclosing loop
  immediately; `continue` skips the rest of the current iteration and advances to the next.
  Both are reserved keywords and affect only the innermost loop (no labeled jumps in v0.1).
  ```keel
  for item in items {
      if item == target {
          break
      }
      process(item)
  }

  for n in 1..100 {
      if n % 2 == 0 {
          continue
      }
      process_odd(n)
  }
  ```

- Language: `list.zip(other)` pairs two lists element-by-element into a list of 2-element tuples,
  stopping at the shorter list. The return type is inferred as `list[(T, U)]`, so tuple
  destructuring in `for` loops is fully typed.
  ```keel
  names  = ["alice", "bob", "carol"]
  scores = [90, 85, 95]

  for (name, score) in names.zip(scores) {
      Log.info("{name} scored {score}")
  }
  ```

---

## [0.1.22] — 2026-05-14

`when` now works as an expression — pattern-match and produce a value in one step.

### Changed

- Language: `@tools` conditional guards now use `if` instead of `when`.
  `Db.exec if self.admin` replaces the old `Db.exec when self.admin`.
  ```keel
  @tools [
    Email.send  if self.confirmed,
    Db.exec     if self.admin,
  ]
  ```

### Added

- Language: `when` is now also an **expression** — use it anywhere a value is expected.
  The matched arm's value becomes the result of the expression. All arms must produce
  the same type; a type mismatch is a compile error.
  ```keel
  label = when score {
    "A" => "excellent"
    "B" => "good"
    _   => "needs work"
  }
  ```
  The statement form (`when x { ... }` standing alone) is unchanged. The expression form
  is valid as an assignment RHS, a return value, a function argument, or any other
  expression position. Exhaustiveness rules are identical to the statement form.

---

## [0.1.21] — 2026-05-14

Nullable safety enforced at task call sites.

### Added

- Type checker: nullable safety is now enforced at task call sites. Passing a
  nullable value (`T?`) where a non-nullable parameter (`T`) is expected is a
  type error; the error message guides the caller to use `!` (null-assert) or
  `??` (null-coalesce) to unwrap.
  ```keel
  task process(x: str) { ... }

  task t() {
    val: str? = Env.get("KEY")
    process(val)          # error: expected str, got str? — use `!` or `??`
    process(val!)         # ok
    process(val ?? "")    # ok
  }
  ```
  Applies to all three call forms: top-level tasks, `self.task_name(...)`, and
  `self.method(...)`. Named args (`process(x: val)`) are also checked.
  Type mismatches beyond nullable (e.g. `int` passed where `str` expected) are
  caught at call sites as well.

---

## [0.1.20] — 2026-05-14

Full generic type support — types, enums, tasks, and function types.


### Added

- Parser + AST: generic type declarations (`type Foo[T]`, `type Pair[A, B]`) are now parsed
  and stored as `TypeDecl.type_params`. The `type_params` field is empty for non-generic types,
  so all existing declarations are unaffected.
  ```keel
  type Paginated[T] {
    items: list[T]
    page: int
    has_more: bool
  }

  type Bag[T] = list[T]

  type Pair[A, B] =
    | both { first: A, second: B }
    | only_first { value: A }
    | only_second { value: B }
  ```
- Type checker: `TypeExpr::Generic(name, args)` now resolves to a concrete `Ty` instead of
  `Ty::Unknown`. Generic struct instantiations resolve to `Ty::Struct` with type parameters
  substituted; generic aliases resolve to the expanded alias type. Generic enums register their
  variant names so exhaustiveness checking still works.
  ```keel
  type Paginated[T] { items: list[T]\npage: int\nhas_more: bool }

  task t(p: Paginated[str]) {
    items: list[str] = p.items   # type-checked: list[str] not Unknown
  }
  ```
- Formatter: `keel fmt` now round-trips generic type declarations, emitting `type Name[T, U]`
  correctly.
- Example: `examples/generic_types.keel` demonstrates generic structs, multi-parameter generics,
  and generic aliases.
- Type checker: generic enum variant field types are now deeply checked. Bindings
  destructured from a generic enum variant resolve to the substituted field type instead
  of `Unknown`. `Ty::Enum` now carries the resolved type arguments for generic instantiations.
  ```keel
  type Pair[A, B] =
    | both { first: A, second: B }

  task t(p: Pair[str, int]) {
    when p {
      both { first, second } => {
        f: str = first   # checked: str
        s: int = second  # checked: int
      }
    }
  }
  ```
- Parser: function type literals `(T1, T2) -> Ret` are now parsed as `TypeExpr::Func`.
  Zero-parameter `() -> Ret` and single-parameter `(T) -> Ret` both work. The tuple parser
  is unified with the function-type parser — `(T1, T2)` (no `->`) still produces a tuple.
  ```keel
  type Handler = (str) -> bool
  type Reducer = (str, int) -> str
  type Thunk   = () -> none
  type Predicate[T] = (T) -> bool   # generic + function type
  ```
- Parser + AST + type checker: generic task declarations. Tasks may now carry type parameters
  with the `task name[T, U](...)` syntax. Type arguments are **inferred at call sites** —
  no explicit instantiation syntax needed.
  ```keel
  task identity[T](x: T) -> T { x }
  task first[A, B](a: A, b: B) -> A { a }

  task main() {
    s: str = identity("hello")   # T inferred as str
    n: int = identity(42)        # T inferred as int
    f: str = first("hi", 99)     # A = str, B = int
  }
  ```
- Formatter: `keel fmt` now round-trips generic task declarations, emitting `task name[T, U](...)`.

---

## [0.1.19] — 2026-05-14


### Added

- Type checker: `?.` (null-safe field access) now propagates `T?` — field lookups on nullable
  structs return `Nullable(field_type)` instead of `Unknown`.
- Type checker: `??` (null coalesce) now unwraps the left-hand nullable and returns its inner
  type, so `x ?? fallback` is typed as the unwrapped `T` rather than the fallback's type.
  ```keel
  task greet(name: str?) {
    label: str = name ?? "guest"   # str? unwrapped to str
  }
  ```
- Type checker: `Ai.extract(text, as: T)` and `Ai.decide(text, as: T)` now resolve the `as:`
  argument and return `T?` instead of `unknown?`, enabling downstream field access checks.
  ```keel
  type Contact = { name: str, email: str }
  task go(text: str) {
    c = Ai.extract(text, as: Contact)
    n = c?.name   # typed str?
  }
  ```
- Type checker: lambda block bodies (`x => { ... }`) now infer their return type from the last
  expression in the block, matching the behaviour of expression-body lambdas.
- Type checker: `set[...]` literals now infer as `set[T]` instead of `list[T]`.
- Type checker: implicit return — when a task's last statement is an expression,
  its type is now checked against the declared return type.
  ```keel
  task double(n: int) -> int {
    n * 2   # checked: must be int
  }
  ```
- Type checker: `if`-expression branches are now unified — both must produce the
  same concrete type. When one branch exits via `return`, the other branch's type
  is propagated as the expression type.
  ```keel
  result = if flag { 1 } else { "oops" }   # error: branches must have the same type
  ```

---

## [0.1.18] — 2026-05-13

Explicit self for agent task calls with fixes for named args, early return, and Async.spawn scope

### Added

- Added repo-local Rust audit tooling and agent guidance, including `scripts/rust_audit.sh`,
  coverage helpers, Rust formatting defaults, and the `ms-rust` skill files.
- Added broad runtime and pipeline test coverage across namespace dispatch, LLM behavior,
  pipeline errors, AST walking, runtime configuration isolation, and prelude namespaces.

### Changed

- Split the AST and CLI into focused modules while preserving the public AST visitor alias and
  existing command behavior.
- Made agent-owned task invocation explicit: `self.task(...)` is the local agent form, bare
  `task(...)` resolves through lexical/global scope only, and `MyAgent.task(...)` is no longer a
  cross-agent dispatch surface.
- Split the runtime prelude into namespace modules and introduced shared namespace helpers so
  `src/runtime/mod.rs` no longer owns every prelude implementation.
- Moved runtime-owned services into `RuntimeContext`, including clocks, file systems, memory
  stores, caches, LLM clients, trace/log settings, and async task handles.
- Migrated short-lived internal runtime locks to `parking_lot` and exposed test-only runtime
  helpers through the `test-util` feature.
- Replaced integration-test binary rebuilds in the example parse smoke test with direct pipeline
  checks to avoid stale `CARGO_BIN_EXE_keel` races.

### Fixed

- Fixed integer modulo by zero panicking at runtime; it now returns a `RuntimeError` consistent
  with integer division by zero.
- Fixed `return` inside an expression-position `if`/`when` body being silently swallowed instead
  of propagating out of the enclosing task or closure. The interpreter now correctly propagates
  early-return signals through expression evaluators until they reach a call boundary.
- Fixed `Async.spawn` spawning closures in a bare interpreter that had no access to user-defined
  tasks, enum types, struct types, or registered closures. Spawned tasks now receive a snapshot
  of the parent interpreter's symbol tables and share `live_agents`.
- Fixed `apply_lint_fixes` potentially panicking when two fixable-warning spans overlapped;
  overlapping ranges are now merged before applying replacements.
- Named arguments (e.g. `foo(b: 20)`) are now respected when calling user-defined tasks.
  Previously all args were bound positionally regardless of label; now named args bind by
  parameter name and the remainder fill positional slots in order.
- Using unimplemented `@limits` fields (`max_cost_per_request`, `require_confirmation`) now
  produces a clear error at startup instead of being silently ignored.
- HTTP handler closure errors are now logged to stderr before falling back to a 500 response,
  making failures visible instead of silently discarded.
- Removed `now` from the LSP keyword completion list; it is a prelude identifier (`Time.now()`),
  not a reserved keyword.
- Added `Agent.send(target, message)` to the SPEC prelude table (§3.2) where it was missing
  despite being fully implemented.
- Isolated runtime-affecting state per Keel runtime context. `--trace`, `--log-level`,
  `Log.set_level`, LLM tracing, and `Async.spawn` handle IDs no longer use process-global
  mutable state that can leak between scripts or embedded program instances.
- Tightened lint/type-checker override handling by using explicit `expect` messages instead of
  silent fallbacks.
- Corrected stale docs and CI gates around deferred `.keelc` bytecode behavior, keyword counts,
  and release/test checks.
- Kept static CLI commands such as `keel check` from constructing the runtime/LLM client, so
  `KEEL_TRACE=1 keel check file.keel` no longer prints runtime provider banners.
- Made `Async.join_all` and `Async.select` await real spawned task handles, return closure results,
  preserve agent context for `self`, and propagate spawned task errors instead of returning raw handles.
- Fixed `Schedule.cron` weekday matching so `1-5` means Monday through Friday with standard
  numeric cron fields.
- Prevented shared persistent-memory reads from renaming corrupt JSON files while other readers may
  still be using the file.

---

## [0.1.17] — 2026-05-08

Agent capability gating with conditional guards & readonly state fields

### Conditional `@tools` guards

`@tools` entries now support a `when` guard — a boolean expression evaluated at the start of each
handler turn. Tools whose guard is false are blocked for that turn. Guards can access `self.*` state
and call tasks that return `bool`.

Entries can gate a whole namespace or a specific method:

```keel
agent SupportBot {
  state { confirmed: bool = false, admin: bool = false }

  @tools [
    Email.fetch,                           # always allowed
    Email.send when self.confirmed,        # only after confirmation
    Db.query,                              # always allowed
    Db.exec   when self.admin,             # admin only
    Http,                                  # whole namespace, always
  ]
}
```

Calling a blocked method raises a `CapabilityError` at runtime.

---

### Readonly state fields

Agent state fields can now be declared `readonly` to prevent the agent from overwriting runtime-provided context:

```keel
agent SessionBot {
  state {
    turns:      int          = 0
    session_id: readonly str = "default-session"
  }

  on message(msg: str) {
    self.turns = self.turns + 1        # ok
    # self.session_id = "x"           # compile error: field is declared readonly
    Io.show(self.session_id)           # reading is always allowed
  }
}
```

The `readonly` modifier sits between the colon and the type. The type checker rejects any `self.field = ...` assignment to a readonly field. The runtime enforces the same restriction as a second safety net.

---

## [0.1.16] — 2026-05-07

Thirteen new list operations, seven new string methods, and `keel check --strict` for unknown-type binding detection.

### Extended list operations

`list[T]` gains thirteen new built-in methods:

| Method | Returns | Notes |
|---|---|---|
| `list.any(predicate)` | `bool` | `true` if any element matches |
| `list.all(predicate)` | `bool` | `true` if every element matches |
| `list.find(predicate)` | `T?` | first match or `none` |
| `list.reduce(fn, init)` | any | fold; `fn` receives `(acc, elem)` |
| `list.sum()` | `int\|float` | numeric lists only; runtime error on non-numeric |
| `list.min()` | `T?` | `none` on empty list |
| `list.max()` | `T?` | `none` on empty list |
| `list.join(sep)` | `str` | concatenates with separator |
| `list.sort()` | `list[T]` | natural order (int, float, str) |
| `list.reverse()` | `list[T]` | reverses in place (returns new list) |
| `list.flatten()` | `list[T]` | unwraps one level of nesting |
| `list.take(n)` | `list[T]` | first `n` elements |
| `list.skip(n)` | `list[T]` | all but the first `n` elements |

```keel
scores = [42, 17, 95, 8, 73]

scores.sum()                          # 235
scores.min()                          # 8
scores.max()                          # 95
scores.any(s => s > 80)              # true
scores.all(s => s > 5)               # true
scores.find(s => s > 60)             # 95
scores.reduce((acc, x) => acc + x, 0) # 235
scores.sort()                         # [8, 17, 42, 73, 95]
scores.sort().reverse().take(3)       # [95, 73, 42]
scores.skip(2).join(", ")            # "95, 8, 73"

[[1, 2], [3, 4]].flatten()           # [1, 2, 3, 4]
["a", "b", "c"].join(" | ")          # "a | b | c"
```

### New string methods

Seven new methods on `str`, plus the previously documented-but-unimplemented `to_int` and `to_float` are now wired:

| Method | Returns | Notes |
|---|---|---|
| `.trim_start()` | `str` | strips leading whitespace |
| `.trim_end()` | `str` | strips trailing whitespace |
| `.repeat(n)` | `str` | repeats string `n` times |
| `.slice(start, end?)` | `str` | char-indexed substring; exclusive end |
| `.index_of(needle)` | `int?` | char position of first match, or `none` |
| `.to_int()` | `int?` | parse as integer; `none` on failure |
| `.to_float()` | `float?` | parse as float; `none` on failure |

```keel
"  hello  ".trim_start()          # "hello  "
"  hello  ".trim_end()            # "  hello"
"ha".repeat(3)                    # "hahaha"
"hello world".slice(6, 11)        # "world"
"hello world".index_of("world")   # 6
"hello world".index_of("xyz")     # none
"42".to_int()                     # 42
"3.14".to_float()                 # 3.14
"bad".to_int()                    # none
```

### `keel check --strict`

A new `--strict` flag for `keel check` surfaces bindings whose type the checker cannot resolve. In normal mode, these are silently accepted as `Unknown`; `--strict` turns them into errors.

```bash
keel check file.keel           # normal — Unknown bindings are silent
keel check --strict file.keel  # strict — Unknown bindings are errors
```

Example: `data = Json.parse(raw)` produces `cannot infer type of 'data'; consider adding a type annotation` in strict mode. Fix it with an explicit cast: `data = Json.parse(raw) as MyType`.

Strict mode is opt-in — existing programs continue to pass under `keel check`.

---

## [0.1.15] — 2026-05-06

### Typed AI errors and `try/catch` wiring (breaking)

**`fallback:` parameter removed from all `Ai.*` calls.**

Previously `Ai.classify` and `Ai.summarize` accepted a `fallback:` named argument that silently swallowed both call failures and schema mismatches. This violated Keel's "no silent fallbacks" principle.

**Migration:** replace `fallback:` with an explicit `??`:

```keel
# before
urgency = Ai.classify(email.body, as: Urgency, fallback: Urgency.medium)

# after
urgency = Ai.classify(email.body, as: Urgency) ?? Urgency.medium
```

**New error model for `Ai.*` calls:**

| Situation | Behaviour |
|---|---|
| Call failure (network, timeout, mock) | Returns `none` — `??` provides the default |
| LLM output didn't match schema/enum | Throws `AiSchemaError` — `try/catch` to handle |
| Fatal config error | Propagates as a hard error (same as before) |

`AiSchemaError` carries `message: str` and `got: str` (the raw LLM output). Both are caught by `catch err: Error`.

```keel
try {
  urgency = Ai.classify(email.body, as: Urgency) ?? Urgency.medium
} catch err: AiSchemaError {
  Io.notify("Unexpected LLM output: {err.got}")
  urgency = Urgency.medium
} catch err: Error {
  Io.notify("AI call failed: {err.message}")
}
```

**`try/catch` now fully wired.** Catch clauses are matched by type name (`AiSchemaError`, `Error`, any named type). The first matching clause runs; unmatched errors re-propagate. The bound name (e.g. `err`) carries a map with at least `message: str`.

---

## [0.1.14] — 2026-05-06

### `for` loop inline filter with `if`

`for` loops now support an inline filter guard using `if`, replacing the `where` keyword in that position:

```keel
# Only process unread emails
for email in emails if email.unread {
  triage(email)
}

# Works with destructuring
for { from, subject } in emails if subject != "" {
  Io.show("{from}: {subject}")
}

# Works with ranges
for n in 1..10 if n % 2 == 0 {
  Io.show(n)
}
```

This is a **breaking change** for any existing `for...where` loops (none existed in the standard library or examples). The `where` keyword is preserved in `when` arm guards and remains reserved for future type-predicate syntax.

### `Time` namespace — full rework

**Breaking changes** from the earlier v0.1.14 Time stub:

- `now` is no longer a keyword. Use `Time.now()`.
- `Time.format(dt, as:)` is removed. Use `dt.format(as:)`.
- `Time.diff(a, b)` is removed. Use `a - b`.
- `Time.parse()` rejects naive strings (no timezone offset) — returns `none`. Supply `tz:` to coerce a naive string.

**New API:**

```keel
# Factories — namespace style (constructors, no receiver)
now  = Time.now()                          # UTC, millisecond-precision RFC 3339
ny   = Time.now(tz: "America/New_York")    # IANA timezone — offset-shifted RFC 3339
dt   = Time.parse("2026-05-06T09:00:00Z") # datetime? — none on failure or missing TZ
dt2  = Time.parse("2026-05-06", tz: "UTC") # coerce naive string with tz:

# Methods on datetime values
p    = dt.parts()                          # {year, month, day, hour, minute, second, millisecond, tz}
s    = dt.format(as: "%Y-%m-%d")          # str? — none if not a datetime

# Operators
elapsed  = finish - start                 # datetime - datetime → duration
deadline = Time.now() + 3.days           # datetime + duration → datetime
ago      = Time.now() - 30.minutes       # datetime - duration → datetime
ok       = deadline > Time.now()         # comparison still works

# Millisecond duration literal
short = 500.ms    # aliases: millis, millisecond, milliseconds
```

`Time.now()` emits millisecond-precision RFC 3339 (`2026-05-06T07:10:17.355Z`). All `datetime ± duration` results also preserve millisecond precision.

### Keyword count claim removed

The specific keyword count (previously claimed as 22, 27, or 28 words inconsistently across files) has been removed from `README.md`, `SPEC.md`, `ROADMAP.md`, and `CLAUDE.md`. The language is evolving and pinning a count creates incorrect documentation.

---

## [0.1.13] — 2026-05-05

### Destructuring (§8.4)

Keel now supports all five destructuring forms specified in SPEC.md §8.4. No new keywords — destructuring is pure syntax sugar over existing binding forms.

**Struct shorthand** — bind fields by their original names:

```keel
{urgency, category} = result
```

**Struct rename** — bind a field under a different local name:

```keel
{urgency: u, category: c} = result
```

**Tuple positional** — bind list elements by position:

```keel
(label, count) = ("alpha", 42)
```

**For-loop iteration** — destructure each element as the loop variable:

```keel
for {from, subject} in emails {
  Io.show("{from}: {subject}")
}
```

**Task parameters** — destructure a struct argument at the call boundary:

```keel
task handle({body, from}: Email) {
  Io.show("From {from}: {body}")
}
```

The type checker enforces struct field existence and tuple arity. Missing fields and arity mismatches are compile-time errors. Keyword-named fields (`from`, `state`, `in`, etc.) are accepted in all destructure positions.

**Example** — `examples/destructure.keel`:

```keel
type Email = { body: str, from: str, subject: str }

task summarise({ body, from }: Email) -> str {
  return "From {from}: {body}"
}

agent Inbox {
  @on_start {
    emails = [
      { body: "hello", from: "alice@example.com", subject: "hi" },
      { body: "world", from: "bob@example.com",   subject: "hey" },
    ]
    for { from, subject } in emails {
      Io.show("{from}: {subject}")
    }
    { body, from } = { body: "test", from: "carol@example.com", subject: "test" }
    Io.show("Body: {body}")
    pair = ("alpha", 42)
    (label, count) = pair
    Io.show("{label} — {count}")
    stop(self)
  }
}
run(Inbox)
```

### Tests

10 new integration tests: struct shorthand, struct rename, tuple, for-loop, task param, keyword field `from`, missing-field type error, tuple arity mismatch type error, and `examples_all_parse_includes_destructure`.

---

## [0.1.12] — 2026-05-04

### Range operator `..`

`start..end` produces an **inclusive** `list[int]` containing every integer from `start` to `end`.

```keel
agent Counter {
  @on_start {
    for i in 1..5 {
      Io.notify("{i}")   # prints 1, 2, 3, 4, 5
    }

    xs = 0..3            # xs == [0, 1, 2, 3]
    Io.notify("{xs.count()}")   # prints 4
  }
}
run(Counter)
```

- Both operands must be `int`; non-integer bounds are a type error.
- `5..3` → `[]` (empty when start > end). `4..4` → `[4]`.
- **Lazy evaluation** — `1..1_000_000_000` is O(1) memory. `for i in 1..n` never materializes the list. Analytical methods (`count`, `is_empty`, `contains`, `first`, `last`) are O(1). `map`/`filter` iterate lazily and return a new `list`.
- No spaces around `..` in formatted output.
- SPEC grammar updated: `RangeExpr <- AddExpr (".." AddExpr)?` inserted between `CompExpr` and `AddExpr`.
- Bare `!` postfix (null assert) added to SPEC §20 grammar alongside `!.IDENT`.

---

## [0.1.11] — 2026-05-03

### Memory — safe cross-process storage (breaking path change)

Persistent memory storage is now path-safe and cross-process safe.

#### Breaking

The persistent memory directory layout changed:

| Before (v0.1.10) | After (v0.1.11) |
|---|---|
| `~/.keel/memory/<file-stem>/<agent>.json` | `~/.keel/memory/<stem>_<hash12>/<agent>.json` |

The `<hash12>` is the first 12 hex characters of the SHA-256 hash of the canonicalized source file path. Two programs that happen to share a filename (e.g. `counter.keel` in different directories) now get their own storage buckets. Existing data is **not** auto-migrated; move manually if needed.

#### What changed

**Identity** — directory name is now `<stem>_<hash12>` derived from the canonical file path. REPL / stdin / inline sessions use `__repl__`, `__stdin__`, `__inline__` (no hash, stable within their kind).

**Cross-process safety** — each `Memory.*` operation acquires an advisory `flock` on a sidecar `<agent>.lock` file (exclusive for writes, shared for reads). The sidecar is never renamed, so the lock target is stable while the data file is being atomically replaced.

**Crash durability** — `Memory.remember` now calls `fsync` on the temp file before rename, and `fsync` on the parent directory after rename. A crash mid-write leaves the previous `.json` intact.

**Path validation** — agent names are now validated at the storage boundary (hard error, not `debug_assert`). Identifiers containing `.`, `/`, `\`, or `\0` are rejected.

```keel
agent Bot {
  @memory persistent   # stored at ~/.keel/memory/bot_<hash12>/Bot.json

  @on_start {
    last = Memory.recall("last_user")
    if last != none {
      Io.show("Welcome back, {last}!")
    }
    Memory.remember("last_user", "Alice")
    stop(self)
  }
}
```

---

## [0.1.10] — 2026-05-03

### Memory namespace

`Memory.remember`, `Memory.recall`, and `Memory.forget` are now real operations. They were no-op stubs since v0.1.0.

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
```

#### `@memory` — scope selection

Three modes, selected with the `@memory` attribute:

- **`session`** (default when the attribute is omitted) — in-process store. Values persist across handler calls within a single program run and are cleared on restart.
- **`persistent`** — file-backed JSON store at `~/.keel/memory/<agent-name>.json`. Values survive restarts.
- **`none`** — disables `Memory.*` entirely for the agent. Any call raises `CapabilityError`.

```keel
agent Bot {
  @memory persistent   # remembers across runs

  @on_start {
    last = Memory.recall("last_user")
    if last != none {
      Io.show("Welcome back, {last}!")
    }
    Memory.remember("last_user", "Alice")
    stop(self)
  }
}
```

The persistent store is a simple JSON key-value file — one file per agent name. There is no vector search in v0.1; semantic recall is planned for v0.2 via the `VectorStore` interface.

---

## [0.1.9] — 2026-05-03

### `keel init` — three fixes

**In-place initialization.** Running `keel init` with no argument now scaffolds directly into the current directory instead of creating a subdirectory named after the parent folder.

```bash
mkdir myproject && cd myproject
keel init          # writes main.keel here, not myproject/myproject/
keel run main.keel
```

**Absolute/relative path argument.** `keel init /some/path/myproject` now uses the basename (`myproject`) as the project and agent name. Previously the full path was used, producing an invalid agent name.

```bash
keel init /tmp/mybot   # agent MyBot { ... }  ✓  (was: agent /tmp/mybot { ... })
```

**Runnable template.** The generated `main.keel` now prints and exits cleanly instead of scheduling a timer. The old template used `Schedule.every` which kept the process alive forever without any visible output.

```keel
agent MyBot {
  @role "Describe what this agent does"

  @on_start {
    Io.show("Hello from MyBot!")
    stop(self)
  }
}

run(MyBot)
```

### `stop(self)` — self-stop from inside an agent

Bare `self` now resolves to an `AgentRef` for the current agent, so an agent can stop itself without repeating its own name.

```keel
agent Worker {
  @on_start {
    Io.show("done")
    stop(self)     # was: stop(Worker)
  }
}
```

`self` as an expression is valid anywhere inside an agent body where an `AgentRef` is expected. It errors at runtime if used outside an agent context.

### Tooling — Linter & Sharper Errors

#### `keel lint` command

New command that checks a Keel program for style and best-practice issues beyond type correctness.

```bash
keel lint agent.keel
keel lint --fix agent.keel
```

Four rules:

**Unused variable** — warns when a local binding is assigned but never read.

```keel
agent Noisy {
  @on_start {
    temp = "debug"        # ⚠ unused — prefix with _temp to suppress
    Io.show("hello")
  }
}
```

**Uncalled task** — warns when a `task` is declared but never invoked anywhere in the program.

```keel
task helper(x: int) -> int { x + 1 }   # ⚠ never called

agent Main {
  @on_start { Io.show("hi") }
}
```

**`Ai.*` outside agent** — warns when `Ai.classify`, `Ai.summarize`, etc. are called from a plain task or top-level statement without `@role` / `@model` context.

```keel
task process(text: str) -> str {
  Ai.summarize(text)   # ⚠ Ai.* called outside agent context
}
```

**State written, never read** — warns when an agent state field is assigned but `self.field` is never referenced.

```keel
agent Counter {
  state { total: int = 0 }
  @on_start {
    self.total = self.total + 1   # ⚠ self.total is written but never read
  }
}
```

`--fix` removes unused variable assignments automatically. Prefix a variable with `_` to suppress the warning:

```keel
_debug = expensive_call()   # suppressed — no warning
```

#### `keel check` — source spans in all diagnostics

Every error from `keel check` now includes a line:column pointer and an underlined source excerpt rendered via `miette`. Previously, many errors printed only a message with no location. Arity errors now list the expected parameter names as a correction hint:

```
  × Type error
   ╭─[agent.keel:8:5]
 7 │   @on_start {
 8 │     greet()
   ·     ───┬───
   ·        ╰── task `greet` takes 1 argument(s), got 0 — expected: name
 9 │   }
   ╰────
```

#### New example: `examples/lint_best_practices.keel`

Demonstrates variable reuse, state tracking, and task calls — the patterns the linter validates as correct.

---

## [0.1.8] — 2026-04-30

### Reactive Agents & Text Processing

This release adds HTTP webhook handling, in-memory caching, regex and string tools, and LSP go-to-definition and rename.

#### Cache namespace

Process-scoped in-memory cache with optional TTL:

```keel
Cache.set("key", "value", ttl: 60.seconds)
val = Cache.get("key")          // returns value or none
Cache.delete("key")
Cache.clear()
```

#### Str namespace

Regex matching and string manipulation:

```keel
if Str.match(text, "\\d+") {
  number = Str.extract(text, "(\\d+)")
}
text = Str.truncate("hello world", 5)      // "hello…"
text = Str.pad("42", 5)                    // "   42"
text = Str.pad("42", 5, char: "0")         // "00042"
```

#### Http.serve

HTTP listener for webhooks and reactive agents:

```keel
Http.serve(8080, (request) => {
  method = request["method"]
  path = request["path"]
  body = request["body"]
  { status: 200, body: "OK" }
})
```

The handler receives a `request` map with `method`, `path`, `body` and returns a response map with `status`, `body`.

#### LSP go-to-definition & rename

Jump to task/agent/type declarations and rename symbols across a file in VS Code and other LSP clients.

---

## [0.1.7] — 2026-04-30

### Structured Concurrency & Agent Constraints

This release adds file I/O, JSON processing, async task spawning, cron scheduling, and agent capability restrictions.

#### File namespace

Read, write, and list files on disk:

```keel
File.write("data.txt", "Hello Keel")
content = File.read("data.txt")

if File.exists("data.txt") {
  Io.show(content)
}

entries = File.list("data/")
```

`File.read` raises `FileError` if the file doesn't exist; `File.write` creates intermediate directories automatically.

#### Json namespace

Parse JSON strings and serialize Keel values:

```keel
data = Json.parse("{\"name\": \"Alice\", \"age\": 30}")
name = data["name"]

user = { name: "Bob", age: 25 }
json_str = Json.stringify(user)
```

#### Schedule.cron

Schedule recurring tasks using 5-field cron expressions:

```keel
Schedule.cron("0 9 * * 1-5", () => {
  Io.show("Morning digest")
})

Schedule.cron("*/5 * * * *", () => {
  Io.show("Every 5 minutes")
})
```

Supports standard cron syntax: minute, hour, day, month, weekday.

#### Async.spawn / join_all / select

Spawn independent Tokio tasks and await their completion:

```keel
task1 = Async.spawn(() => {
  result1 = Http.get("https://api1.example.com")
  Io.show(result1)
})

task2 = Async.spawn(() => {
  result2 = Http.get("https://api2.example.com")
  Io.show(result2)
})

handles = [task1, task2]
results = Async.join_all(handles)
Io.show("All tasks done")
```

`Async.select` returns the first handle to complete (race condition).

#### @tools capability gating

Restrict namespace access within an agent:

```keel
agent RestrictedAgent {
  @tools [Io, File]

  @on_start {
    Io.show("Allowed")
    File.write("x.txt", "Allowed")
    # Http.get would raise CapabilityError
  }
}
```

If no `@tools` attribute is specified, all namespaces are accessible.

#### @limits enforcement infrastructure

Extract and enforce per-agent resource limits:

```keel
agent LimitedAgent {
  @limits { timeout: 30s, max_tokens: 1000, max_cost: 5.0 }

  @on_start {
    response = Ai.prompt("...")
  }
}
```

The `timeout` limit wraps calls via `Control.with_timeout`. The `max_tokens` and `max_cost` are passed to Ollama where supported.

#### LSP completion

`textDocument/completion` now suggests prelude namespaces, method names, and keywords. Triggered on "." or manual invocation.

#### New example programs

- `file_processing.keel` — File read/write/exists/list
- `json_processing.keel` — JSON parse/stringify
- `cron_schedule.keel` — Cron expression scheduling
- `parallel_execution.keel` — Async task spawning and joining
- `capability_gating.keel` — @tools attribute enforcement

---

## [0.1.6] — 2026-04-28

### Wiring & ergonomics

This release closes long-standing gaps in the standard library and the
language server. Every primitive that was a stub in 0.1.5 now does what
its name promises.

#### Nested string literals inside `{interp}`

The lexer used to terminate a string at the first `"` it saw, even one
hiding inside a `{...}` slot. The lexer now scans the slot body with brace
depth tracking and recursively handles nested `"..."`, so this works:

```keel
agent A {
  @on_start {
    name = "world"
    Io.show("hi {"there {name}"}")  # prints: hi there world
  }
}
```

#### `Control.retry` / `with_timeout` / `with_deadline`

The three control-flow combinators now have real implementations.

```keel
# Re-invoke until success or the budget is spent.
result = Control.retry(5, () => Ai.prompt(system: "...", user: "..."))

# Abort if the closure runs past the duration.
fast = Control.with_timeout(2.seconds, () => slow_call())

# Abort once the absolute deadline has passed.
done = Control.with_deadline("2026-12-31T23:59:00Z", () => long_task())
```

`with_timeout` and `with_deadline` raise `TimeoutError` / `DeadlineError`
on expiry; `retry` surfaces the last attempt's error if every attempt
fails.

#### `Agent.broadcast(team, data)`

Tag agents with `@team [...]` and dispatch a single event to every
member of a named team:

```keel
agent Alpha { @team ["frontline"]  on alert(m: str) { ... } }
agent Beta  { @team ["frontline"]  on alert(m: str) { ... } }
agent Gamma { @team ["backoffice"] on alert(m: str) { ... } }

agent Coordinator {
  @on_start {
    Agent.run(Alpha); Agent.run(Beta); Agent.run(Gamma)
    Agent.broadcast("frontline", "incident", event: "alert")
    # Alpha and Beta fire; Gamma does not.
  }
}
```

#### `Email.archive(message)`

`Email.archive` now performs an IMAP folder move (`UID MOVE`, falling back
to `COPY` + `\Deleted` + `EXPUNGE` for servers without the MOVE
extension). The destination folder is `Archive` by default and can be
overridden with the `IMAP_ARCHIVE_FOLDER` env var. `Email.fetch` now
returns each message's UID under the `uid` key so `archive` can target
the right message.

#### `map[K, V]` method inference

Map literals support the common operations on both the type checker and
runtime side:

| Expression | Inferred type |
|---|---|
| `map.get(k)` | `V?` |
| `map.keys()` | `list[K]` |
| `map.values()` | `list[V]` |
| `map.len()` / `map.count()` / `map.size()` | `int` |
| `map.is_empty()` | `bool` |
| `map.contains(k)` / `map.has(k)` | `bool` |

The checker also accepts a `{k: v, ...}` struct literal in any position
that expects `map[str, V]`, so the same surface syntax serves both
struct and map construction.

#### LSP hover

`textDocument/hover` returns the inferred type of the identifier under
the cursor, looking through `let`-bindings, function parameters, agent
state fields, and prelude namespaces (`Io`, `Ai`, `Control`, …).

#### Examples

`examples/retry_on_failure.keel`, `examples/broadcast_team.keel`,
`examples/nested_interp.keel`, and `examples/map_methods.keel` exercise
the new primitives end-to-end and run under `KEEL_LLM=mock`.

---

## [0.1.5] — 2026-04-27

### Type checker hardening

#### Nullable safety at call sites

`T?` is now a distinct, enforced type. Passing a nullable value where a
non-nullable is expected is a type error.

```keel
task t() {
  x: str = Env.get("KEY")   # error: expected str, got str?
  y: str = Env.get("KEY")!  # ok — ! unwraps, throws NullError if none
  z: str = Env.get("KEY") ?? "default"  # ok — ?? coalesces
}
```

`NullAssert` (`!`) now returns the unwrapped inner type in the checker, so
`x: str = some_nullable!` is accepted without a false positive.

#### Return-type matching

`return expr` inside a task with a declared `-> T` is now verified against
that type.

```keel
task greet() -> str {
  return 42   # error: return value: expected str, got int
}
```

Bare `return` (no value) is still accepted inside any task.

#### Struct field checks

Named struct types now resolve to their field list in the checker. A struct
literal passed where a named type is expected is checked for missing fields.
Extra fields are allowed (structural subtyping).

```keel
type Person { name: str, age: int }

task t() {
  p: Person = { name: "Alice" }         # error: missing field `age`
  q: Person = { name: "Bob", age: 30 }  # ok
  r: Person = { name: "Eve", age: 25, extra: true }  # ok — extra fields allowed
}
```

#### Generic list and string method type inference

List and string method calls now return typed results instead of `unknown`:

| Expression | Inferred type |
|---|---|
| `list.push(x)` / `list.filter(fn)` | `list[T]` (same element type) |
| `list.len()` / `list.count()` | `int` |
| `list.contains(x)` / `list.is_empty()` | `bool` |
| `list.first()` / `list.last()` | `T?` |
| `list.map(fn)` | `list[unknown]` (lambda return deferred) |
| `listA + listB` | `list[T]` |
| `str.len()` / `str.count()` | `int` |
| `str.upper()` / `str.lower()` / `str.trim()` / `str.replace()` | `str` |
| `str.split(sep)` | `list[str]` |
| `str.contains(s)` / `str.starts_with(s)` / `str.ends_with(s)` | `bool` |

```keel
task t() {
  items = ["a", "b", "c"]
  n: int   = items.len()           # ok
  more     = items.push("d")       # list[str]
  short    = items.filter(x => true)  # list[str]
  for s in short { Io.notify(s) }  # s: str
}
```

---

## [0.1.4] — 2026-04-27

### Parser hardening — every expression-level feature now works

#### `if`-as-expression

`if` can now appear on any right-hand side, not just as a standalone statement.

```keel
label = if score > 0.8 { "high" } else if score > 0.5 { "medium" } else { "low" }
```

#### `let` type annotations validated

Declaring a type on a `let` binding now produces a type error when the
inferred type of the value doesn't match. The check is skipped for
user-defined named types that the checker can't fully resolve yet.

```keel
x: int = "hello"   # type error: `x` expected int, got str
y: str = "hello"   # ok
```

#### `!` postfix unwrap — retroactive

`expr!` (null-assert; throws `NullError` if the value is `none`) was already
fully implemented across the parser, interpreter, and type checker. The ROADMAP
marker was stale — no code change, just updated docs.

#### `list + list` and `list.push(item)`

Lists support concatenation with `+` and a functional `push` that returns a
new list.

```keel
base  = ["a", "b"]
extra = ["c", "d"]
all   = base + extra               # ["a", "b", "c", "d"]
more  = all.push("e")              # ["a", "b", "c", "d", "e"]
```

`push` does not mutate in place — reassign the result: `items = items.push(x)`.

#### Full string interpolation

String interpolation slots (`{...}`) now run through the real expression
parser. Function calls, binary expressions, and method chains all work.

```keel
summary = "Items: {cart.count()}, total: {price * qty}"
```

#### `@on_stop` lifecycle hook

Agents with `@on_stop { ... }` now have their block executed before the agent
is removed from the runtime.

```keel
agent Logger {
    @on_stop { Log.info("Logger shutting down") }
}
# Agent.stop(Logger) → prints "Logger shutting down"
```

#### `Agent.delegate(target, task, args)`

Posts a named task event to another agent's mailbox. The receiving agent
handles it via its `on <task>` handler.

```keel
Agent.delegate(Processor, "handle", payload)
# Processor's `on handle` fires with payload
```

#### `Search`, `Db`, `Time` stub namespaces

Calling any method on these namespaces now raises a clear
`"... is planned for v0.2 and is not available in v0.1."` error instead of the
generic `"Unknown namespace"` panic.

```keel
Search.web("keel language")
# Error: Search is planned for v0.2 and is not available in v0.1.
```

---

## [0.1.3] — 2026-04-26

### Declarations reach the model

#### `@rules` injected into every LLM system prompt

`@rules` was previously parsed but silently dropped. Every `Ai.*` call made
inside an agent with `@rules` now prepends the rules as a bullet list in the
system prompt, between the role preamble and the operation-specific instructions.

```keel
agent Support {
    @role "Customer support specialist"
    @rules [
        "Never reveal internal pricing logic",
        "Escalate if the user expresses frustration 3+ times"
    ]

    @on_start {
        reply = Ai.draft("Welcome message", tone: "friendly")
    }
}
```

System prompt shape sent to the LLM:

```
You are Customer support specialist.

Rules:
- Never reveal internal pricing logic
- Escalate if the user expresses frustration 3+ times

You are a text drafter. Draft the following with a friendly tone.
```

Enable `KEEL_TRACE=1` to see the full system prompt for every call.

#### `Ai.summarize` — `format:` and `max:` wired

`format:` and `max:` were previously parsed but ignored. They now emit
directives in the system prompt:

```keel
summary = Ai.summarize(article, format: bullets, max: 5, unit: sentences)
```

- `format: bullets` → "Format your response as a bulleted list."
- `format: prose` → "Format your response as flowing prose."
- `max: N` with `unit: sentences` → "Use at most N sentences."
- `max: N` without a unit → "Use at most N items."

#### `Ai.prompt` — `response_format: json` wired

`response_format: json` was previously ignored. It now:
1. Appends "Respond with valid JSON only. No prose, no markdown fences." to the system prompt.
2. Validates the LLM reply — if the reply cannot be parsed as JSON, a runtime error is raised.

```keel
score = Ai.prompt(
    system: "Rate sentiment on a 1-10 scale.",
    user: "Text: {review}",
    response_format: json
)
```

#### `Ai.extract(x, as: T)` — schema derived from declared struct type

`as: T` was previously parsed but fell through to an empty schema. A
`struct_types` registry is now populated during program evaluation, and
`Ai.extract(x, as: T)` derives the field schema from it.

```keel
type Invoice { vendor: str, amount: float, date: str }

agent Extractor {
    @on_start {
        result = Ai.extract("Invoice from ACME $99.99 on 2026-01-10", as: Invoice)
    }
}
```

The outgoing system prompt now contains `vendor`, `amount`, and `date` as
extraction targets. Using `as: UnknownType` raises a runtime error with an
actionable message.

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
- **Attributes.** `@role`, `@model` are core attributes. `@on_start`, `@on_stop`, `@tools`, `@memory`, `@rules`, `@limits`, `@team`, `@provider` are stdlib attributes, parsed by the grammar. Wiring status: `@role`, `@model`, `@on_start`, and `@rules` are executed at runtime. `@role` is prepended as `"You are {role}.\n\n..."` to every `Ai.*` system prompt; `@rules` injects a bullet list of rules between the role preamble and the operation-specific instructions (wired in v0.1.3). The remaining stdlib attributes are parsed but have no runtime effect yet (tracked in [ROADMAP](ROADMAP.md)).
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
  - Partial: `Ai.classify(..., considering: {...})` — argument parsed but not forwarded to the LLM yet; `Ai.decide` returns a plain `{choice, reason, confidence: 1.0}` map instead of a typed `Decision[T]`. (As of v0.1.3: `Ai.summarize(format:, max:)`, `Ai.prompt(response_format: json)`, and `Ai.extract(as: T)` are all fully wired.)
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
