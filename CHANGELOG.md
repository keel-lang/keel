# Changelog

All notable changes to Keel.

> **Alpha.** Keel is v0.1. Breaking changes are expected between 0.x releases. Do not build production systems on 0.x.

> **Doc-update rule.** Any feature or spec change — added, updated, or removed — must update the docs in the same release. At minimum: `docs/src/release-notes.md` plus every guide page in `docs/src/guide/` (and `docs/src/examples/`, `docs/src/cli/`, `docs/src/config/` where applicable) that the change touches. `SPEC.md` and `ROADMAP.md` are part of this rule. A release is not shipped until `mdbook build` runs clean over the updated pages.

---

## [Unreleased]

%%TAGLINE%% update this line before releasing — one sentence summary of the release

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
