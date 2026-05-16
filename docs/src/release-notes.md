# Release Notes

> **Alpha.** Keel is v0.1. Breaking changes are expected between 0.x releases.

---

## Unreleased

---

## v0.1.24 — 2026-05-16

### Bug fixes

This release focuses on runtime safety and performance:

- Fixed potential deadlock in agent team queries through proper lock ordering.
- HTTP client now uses connection pooling, eliminating redundant TCP+TLS handshakes.
- File and memory I/O operations now use async-safe blocking thread pools, preventing event loop stalls.
- Fixed async task spawning to properly route events to the parent interpreter.
- Reduced allocations in string truncation and improved deduplication performance.

---

## v0.1.23 — 2026-05-16

### Augmented assignment — `+=`, `-=`, `*=`, `/=`

Mutate an existing variable in its nearest enclosing scope without shadowing it:

```keel
total = 0
for i in 1..5 {
    total += i
}
# total is 15
```

Also works on `self.field` in agent handlers: `self.counter += 1`.

### `raise expr` — throw errors symmetrically with `try/catch`

`raise` makes the error model complete — you can now throw as well as catch:

```keel
task validate(n: int) {
    if n < 0 {
        raise "n must be non-negative"
    }
    return n
}
```

Strings become the error message; other values are converted via their display representation. Caught by `try/catch err: Error` like any other runtime error.

### `break` and `continue` in `for` loops

`break` exits the nearest enclosing `for` loop immediately. `continue` skips the rest of the current iteration and advances to the next. Both are reserved keywords.

```keel
# stop as soon as the target is found
for item in items {
    if item == target {
        break
    }
    process(item)
}

# skip even numbers
for n in 1..100 {
    if n % 2 == 0 {
        continue
    }
    process_odd(n)
}
```

Both affect only the **innermost** loop. No labeled jumps in v0.1.

### `list.zip(other)` — pair two lists

`zip` pairs elements from two lists into a list of 2-element tuples, stopping at the shorter list.
The return type is inferred as `list[(T, U)]`, so tuple destructuring in `for` loops is fully typed.

```keel
names  = ["alice", "bob", "carol"]
scores = [90, 85, 95]

for (name, score) in names.zip(scores) {
    Log.info("{name} scored {score}")
}
```

---

## v0.1.22 — 2026-05-14

### `@tools` guards now use `if`

Conditional tool entries in `@tools` now use `if` instead of `when`:

```keel
@tools [
  Email.send  if self.confirmed,   # only after confirmation
  Db.exec     if self.admin,       # admin only
]
```

This separates tool guards from the `when` pattern-match keyword. Existing code using `when self.*` in `@tools` must be updated to `if self.*`.

### `when` as an expression

`when` now works in expression position — the matched arm's value becomes the result.

```keel
label = when score {
  "A" => "excellent"
  "B" => "good"
  _   => "needs work"
}
```

All arms must produce the same type; a mismatch is a compile error. Exhaustiveness
rules are identical to the statement form. See the [control flow guide](guide/control-flow.md).

---

## v0.1.21 — 2026-05-14

### Nullable safety enforced at task call sites

The type checker now rejects nullable arguments passed to non-nullable parameters
at every task call site — top-level tasks, `self.task(...)`, and `self.method(...)`.

```keel
task process(text: str) { ... }

task t() {
  val: str? = Env.get("PROMPT")
  process(val)          # error: expected str, got str? — use `!` or `??`
  process(val!)         # ok
  process(val ?? "")    # ok
}
```

Named arguments are also checked: `process(text: val)` produces the same error.
Type mismatches at call sites (e.g. `int` passed where `str` is expected) are
caught as well.

---

## v0.1.20 — 2026-05-14

### Generic type declarations

Type declarations can now be parameterised over one or more type variables.

```keel
type Paginated[T] {
  items: list[T]
  page: int
  has_more: bool
}

type Pair[A, B] {
  first: A
  second: B
}

type Bag[T] = list[T]
```

The type checker resolves generic instantiations to concrete types.
`Paginated[str].items` is now checked as `list[str]` rather than `unknown`.
Generic enums register variant names for exhaustiveness checking.

### Function type literals

Function types can now be written inline and used as type aliases or parameter types.

```keel
type Handler      = (str) -> bool
type Reducer      = (str, int) -> str
type Thunk        = () -> none
type Predicate[T] = (T) -> bool

task t(h: Handler) {
  ok: bool = h("hello")
}
```

Zero-parameter and multi-parameter forms both work. Tuple syntax `(T1, T2)` without `->` continues to produce a tuple type.

### Generic enum variant field types

Bindings destructured from a generic enum variant now resolve to the substituted
field type instead of `unknown`.

```keel
type Pair[A, B] =
  | both { first: A, second: B }
  | only_first { value: A }
  | only_second { value: B }

task t(p: Pair[str, int]) {
  when p {
    both { first, second } => {
      f: str = first    # type-checked as str
      s: int = second   # type-checked as int
    }
    only_first { value } => { Io.show(value) }
    only_second { value } => { Io.show("{value}") }
  }
}
```

### Generic task declarations

Tasks may now declare type parameters. Type arguments are inferred at every
call site — no explicit instantiation syntax is needed.

```keel
task identity[T](x: T) -> T { x }
task first[A, B](a: A, b: B) -> A { a }

task main() {
  s: str = identity("hello")   # T = str
  n: int = identity(42)        # T = int
  f: str = first("hi", 99)     # A = str, B = int
}
```

The formatter round-trips `task name[T, U](...)` syntax correctly.

---

## v0.1.19 — 2026-05-14

### Type checker improvements (additive)

Seven type checker gaps closed — no existing programs break.

- **`?.` propagates nullable** — `x?.field` on a nullable struct now types as `FieldType?` instead of `unknown`.
- **`??` unwraps nullable** — `x ?? fallback` now returns the inner type of `x`, not the fallback's type.
- **`Ai.extract` / `Ai.decide` with `as:`** — when an `as: T` argument is present, the return type is inferred as `T?` rather than `unknown?`. Downstream field accesses on the result are now checked.
- **Lambda block bodies** — `x => { ... }` now infers its return type from the last expression, matching expression-body lambdas.
- **`set[]` literal type** — `set[1, 2, 3]` now infers as `set[int]` instead of `list[int]`.
- **Implicit return checking** — when a task's last statement is an expression, its type is checked against the declared return type. Control-flow statements (`return`, `when`, `for`) are excluded to avoid false positives.
- **`if`-expression branch unification** — both branches of an `if` expression must produce the same concrete type. When one branch exits via `return`, the other branch's type is used.

---

## v0.1.18 — 2026-05-13

### Explicit agent task calls

Agent-owned tasks are now invoked as `self.task(...)`. Inside an agent body,
bare `task(...)` resolves only through lexical and top-level scope, while
cross-agent work stays on mailbox APIs such as `Agent.send(...)` and
`Agent.delegate(...)`.

---

## v0.1.17 — 2026-05-08

### Conditional `@tools` guards

`@tools` entries now support a `when` guard — a boolean expression evaluated at the start of each handler turn. Tools whose guard is false are blocked for that turn. Guards can access `self.*` state and call tasks that return `bool`.

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

See the [Agents guide](guide/agents.md#tools--capability-list) for full details.

### Readonly state fields

State fields declared with `readonly` between the colon and the type are **compiler-enforced read-only**. Any `self.field = ...` assignment is a compile-time error.

```keel
state {
  turns:      int          = 0
  session_id: readonly str = "default-session"
}
```

See the [Agents guide](guide/agents.md#readonly-fields) for full details.

---

## v0.1.16 — List & String Enhancements

### Extended list operations

`list[T]` gains thirteen new methods: `any`, `all`, `find`, `reduce`, `sum`, `min`, `max`, `join`, `sort`, `reverse`, `flatten`, `take`, `skip`. See the [Collections guide](guide/collections.md) for full reference.

### New string methods

Seven new string methods added: `trim_start`, `trim_end`, `repeat`, `slice`, `index_of`, `to_int`, `to_float`. See the [String Interpolation guide](guide/strings.md) for the full method table.

### `keel check --strict`

`keel check --strict <file>` now rejects bindings whose type the checker cannot infer. Normal `keel check` still accepts them silently. Use `--strict` to verify that type annotations are complete. See [`keel check`](../cli/check.md) for details.

---

## v0.1.15 — Error Handling Rework

### Breaking: `fallback:` removed from all `Ai.*` calls

`fallback:` is no longer a valid argument on any `Ai.*` function. Replace every call site with `??` at the expression level:

| Before | After |
|---|---|
| `Ai.classify(text, as: T, fallback: T.x)` | `Ai.classify(text, as: T) ?? T.x` |
| `Ai.summarize(body, in: 3, unit: sentences, fallback: "none")` | `Ai.summarize(body, in: 3, unit: sentences) ?? "none"` |

The type-checker now always infers `T?` for `Ai.classify` regardless of arguments.

### Two-tier failure model

`Ai.*` calls have two distinct failure modes:

| Failure | Result | Handle with |
|---|---|---|
| Network failure / mock mode / timeout | Returns `none` | `??` |
| LLM returned output that doesn't match the schema | Throws `AiSchemaError` | `try/catch` |

### `try/catch` — now wired

`try/catch` was parsed but ignored before this release. Catch clauses now execute and the bound `err` variable carries error fields:

```keel
try {
  urgency = Ai.classify(email.body, as: Urgency) ?? Urgency.medium
} catch err: AiSchemaError {
  Io.notify("Unexpected LLM output: {err.got}")
  urgency = Urgency.medium
} catch err: Error {
  Io.notify("Failed: {err.message}")
}
```

**`AiSchemaError` fields:**

| Field | Type | Value |
|---|---|---|
| `message` | `str` | Human-readable description |
| `got` | `str` | The raw LLM output that failed to match |

`Error` is the catch-all for any other runtime error.

---

## v0.1.14 — 2026-05-06

### `for` loop `if` guard

`for` loops now accept an inline `if` filter, replacing the previous `where` keyword in that position:

```keel
for email in emails if email.unread { triage(email) }
for n in 1..10 if n % 2 == 0 { Io.show(n) }
```

**Breaking:** `for x in list where cond` no longer parses. Use `if` instead.

### `Time` namespace — full rework

`now` is no longer a keyword. The `Time` namespace now provides timezone-aware datetime handling with method syntax on values.

```keel
# Factories — namespace style
now  = Time.now()                          # UTC, millisecond precision
ny   = Time.now(tz: "America/New_York")    # IANA timezone
dt   = Time.parse("2026-05-06T09:00:00Z") # datetime? (none on failure)
dt2  = Time.parse("2026-05-06", tz: "UTC") # coerce naive string with tz:

# Methods on the datetime value
p    = dt.parts()           # {year, month, day, hour, minute, second, millisecond, tz}
s    = dt.format(as: "%Y-%m-%d")   # str?

# Operators
elapsed  = finish - start   # datetime - datetime → duration
deadline = Time.now() + 3.days
ok       = deadline > Time.now()

# Millisecond duration
short = 500.ms              # aliases: millis, millisecond, milliseconds
```

**Breaking changes from the earlier v0.1.14 Time stub:**
- `Time.format(dt, as:)` removed → use `dt.format(as:)`
- `Time.diff(a, b)` removed → use `a - b`
- `Time.parse()` rejects naive strings without a TZ offset — returns `none` instead of raising. Use `Time.parse(str, tz: name)` to coerce.

Naive strings (no UTC offset in the string) are rejected by design — all datetimes in Keel are timezone-aware.

---

## v0.1.13 — 2026-05-05

### Destructuring (§8.4)

All five destructuring forms from SPEC.md §8.4 are now implemented. No new keywords.

```keel
{urgency, category} = result             # struct shorthand
{urgency: u, category: c} = result       # struct rename
(label, count) = ("alpha", 42)           # tuple
for {from, subject} in emails { ... }    # in for loop
task handle({body, from}: Email) { ... } # in task params
```

Keyword-named fields (`from`, `state`, `in`, etc.) work in all positions. Missing struct fields and tuple arity mismatches are compile-time type errors.

See the [Variables & Expressions guide](guide/expressions.md#destructuring) for full documentation.

---

## v0.1.12 — 2026-05-04

### Range operator `..`

`start..end` produces an inclusive `list[int]`. Both bounds must be `int`.

```keel
for i in 1..5 {
  Io.notify("{i}")    # 1, 2, 3, 4, 5
}

xs = 0..3             # [0, 1, 2, 3]
xs.count()            # 4
```

- `5..3` → `[]` (empty when start > end)
- `4..4` → `[4]` (single element)
- Non-integer bounds are a type error at compile time
- All list methods work on ranges: `.filter`, `.map`, `.count`, etc.
- SPEC grammar updated: `RangeExpr <- AddExpr (".." AddExpr)?`

See the [Collections guide](guide/collections.md) for full documentation.

---

## v0.1.11 — 2026-05-04

### Memory — safe cross-process storage (**breaking path change**)

> **Breaking:** The persistent memory directory format changed. Move existing data manually (see below).

Persistent memory is now both path-safe and cross-process safe:

**New path format:** `~/.keel/memory/<stem>_<hash12>/<agent>.json`

The `<hash12>` is derived from the SHA-256 of the canonical source file path, ensuring two programs with the same filename in different directories never share a storage bucket.

**Cross-process writes are now safe.** Each `Memory.*` operation holds an advisory `flock` on a sidecar `<agent>.lock` file. Multiple concurrent `keel run` processes against the same agent serialize correctly.

**Crash durability.** Writes call `fsync` on the temp file before rename, and `fsync` on the parent directory after rename.

**Path validation.** Agent names containing `.`, `/`, `\`, or `\0` are rejected with a hard error (previously `debug_assert`).

**Migration:** data at the old path `~/.keel/memory/<stem>/<agent>.json` is not auto-migrated. Move it to the new location:

```bash
# find the new directory name for your program
keel run --print-memory-dir myprog.keel  # not yet available; use the hash formula
# or simply let Keel recreate it from scratch
```

See the [Agents guide](./guide/agents.md#memory--agent-memory-scope) for the updated path format and multi-process safety notes.

---

## v0.1.10 — 2026-05-03

### Memory namespace

`Memory.remember`, `Memory.recall`, and `Memory.forget` are now real — they were no-op stubs since v0.1.0.

The `@memory` attribute selects the scope:

- **`session`** (default) — in-process, cleared on restart.
- **`persistent`** — file-backed JSON at `~/.keel/memory/<stem>_<hash12>/<agent>.json`, survives restarts. (Path format updated in v0.1.11; the directory includes a SHA-256 hash of the source file path to avoid cross-program collisions.)
- **`none`** — `Memory.*` calls raise `CapabilityError`.

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

See the [Agents guide](./guide/agents.md#memory--agent-memory-scope) for full syntax and the `persistent` mode example.

---

## v0.1.9 — 2026-05-03

### `keel init` fixes & `stop(self)`

**`keel init` with no argument** now initializes in the current directory instead of creating a duplicate subdirectory.

**Path arguments** (`keel init /tmp/mybot`) now use the basename as the project name — previously the full path was injected into the agent name.

**Runnable scaffold** — the generated template no longer uses `Schedule.every`. It prints and exits immediately via `stop(self)`, so `keel run main.keel` works out of the box.

**`stop(self)`** — bare `self` now resolves to an `AgentRef` for the current agent anywhere inside an agent body.

```keel
agent Worker {
  @on_start {
    Io.show("Hello from Worker!")
    stop(self)
  }
}
```

---

### Tooling — Linter & Sharper Errors

New `keel lint` command and source-span diagnostics in `keel check`.

#### `keel lint` — style and best-practice checks

```bash
keel lint <file.keel>
keel lint --fix <file.keel>
```

Four rules ship in v0.1.9:

| Rule | Trigger |
|------|---------|
| Unused variable | binding assigned but never read |
| Uncalled task | `task` declared but never invoked |
| `Ai.*` outside agent | LLM call without `@role` / `@model` context |
| State written, never read | `self.x =` appears but `self.x` is never used |

`--fix` auto-removes unused variable assignments. Prefix a name with `_` to suppress the unused warning.

#### `keel check` — source spans in every diagnostic

Every error from `keel check` now includes a line:column pointer and an underlined source excerpt. Arity errors include the expected parameter names as a hint:

```
  × Type error
   ╭─[agent.keel:8:5]
 7 │   @on_start {
 8 │     greet(42)
   ·     ────┬────
   ·         ╰── task `greet` takes 1 argument(s), got 0 — expected: name
 9 │   }
   ╰────
```

---

## v0.1.8 — 2026-04-30
### Reactive Agents & Text Processing

HTTP webhook handling, in-memory caching, regex & string tools, LSP go-to-definition and rename.

#### `Cache` namespace

Process-scoped in-memory cache with optional TTL. Useful for deduplication, rate-limit tokens, and short-lived computed results across agents.

```keel
Cache.set("key", value, ttl: 5.minutes)
v = Cache.get("key")    # value or none
Cache.delete("key")
Cache.clear()
```

#### `Str` namespace

Regex matching and string manipulation:

```keel
Str.match(text, "\\d+")                 # bool
Str.extract(text, "(\\d+)")             # str? — first capture group
Str.truncate("hello world", 5)          # "hello…"
Str.pad("42", 6, char: "0")             # "000042"
```

#### `Http.serve` — webhook listener

React to inbound HTTP requests without polling:

```keel
Http.serve(8080, (req) => {
  Io.show("Got {req["method"]} {req["path"]}")
  { status: 200, body: "OK" }
})
```

The handler receives `{method, path, body}` and returns `{status, body}`. The event loop keeps running as long as at least one server is active.

#### LSP go-to-definition & rename

- **Go-to-definition** — jump to `task`, `agent`, and `type` declarations from any usage
- **Rename** — rename a user-defined symbol and all its usages in the open file (prelude names are blocked)

---

## v0.1.7 — 2026-04-30
### Structured Concurrency & Agent Constraints

File I/O, JSON processing, async task spawning, cron scheduling, and agent capability enforcement.

#### File namespace

Read, write, and list files on disk. The runtime creates intermediate directories automatically for writes.

```keel
File.write("data.txt", "Hello Keel")
content = File.read("data.txt")

if File.exists("data.txt") {
  Io.show(content)
}

entries = File.list("data/")
```

Methods: `read(path)`, `write(path, content)`, `exists(path)`, `list(dir)`.

#### Json namespace

Serialize and deserialize JSON. `parse` deserializes a JSON string into Keel values (maps, lists, scalars). `stringify` turns Keel values back into JSON.

```keel
data = Json.parse("{\"name\": \"Alice\", \"age\": 30}")
name = data["name"]

user = { name: "Bob", age: 25 }
json_str = Json.stringify(user)
```

#### Schedule.cron

Schedule tasks using 5-field cron expressions. Supports standard cron syntax for minute, hour, day, month, and weekday.

```keel
Schedule.cron("0 9 * * 1-5", () => {
  Io.show("Morning digest")
})

Schedule.cron("*/5 * * * *", () => {
  Io.show("Every 5 minutes")
})
```

#### Async namespace — structured concurrency

Spawn independent Tokio tasks and await completion. `spawn` returns a task handle; `join_all` awaits a list of handles; `select` races handles to the first completion.

```keel
task1 = Async.spawn(() => {
  result = Http.get("https://api1.example.com")
  Io.show(result)
})

task2 = Async.spawn(() => {
  result = Http.get("https://api2.example.com")
  Io.show(result)
})

results = Async.join_all([task1, task2])
Io.show("All done")
```

#### @tools capability gating

Restrict which prelude namespaces are accessible inside an agent. Calls to unlisted namespaces raise `CapabilityError` at runtime.

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

If no `@tools` attribute is specified, all namespaces are allowed.

#### @limits agent attributes

Extract and enforce resource limits (timeout, max_tokens, max_cost) on a per-agent basis. The infrastructure for extracting limits is in place; timeout wraps calls via `Control.with_timeout`.

```keel
agent LimitedAgent {
  @limits { timeout: 30s, max_tokens: 1000, max_cost: 5.0 }

  @on_start {
    response = Ai.prompt("...")
  }
}
```

#### LSP completion

`textDocument/completion` now suggests prelude namespace names, method names, and keywords. Triggered on `.` or manual invocation.

#### New example programs

Five example programs demonstrate the new v0.1.7 features:

- `file_processing.keel` — File read, write, exists, list
- `json_processing.keel` — JSON parse and stringify
- `cron_schedule.keel` — Cron expression scheduling
- `parallel_execution.keel` — Async task spawning and joining
- `capability_gating.keel` — @tools capability restrictions

---

## v0.1.6 — 2026-04-28
### Wiring & ergonomics

Every primitive that was a stub in 0.1.5 now does what its name promises.

#### Nested string literals inside `{interp}`

The lexer used to terminate a string at the first `"` it saw, even one
hiding inside a `{...}` slot. The lexer now scans slot bodies with brace
depth tracking and recursively handles nested `"..."`:

```keel
name = "world"
Io.show("hi {"there {name}"}")        # → "hi there world"
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
on expiry. `retry` surfaces the last attempt's error if every attempt fails.

#### `Agent.broadcast(team, data)`

Tag agents with `@team [...]` and dispatch a single event to every
member of a named team:

```keel
agent Alpha { @team ["frontline"]  on alert(m: str) { ... } }
agent Beta  { @team ["frontline"]  on alert(m: str) { ... } }
agent Gamma { @team ["backoffice"] on alert(m: str) { ... } }

Agent.broadcast("frontline", "incident", event: "alert")
# Alpha and Beta fire; Gamma stays silent.
```

#### `Email.archive(message)`

`Email.archive` performs an IMAP UID MOVE (with COPY + `\Deleted` +
EXPUNGE fallback for servers without the MOVE extension). The
destination folder is `Archive` by default; override with the
`IMAP_ARCHIVE_FOLDER` env var. `Email.fetch` now returns each message's
UID under the `uid` key so `archive` can target the right one.

#### `map[K, V]` method inference

Map literals support the common operations on both the type checker and
runtime side.

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
the cursor — `let`-bindings, function parameters, agent state fields,
and prelude namespaces (`Io`, `Ai`, `Control`, …) all light up.

---

## v0.1.5 — 2026-04-27
### Type checker hardening

Four new checks land in the type checker; zero breaking changes to valid programs.

#### Nullable safety

`T?` is now enforced at assignment and return sites. Passing a nullable value where a non-nullable is expected is a compile-time error. Use `!` to assert non-null (throws `NullError` at runtime if `none`) or `??` to provide a fallback.

```keel
task t() {
  x: str = Env.get("KEY")          # error: expected str, got str?
  y: str = Env.get("KEY")!         # ok — throws if none
  z: str = Env.get("KEY") ?? ""    # ok — falls back to ""
}
```

#### Return-type matching

`return expr` is now checked against the task's declared `-> T`.

```keel
task greet() -> str {
  return 42     # error: return value: expected str, got int
}
```

#### Struct field checks

Struct literals are now checked against named `type` declarations. Missing required fields are reported; extra fields are allowed.

```keel
type Person { name: str, age: int }

task t() {
  p: Person = { name: "Alice" }           # error: missing field `age`
  q: Person = { name: "Bob", age: 30 }    # ok
}
```

#### List and string method type inference

Method calls on `list[T]` and `str` now return typed results:

| Method | Return type |
|---|---|
| `list.push(x)` / `list.filter(fn)` | `list[T]` |
| `list.len()` | `int` |
| `list.first()` / `list.last()` | `T?` |
| `str.upper()` / `str.trim()` / `str.replace()` | `str` |
| `str.split(sep)` | `list[str]` |
| `str.len()` | `int` |

---

## v0.1.4 — 2026-04-27
### Parser hardening — every expression-level feature now works

#### `if`-as-expression

`if` can now appear on any right-hand side, not just as a standalone statement. `else if` chains are fully supported.

```keel
label = if score > 0.8 { "high" } else if score > 0.5 { "medium" } else { "low" }
```

#### `let` type annotations validated

Declaring a type on a binding now produces a compile-time error when the value type doesn't match.

```keel
x: int = "hello"   # error: expected int, got str
y: str = "hello"   # ok
```

#### `list + list` and `list.push(item)`

Lists now support concatenation with `+` and a functional `push` that returns a new list (no mutation).

```keel
all  = ["a", "b"] + ["c", "d"]   # ["a", "b", "c", "d"]
more = all.push("e")              # ["a", "b", "c", "d", "e"]
```

#### Full string interpolation

`{…}` slots now accept any expression: method calls, binary operations, chained calls.

```keel
summary = "Items: {cart.count()}, subtotal: {price * qty}"
```

#### `@on_stop` lifecycle hook

Agents can now declare a shutdown block that runs before the agent is removed from the runtime.

```keel
agent Logger {
  @on_stop { Io.show("Logger shutting down") }
}
```

#### `Agent.delegate(target, task, args)`

Posts a named task event to another agent's mailbox; the receiving agent handles it via its `on <task>` handler.

```keel
Agent.delegate(Processor, "handle", payload)
```

#### `Search`, `Db`, `Time` stub namespaces

Calling any method on these namespaces now raises a clear `"… is planned for v0.2"` error instead of a generic crash.

---

## v0.1.3 — 2026-04-26
### Declarations reach the model

#### `@rules` injected into every LLM system prompt

```keel
agent Support {
  @role "Customer support specialist"
  @rules [
    "Never reveal internal pricing logic",
    "Escalate if the user expresses frustration 3+ times"
  ]
}
```

Rules are prepended as a bullet list in the system prompt, between the role preamble and the operation instructions. Enable `KEEL_TRACE=1` to see the full prompt for every call.

#### `Ai.summarize` — `format:` and `max:` wired

```keel
summary = Ai.summarize(article, format: bullets, max: 5, unit: sentences)
```

#### `Ai.prompt` — `response_format: json` wired

Appends "Respond with valid JSON only" to the system prompt and validates the reply.

```keel
score = Ai.prompt(system: "Rate 1–10.", user: review, response_format: json)
```

#### `Ai.extract(x, as: T)` — schema derived from struct type

```keel
type Invoice { vendor: str, amount: float, date: str }
result = Ai.extract("Invoice from ACME $99.99 on 2026-01-10", as: Invoice)
```

---

## v0.1.2 — 2026-04-19
### Internal hardening

- Bumped to **Rust edition 2024** (requires rustc ≥ 1.85).
- Runtime config decoupled from environment: `--trace` / `--log-level` / `Log.set_level` now go through typed setters, removing all `unsafe` env mutation.
- Release pipeline: Homebrew tap push now uses a short-lived GitHub App installation token instead of a long-lived PAT.

---

## v0.1.1 — 2026-04-19
### Release

- Dropped prebuilt macOS Intel binaries. Apple Silicon and Linux x86_64 are shipped; Intel Macs build from source (`cargo build --release`).

---

## v0.1.0 — First public alpha
### Everything is new

First release of the language, runtime, standard library, and tooling.

**Language highlights:**
- Statically typed, inference-first — 28 reserved keywords
- `agent` is the only concurrency primitive (actor model, serial mailbox, isolated `self` state)
- Prelude-as-stdlib — `Ai`, `Io`, `Email`, `Http`, `Schedule`, `Agent`, `Log`, … in scope everywhere, no `use` needed
- Algebraic types: simple enums, rich enums with per-variant fields, structural interfaces
- Exhaustive `when` pattern matching, duration literals (`5.minutes`), nullable syntax (`T?`, `??`, `?.`)

**Tooling:**
- `keel run` — execute, with `--trace` and `--log-level`
- `keel check` — static analysis (scope, arity, enum exhaustiveness)
- `keel fmt` — idempotent AST pretty-printer
- `keel repl` — interactive, history-aware
- `keel lsp` — diagnostics over stdio (VS Code extension included)

**Distribution:**
- GitHub releases for `aarch64-apple-darwin` + `x86_64-unknown-linux-gnu`
- `curl https://keel-lang.dev/install.sh | sh`
- `brew install keel-lang/tap/keel`
