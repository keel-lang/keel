# Keel Roadmap

> Keel is in **alpha** (v0.1). Expect breaking changes. Do not build production systems yet.

---

## Principles

1. **Small core, deep stdlib.** Everything that can be a library is one. The core earns its keep through the type system, the compiler, or the actor runtime.
2. **Rust from day one.** Single-binary distribution, async via Tokio, no runtime dependencies on other language ecosystems.
3. **Prelude-as-stdlib.** Users never write `use keel/ai`. Namespaces like `Ai`, `Io`, `Schedule`, `Http` are auto-imported. Implementations are designed to become swappable via interfaces; v0.1 ships Ollama only.
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
| Type checker | [x] | Scope, arity, enum exhaustiveness, nullable safety (including call-site enforcement), return-type matching, struct subtyping, `?.`/`??` propagation, `Ai.extract`/`Ai.decide` `as:` inference, lambda block bodies, `set[]` literals, implicit return, `if`-expr branch unification (v0.1.19), generic type instantiation — `Foo[T]` declarations parsed and substituted at use sites; generic struct/alias bodies resolve concretely (v0.1.20); generic task declarations with call-site type-parameter inference (v0.1.20); nullable arg checking at all task call sites (v0.1.21); `when` expression arm-type unification (v0.1.22). |
| Augmented assignment (`+=`, `-=`, `*=`, `/=`) | [x] | Mutates the nearest enclosing scope binding; does not shadow; works on locals and `self.field` |
| `break` / `continue` in loops | [x] | Exit or skip iterations; both are reserved keywords; innermost-loop semantics only |
| `raise expr` | [x] | Symmetric with `try`/`catch`; string value becomes error message; any other value is converted via display |
| Type checker: operator type compatibility | [x] | Binary expressions (`+`, `-`, `%=`, etc.) don't validate that operand types are compatible — `"x" + 5` passes the checker and fails at runtime. Needs a `check_binop(lhs_ty, op, rhs_ty)` pass applied uniformly to all `BinOp` and `AugAssign` sites. |
| Variadic parameters (`...param: T`) | [x] | Declaration, `list[T]` inside body, `...expr` spread at call sites; named args follow naturally via positional/named dispatch | Unblocks `min`/`max` prelude functions |
| Bytecode compiler (`keel build`) | [ ] | Deferred post-v0.1 — tree-walking interpreter covers all alpha workloads |

### Agent model

| Feature | Status | Notes |
|---|---|---|
| `agent` declaration + `run` / `stop` | [x] | |
| `@on_start` / `@on_stop` blocks | [x] | |
| Per-agent serial mailbox + `on <event>` | [x] | |
| `self.` state read/write | [x] | |
| `self.task(...)` agent-local task calls | [x] | Bare `task(...)` stays lexical/global; cross-agent work uses mailbox APIs |
| `readonly` state field modifier | [x] | Compiler + runtime enforcement; assignment to readonly field is an error |
| `Agent.send` / `Agent.delegate` | [x] | |
| `Agent.broadcast(team, data)` | [x] | Fans out to every live agent in the named `@team` |

### Attributes

| Attribute | Tier | Status | Notes |
|---|---|---|---|
| `@model "ollama:..."` | core | [x] | Read by `Ai.*` to pick the Ollama model |
| `@role "..."` | core | [x] | Prepended as `"You are {role}.\n\n..."` to every `Ai.*` system prompt |
| `@on_start { ... }` | stdlib | [x] | Block runs once when the agent starts |
| `@on_stop { ... }` | stdlib | [x] | Block runs once when the agent stops (v0.1.4) |
| `@tools [...]` | stdlib | [x] | Capability gating — unlisted namespaces raise `CapabilityError` (v0.1.7); conditional `when` guards (v0.1.17) |
| `@memory persistent\|session\|none` | stdlib | [x] | Selects memory scope; enforced at runtime (v0.1.10) |
| `@rules [...]` | stdlib | [x] | Injected as a bullet list into the system prompt of every `Ai.*` call (v0.1.3) |
| `@limits { ... }` | stdlib | [~] | `timeout` enforced via `Control.with_timeout` (v0.1.7); `max_tokens`/`max_cost` extracted but not enforced at the Ollama level |
| `@team [...]` | stdlib | [x] | Team membership used by `Agent.broadcast` routing (v0.1.6) |
| `@provider MyProvider` | stdlib | [ ] | Parsed, no per-agent LLM-provider swap |

### Stdlib namespaces

| Namespace | Status | Implemented | Gaps |
|---|---|---|---|
| `Ai` | [~] | `classify`, `summarize` (format/max), `draft`, `extract` (as: T), `translate`, `decide`, `prompt` (response_format: json) | `embed` deferred to v0.2 (requires pluggable provider registry); `Ai.install(provider)` not registered |
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
| `String value methods` | [x] | `.matches`, `.extract`, `.truncate`, `.pad`, `.find_all`, `.sub` — all string ops on the value; `Str` namespace removed | — |
| `File` | [x] | `read`, `write`, `exists`, `list`, `mkdir`, `remove`, `copy`, `move`, `glob`, `mktemp` | — |
| Numeric value methods | [ ] | `.abs()`, `.floor()`, `.ceil()`, `.round()` on `int`/`float`; `floor`/`ceil`/`round` are no-ops on `int` | No `Math` namespace — ops live on the value |
| `min` / `max` prelude functions | [ ] | `min(...items: T, by:?) -> T?`, `max(...items: T, by:?) -> T?`; spread with `...` | Requires variadic params |
| `Random` namespace | [ ] | `Random.float()`, `Random.int(min:, max:)`, `Random.bool()` | — |
| `Uuid` type + namespace | [ ] | `Uuid.v4()`, `Uuid.v7()`, `Uuid.v5(ns:, name:)`, `Uuid.parse(s) -> Uuid?`; `uuid()` prelude alias; `.version()`, `.format(as:)`, `.to_str()` | Implements `Stringable` |
| `Stringable` interface | [ ] | `interface Stringable { task to_str() -> str }`; enables `"{expr}"` interpolation for any type | Implemented by all primitives + `Uuid` |
| `Json` | [x] | `parse`, `stringify` | — |
| `Time` | [x] | `now(tz:)`, `parse(tz:)`, `dt.parts()`, `dt.format(as:)`; `dt ± dur`, `dt - dt → duration`; `500.ms` … `1.week` | — |
| `Search` | [~] | — | Registered; all methods raise a clear "planned for v0.2" error |
| `Db` | [~] | — | Registered; all methods raise a clear "planned for v0.2" error |
| `Crypto` | [ ] | — | `hash(data, algo:)`, `hmac(data, key:, algo:)`, `token(bytes:)`, `random_bytes(n)` — cryptographic primitives; distinct from `Random` (PRNG) |

### CLI

| Command | Status | Notes |
|---|---|---|
| `keel run` | [x] | Execute a .keel program |
| `keel check` | [x] | Type-check only, no execution; `--strict` rejects unknown-typed bindings |
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
- **Pluggable LLM provider registry + `Ai.embed`.** Define a `LlmProvider` trait covering both chat and embedding methods. Ship built-in impls for Ollama, OpenAI, Gemini, DeepSeek, and Anthropic (via Voyage AI). Expose the trait publicly so developers can register custom providers. `Ai.embed` ships alongside this — it needs multi-provider dispatch and a common interface before it's worth implementing.
- **Vector-store `Memory` backend.** Current persistent store is a JSON file. Semantic search needs an embeddings pipeline and `VectorStore` interface — belongs in v0.2.
- **Major dependency bumps** (`chumsky 0.9 → 1.0`, `imap 2 → 3`, `colored 2 → 3`, `lettre 0.11 → 0.12`) — batched for v0.2.
- **`while` loop.** Unbounded iteration. Needs new `Stmt::While` AST node, lexer token, parser, type-checker, and interpreter eval. The highest-impact control-flow gap — deferred only because `while` + `break`/`continue` + a mutable-loop-variable convention is a small but coherent design unit worth speccing carefully.
- **`list[i]` subscript access.** No `Expr::Index` exists yet. Requires new AST variant, parser rule, type-checker (bounds are dynamic), and interpreter. `items.first()` / `items.last()` / `.nth(i)` bridge the gap for now.
- **Structural pattern matching.** `when` currently matches only enum variant tags. Struct destructuring, map shapes, and tuple decomposition would transform agent decision code from imperative `if` chains into exhaustiveness-checked decision tables — significant scope, best specced post-v0.1.
- **Variadic functions** — shipped in v0.1.25 (see core language table).
- **Lazy sequences / generators.** Everything is eagerly materialized. Extending `Range`'s lazy evaluation to a general iterator protocol would make large-dataset pipelines memory-efficient.
- **String format specifiers** (`"{ value:.2f }"`). Agents spend most of their time formatting text; a minimal subset (`.2f`, `>10`, `<10`) would cover 90% of needs.
- **CSV / YAML serialization.** LLMs frequently emit and consume CSV; YAML is the dominant config format. A `Csv` and `Yaml` namespace alongside `Json` rounds out the serialization story.
- **Subprocess / shell-out.** A `Shell.run(cmd)` primitive is the universal tool bridge. Deferred to avoid adding an OS-shell dependency to the v0.1 binary surface; revisit when the sandboxed execution model is clearer.

---

## Beyond v0.1

v0.2 and later milestones are **deliberately un-planned** until v0.1 ships.

- **v1.0** is the first API-stable release. Semver begins at v1.0. Scope defined after real usage feedback from v0.1.

One ship at a time.

---

## How to Get Involved

- **Read the spec.** If something reads wrong, open an issue.
- **Try an example.** Find the gap between spec and implementation; report it.
- **Write an interface implementation.** Custom LLM provider, memory store, scheduler backend — those are exactly the right things to prototype right now.
- **Do not build production systems on v0.1.**
