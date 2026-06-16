# Keel Roadmap

> Keel is in **alpha** (v0.1). Expect breaking changes. Do not build production systems yet.

---

## Principles

1. **Small core, deep stdlib.** Everything that can be a library is one. The core earns its keep through the type system, the compiler, or the actor runtime.
2. **Rust from day one.** Single-binary distribution, async via Tokio, no runtime dependencies on other language ecosystems.
3. **Stdlib-as-modules.** The standard library is imported explicitly — `use std/ai`, `use std/file` — with the same syntax as local file imports; nothing is ambient except agent verbs, generic utilities, and built-in types. Implementations are designed to become swappable via interfaces; v0.1 ships Ollama only.
4. **No silent fallbacks.** Configuration mistakes surface as errors at startup, not as silent mock responses at runtime.

---

## v0.1 — Alpha

**Goal:** a runnable language where agents can be declared, type-checked, and executed end-to-end with a real LLM provider.

Legend: **[x]** complete · **[~]** partial · **[ ]** planned.

### Core language

| Feature | Status | Notes |
|---|---|---|
| Lexer | [x] | |
| Parser | [x] | Attributes, interfaces, named args, `as T`, rich enums, triple-quoted strings, duration literals, destructuring |
| Interpreter | [x] | Namespace dispatch, agent lifecycle, pattern matching, closures, async, `try/catch` |
| Formatter (`keel fmt`) | [x] | Idempotent round-trip against AST |
| Type checker | [x] | Scope, arity, enum exhaustiveness, nullable safety (including call-site enforcement), return-type matching, struct subtyping, `?.`/`??` propagation, `ai.extract`/`ai.decide` `as:` inference, lambda block bodies, `set[]` literals, implicit return, `if`-expr branch unification (v0.1.19), generic type instantiation — `Foo[T]` declarations parsed and substituted at use sites; generic struct/alias bodies resolve concretely (v0.1.20); generic task declarations with call-site type-parameter inference (v0.1.20); nullable arg checking at all task call sites (v0.1.21); `when` expression arm-type unification (v0.1.22). Type-mismatch error caret now points at the type annotation span, not the whole statement. **AST `Node<T>` migration complete (Stage 5):** all `(T, Span)` tuple call-sites replaced by `Node<T>` structs with named `.kind`/`.span` fields. |
| Augmented assignment (`+=`, `-=`, `*=`, `/=`) | [x] | Mutates the nearest enclosing scope binding; does not shadow; works on locals and `self.field` |
| `break` / `continue` in loops | [x] | Exit or skip iterations; both are reserved keywords; innermost-loop semantics only |
| `raise expr` | [x] | Symmetric with `try`/`catch`; string value becomes error message; any other value is converted via display |
| Type checker: operator type compatibility | [x] | Binary expressions (`+`, `-`, `%=`, etc.) don't validate that operand types are compatible — `"x" + 5` passes the checker and fails at runtime. Needs a `check_binop(lhs_ty, op, rhs_ty)` pass applied uniformly to all `BinOp` and `AugAssign` sites. |
| Variadic parameters (`...param: T`) | [x] | Declaration, `list[T]` inside body, `...expr` spread at call sites; named args follow naturally via positional/named dispatch | Unblocks `min`/`max` prelude functions |
| Bytecode compiler (`keel build`) | [ ] | Deferred post-v0.1 — tree-walking interpreter covers all alpha workloads |
| String interpolation format specifiers (`:.2f`, `>10`, `<10`) | [x] | `{expr:spec}` — float precision (`:.Nf`), width alignment (`:<N`, `:>N`, `:^N`), bare width (`:N`); specs may combine (`{x:>10.2f}`); colon detected at outermost bracket depth so named args are not confused with the spec separator |
| Enum `Stringable` — auto-derive `to_str` from variant name | [x] | Unit-only enums interpolate as their variant name (`Signal.buy` → `"buy"`) with no extra `impl` required. Bare variant names (`buy`) are intentionally disallowed to prevent collisions; use the namespaced form `Signal.buy`. |
| `as T` coercions + `typeof()` | [x] | `as T` now performs real coercions: `int↔float`, `int/float/bool→str`, `str→int/float/bool` (raises on invalid), `none→any` (raises), `dynamic` pass-through. Container targets — `list[T]`, `map[K, V]`, and tuple `(T1, T2, …)` — assert the runtime shape and recurse element-wise (raising on shape/element mismatch), so `json.parse(...) as list[dynamic]` narrows cleanly. `typeof(x) -> str` added as a prelude free function returning the runtime type name (declared name for structs/enums). |
| `list.sort_by(key_fn)` — sort with custom key | [x] | `.sort(by: x => x.score)` — optional `by:` named arg on existing `.sort()`, consistent with `min(by:)`/`max(by:)`; key fn returns int/float/str; ascending; two-phase (compute all keys async, sort synchronously) |
| Struct spread-update expression (`{ ...base, field: new }`) | [x] | `{ ...record, price: fill_price }` copies all fields from `record` then overrides `price`. One spread, must be first; zero or more overrides follow. Type tag preserved. Unknown override fields are a compile-time error. |
| Struct pattern matching in `when` | [x] | `{ field1, field2 }` binds named struct fields in `when` arms; `where` guards route on field values; unguarded struct arm is total (no `_` required). |
| Module system — `use std/<name>` + `use "./file.keel"` | [x] | Full module graph loading: std modules (lowercase, imported explicitly), local file imports namespaced by file stem, `as` aliasing, multi-symbol `use A, B as C from ...`, implicit main (top-level statements run only in the entry file), per-file test discovery, circular-import errors with cycle paths, tombstone diagnostics for the removed PascalCase prelude. One flat global namespace per program in this release (conflicts are compile errors); module-private scoping and stdlib-written-in-Keel (`stdlib/` sources embedded in the binary, merged with intrinsics per module) are follow-ups. Closes #66. |

### Agent model

| Feature | Status | Notes |
|---|---|---|
| `agent` declaration + `run` / `stop` | [x] | |
| `@on_start` / `@on_stop` blocks | [x] | |
| Per-agent serial mailbox + `on <event>` | [x] | Bounded channel (1024 default; `KEEL_EVENT_QUEUE_CAPACITY`); overflow policy per producer — scheduler drops, HTTP returns 503, `Agent.send` raises `RuntimeBusy` |
| `self.` state read/write | [x] | |
| `self.task(...)` agent-local task calls | [x] | Bare `task(...)` stays lexical/global; cross-agent work uses mailbox APIs |
| `readonly` state field modifier | [x] | Compiler + runtime enforcement; assignment to readonly field is an error |
| `Agent.send` / `Agent.delegate` | [x] | |
| `broadcast(team, data)` | [x] | Fans out to every live agent in the named `@team` |
| Type-safe `Agent.delegate` — symbol form | [x] | `delegate(Foo.handle, arg)` — `Foo.handle` is a compile-time–resolved handler reference; the checker validates the handler exists and the data arg matches the handler's parameter type. String form `delegate(Foo, "handle", arg)` is also validated for plain string literals. Both forms coexist for backward compatibility. |

### Attributes

| Attribute | Tier | Status | Notes |
|---|---|---|---|
| `@model "ollama:..."` | core | [x] | Read by `ai.*` to pick the Ollama model |
| `@role "..."` | core | [x] | Prepended as `"You are {role}.\n\n..."` to every `ai.*` system prompt |
| `@on_start { ... }` | stdlib | [x] | Block runs once when the agent starts |
| `@on_stop { ... }` | stdlib | [x] | Block runs once when the agent stops (v0.1.4) |
| `@tools [...]` | stdlib | [x] | Deny-by-default capability gating for effectful modules (ai, io, http, email, file, shell, db, search, env); pure-compute modules are never gated; `@tools all` is the explicit unrestricted form; uncovered direct calls are compile errors, transitive calls raise `CapabilityError` at runtime; `if` guards re-evaluated per turn |
| `@memory persistent\|session\|none` | stdlib | [x] | Selects memory scope; enforced at runtime (v0.1.10) |
| `@rules [...]` | stdlib | [x] | Injected as a bullet list into the system prompt of every `ai.*` call (v0.1.3) |
| `@limits { ... }` | stdlib | [~] | `timeout` enforced via `control.with_timeout` (v0.1.7); `max_tokens`/`max_cost` extracted but not enforced at the Ollama level |
| `@team [...]` | stdlib | [x] | Team membership used by `broadcast` routing (v0.1.6) |
| `@provider MyProvider` | stdlib | [ ] | Parsed, no per-agent LLM-provider swap |

### Stdlib namespaces

| Namespace | Status | Implemented | Gaps |
|---|---|---|---|
| `std/ai` | [~] | `classify`, `summarize` (format/max), `draft`, `extract` (as: T), `translate`, `decide`, `prompt` (response_format: json) | `embed` deferred to v0.2 (requires pluggable provider registry); `ai.install(provider)` not registered |
| `std/io` | [x] | `notify`, `show`, `ask`, `confirm` | — |
| `std/schedule` | [x] | `every`, `after`, `at`, `cron`, `sleep` | — |
| `std/email` | [~] | `fetch` (IMAP), `send` (SMTP), `archive` (IMAP folder move with fallback) | — |
| `std/http` | [x] | `get`, `post`, `request`, `serve` (webhook listener) | — |
| `std/env` | [x] | `get`, `require` | — |
| `std/log` | [x] | `info`, `warn`, `error`, `debug`, `set_level`, `level` | — |
| Agent verbs (built-in) | [x] | `run`, `stop`, `send`, `delegate`, `broadcast` — language-level free functions, always in scope | The `Agent` namespace dissolved into the language with the module system |
| `std/memory` | [x] | `remember`, `recall`, `forget` — session (default) or persistent (file-backed JSON) | Vector-store backend (semantic search) is v0.2 |
| `std/control` | [x] | `retry`, `with_timeout`, `with_deadline` (v0.1.6) | — |
| `std/async` | [x] | `spawn`, `join_all`, `select`, `sleep` (v0.1.7) | — |
| `std/cache` | [~] | `set` (optional TTL), `get`, `delete`, `clear` — process-scoped | `cache.get` return type undocumented — at runtime returns the stored value at its original type, or `none` if absent or expired; the Keel-visible type is `dynamic?` (dynamic because the checker cannot know what was stored, nullable for absent keys). Add `cache.get(key) -> dynamic?` to SPEC §3 so users know the stored type is preserved and a narrow cast (`as T`) recovers it. |
| `String value methods` | [x] | `.matches`, `.extract`, `.truncate`, `.pad`, `.find_all`, `.sub` — all string ops on the value; `Str` namespace removed | — |
| `std/file` | [x] | `read`, `write`, `exists`, `list`, `mkdir`, `remove`, `copy`, `move`, `glob`, `mktemp` | — |
| Numeric value methods | [x] | `.abs()`, `.floor()`, `.ceil()`, `.round()` on `int`/`float`; `floor`/`ceil`/`round` are no-ops on `int` | No `Math` namespace — ops live on the value |
| `min` / `max` prelude functions | [x] | `min(...items: T, by:?) -> T?`, `max(...items: T, by:?) -> T?`; single list arg auto-spread; `none` on empty | — |
| `std/random` | [x] | `random.float()`, `random.int(min:, max:)`, `random.bool()` | — |
| `std/uuid` | [x] | `uuid.v4()`, `uuid.v7()`, `uuid.v5(ns:, name:)`, `uuid.parse(s) -> Uuid?`; `uuid()` prelude alias; `.version()`, `.format(as:)`, `.to_str()` | Implements `Stringable` |
| `Stringable` interface | [x] | `impl Stringable for T { task to_str(self) -> str { ... } }`; enables `"{expr}"` interpolation for user-defined types | Primitives + `Uuid` built-in; user types opt in via `impl` block; `impl` reserved keyword |
| User-defined interfaces | [x] | `interface Name { task method(self) -> T }` declares a protocol; `impl Name for Type { ... }` satisfies it; compiler validates all methods present, arity, and return types; impl methods take priority over built-in map methods | No interface-as-type (`task f(x: Iterable)`) — values are structural, not nominally typed through interfaces |
| `Comparable` interface | [x] | `task compare(self, other) -> int`; wired into `list.sort()`, `list.min()`, `list.max()`, global `min()`/`max()` | Async insertion sort for struct lists |
| `Equatable` interface | [x] | `task equals(self, other) -> bool`; method-only, `==` stays structural | — |
| `Serializable` interface | [x] | `task to_json(self) -> str`; auto-wired into `json.stringify` | — |
| `Iterable` interface | [x] | `task items(self) -> list[T]`; struct usable in `for` loop | Not a generator; materialises full list; concrete `list[T]` return type accepted |
| `Hashable` interface | [ ] | `interface Hashable { task hash(self) -> int; task equals(self, other: Self) -> bool }` — allows user-defined structs and enums to be used as `map[K, V]` keys; compiler validates K implements Hashable | Deferred to v0.2; in v0.1 only `str`, `int`, `bool` are valid map key types (float is rejected at compile time, nullable and struct/enum keys raise) |
| `std/json` | [~] | `parse`, `stringify` | `json.parse` return-type semantics undocumented — at runtime, JSON objects become `Value::Map` (field access `parsed.key` works), arrays become `Value::List` (index `parsed[i]` works), numbers become `int` or `float`, strings become `str`. None of this is stated in SPEC or ROADMAP. Add to SPEC §3 and to `docs/src/guide/` so users know `(json.parse(body) as dynamic).field` is valid Keel. |
| `std/time` | [~] | `now(tz:)`, `parse(tz:)`, `dt.parts()`, `dt.format(as:)`, `epoch_ms() -> int`; `dt ± dur`, `dt - dt → duration`; `500.ms` … `1.week` | — |
| `std/search` | [~] | — | Registered; all methods raise a clear "planned for v0.2" error |
| `std/db` | [x] | `connect(url)` → `DbConnection`, `db.query(sql, params?)` → `list[map[str,dynamic]]`, `db.exec(sql, params?)` → `int` | Multi-backend (Postgres, MySQL) deferred to v0.2 |
| `std/crypto` | [x] | `sha224(data)`, `sha256(data)`, `sha384(data)`, `sha512(data)`, `sha512_224(data)`, `sha512_256(data)`, matching `hmac_` methods, `token(bytes:)`, `random_bytes(n)` | Cryptographic primitives; distinct from `Random` (PRNG) |
| `std/math` | [x] | `sqrt`, `pow`, `exp`, `log`, `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `PI()`, `E()` | All return `float`; accept `int` or `float` input; domain errors raise |
| `std/csv` | [x] | `parse`, `parse_records`, `stringify` | RFC 4180–compliant. `parse` → `list[list[str]]`; `parse_records` → `list[map[str, str]]` keyed by first row; `stringify` accepts `list[list[str]]` (include a header list as the first element if desired). Cells with commas, quotes, or newlines are auto-quoted. |
| `std/yaml` | [ ] | — | `yaml.parse(str) -> dynamic`, `yaml.stringify(value) -> str` (module-only — ships without any ambient form). YAML is the dominant config and agent-definition format. Promoted from "Deferred post-v0.1". |
| `std/shell` | [x] | `run(cmd, stdin:?, cwd:?)` → `{ stdout: str, stderr: str, exit_code: int }` | — |

### CLI

| Command | Status | Notes |
|---|---|---|
| `keel run` | [x] | Execute a .keel program |
| `keel check` | [x] | Type-check only, no execution; `--strict` rejects unknown-typed bindings |
| `keel test` | [x] | Execute top-level `test` blocks with test-local `mock Ns.method => value` overrides and `assert` statements |
| `keel fmt` | [x] | Auto-format; idempotent round-trip against the AST |
| `keel init` | [x] | Scaffold a new project |
| `keel repl` | [x] | Interactive REPL; multi-line input, persistent environment |
| `keel lint` | [x] | Static analysis; `--fix` flag (v0.1.9) |
| `keel lsp` | [x] | Language server; diagnostics, hover, completion, go-to-definition, rename |
| `keel build` | [ ] | Bytecode compiler + VM; explicitly deferred post-v0.1 |

### Distribution

| Item | Status | Notes |
|---|---|---|
| macOS (Apple Silicon) + Linux x86_64 release tarballs | [x] | Built by CI on tag push |
| Homebrew tap (`keel-lang/tap/keel`) | [x] | |
| `install.sh` one-liner | [x] | Served at `https://keel-lang.dev/install.sh` |
| Manual-trigger release workflows | [x] | `release-patch.yml` and `release-minor.yml` in `.github/workflows/` |
| End-to-end install validation | [ ] | Tarballs, Homebrew, one-liner all verified post-release |

### Deferred post-v0.1

- **`keel build` bytecode compiler.** Tree-walking interpreter is fast enough for alpha (~8ms cold start). Revisit with a concrete motivator (LLVM/WASM backend, embeddable runtime).
- **Pluggable LLM provider registry + `ai.embed`.** Define a `LlmProvider` trait covering both chat and embedding methods. Ship built-in impls for Ollama, OpenAI, Gemini, DeepSeek, and Anthropic (via Voyage AI). Expose the trait publicly so developers can register custom providers. `ai.embed` ships alongside this — it needs multi-provider dispatch and a common interface before it's worth implementing.
- **Vector-store `Memory` backend.** Current persistent store is a JSON file. Semantic search needs an embeddings pipeline and `VectorStore` interface — belongs in v0.2.
- **Major dependency bumps** (`chumsky 0.9 → 1.0`, `imap 2 → 3`, `colored 2 → 3`, `lettre 0.11 → 0.12`) — batched for v0.2.
- ~~**`while` loop.**~~ Shipped in v0.1.27. `Stmt::While`, lexer token, parser, type-checker, formatter, lint pass, and interpreter eval. `break`/`continue` work identically to `for` loops.
- ~~**Structural pattern matching (struct/map/tuple shape).**~~ Promoted to v0.1.x planned — see Core language table. Enum variant matching, rich destructuring, wildcards, and literal matching remain [x].
- **Variadic functions** — shipped in v0.1.25 (see core language table).
- **Lazy sequences / generators.** Everything is eagerly materialized. Extending `Range`'s lazy evaluation to a general iterator protocol would make large-dataset pipelines memory-efficient.
- ~~**String format specifiers** (`"{ value:.2f }"`)~~. Promoted to v0.1.x planned — see Core language table.
- ~~**CSV / YAML serialization.**~~ Promoted to v0.1.x planned — see `std/csv` / `std/yaml` in Stdlib namespaces table.
- ~~**Subprocess / shell-out.**~~ Shipped in v0.1.x — see `Shell` in Stdlib namespaces table.

---

## Beyond v0.1

v0.2 and later milestones are **deliberately un-planned** until v0.1 ships.

Known technical debt to address post-v0.1:

- ~~**Split `keel-lang` into a layered workspace.**~~ Shipped (#71). The single ~41.6k-LOC crate — the heaviest compile unit in the graph — is now five crates: `keel-syntax` → `keel-catalog` → `keel-compiler` → `keel-runtime` → `keel-lang` (facade + binary). Moving the stdlib catalog into the neutral `keel-catalog` leaf breaks the old `types → runtime` cycle, and the acyclic layering is now compiler-enforced. The embedding API is unchanged — `keel-lang` re-exports `ast`/`modules`/`catalog`/`diagnostics`/`session` under their original paths. Incremental/test-loop build times drop sharply (parser-edit test build ~6.9s → ~0.8s); clean full-build and `lto="fat"` release builds are unchanged by design.

- ~~**Type-tagged struct values.**~~ Shipped alongside v0.1.28. `Value::Struct(TypeName, fields)` is now a distinct variant; impl dispatch is O(1) via direct type-name lookup. Field-set fallback retained for untagged map literals. Ambiguous dispatch between types sharing field names is eliminated.

- ~~**Nominal struct type identity.**~~ Shipped. `Ty::Struct` now carries `name: Option<String>`. Named struct types are nominally distinct in the checker — `type A { x: int }` and `type B { x: int }` are no longer interchangeable. Anonymous struct literals remain structurally compatible with any named type that has the required fields. `impl` dispatch no longer falls back to field-set subset matching for untagged maps; list elements are promoted to their declared struct type at typed assignment boundaries. `describe_ty` now returns the declared name (e.g. `Score`) for named structs in error messages.

- ~~**Name resolution extracted from `infer_expr`.**~~ Shipped. The dedicated HIR lowering pass now owns global identifier classification and resolved reference IDs. `infer_expr` no longer performs ad-hoc string comparisons against multiple lookup tables for `Ident`, `FieldAccess`, `MethodCall`, and `Call` arms. Adding a new prelude namespace requires only a catalog entry. Prerequisite for multi-file imports and cross-module visibility (tracked as a future item).

- ~~**Read-only HIR semantic index.**~~ Shipped. Parser AST now lowers into `src/hir/` before checker and LSP analysis. HIR owns `SymbolId`s, resolved identifier references, and struct-vs-map brace-literal classification. Interpreter migration remains intentionally deferred until a second execution backend needs the shared representation.

- ~~**Typed runtime errors for all stdlib namespaces.**~~ Shipped. All catchable namespace errors now carry a stable `RuntimeErrorKind` (`FileError`, `CsvError`, `DbError`, `MathError`, `MemoryError`, `EmailError`, `HttpError`, `ShellError`, `JsonError`, `EnvError`, `AiError`, `AiSchemaError`, `CapabilityError`, `TimeoutError`, `DeadlineError`, `UserRaised`, `RuntimeBusy`). `try/catch` can match any specific type or use `Error` as the fallback. `raise` now produces `UserRaised`. The mutable interpreter side-channel (`last_typed_error`) was removed in the companion issue (#19); this issue finishes the migration by classifying all remaining `miette!` string errors.

- **v1.0** is the first API-stable release. Semver begins at v1.0. Scope defined after real usage feedback from v0.1.

One ship at a time.

---

## How to Get Involved

- **Read the spec.** If something reads wrong, open an issue.
- **Try an example.** Find the gap between spec and implementation; report it.
- **Write an interface implementation.** Custom LLM provider, memory store, scheduler backend — those are exactly the right things to prototype right now.
- **Do not build production systems on v0.1.**
