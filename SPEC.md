# Keel Language Specification — v0.1 (Alpha)

> **Status: Alpha.** Keel is in early design. The language is **not yet stable** and has **no production users**. Expect breaking changes between 0.x releases. This document is the design target for v0.1; [CHANGELOG.md](CHANGELOG.md) tracks implementation status, and the [GitHub project board](https://github.com/orgs/keel-lang/projects/1) tracks planned work.

---

## 0. The Shape of Keel

Keel is a programming language for building AI agents. Two ideas define it:

1. **The actor model is core.** An `agent` is the primitive unit of concurrency — a serial-handler mailbox with isolated mutable state. This is the only primitive that can't be a library.
2. **Everything else is a library.** AI calls, scheduling, I/O, HTTP, memory, search, tool integration — all live in a standard library that ships with the runtime as modules. One import line — `use std/ai` — and `ai.classify(...)` works. Stdlib modules and local files are the same concept (§20).

The import is one line of ceremony, after which `ai.classify(...)` reads as if `classify` were a keyword — but the compiler doesn't know or care what `ai` is. That keeps the core language small (fewer keywords, fewer parser special cases, fewer type-inference rules) while keeping the ergonomics. Top-level statements form a file's **implicit main**: they run when the file is executed directly, never on import, so every `.keel` file is both runnable and importable with zero boilerplate.

### Design principles

1. **Small core, deep stdlib.** Every feature that can be a library is one. The core earns its keep through the type system, the compiler, or the actor runtime — not through surface syntax convenience.
2. **Static typing with full inference as the design target.** The alpha checker already catches core mismatches and deliberately leaves some unsupported cases as `Unknown`; the [CHANGELOG.md](CHANGELOG.md) records what each release added to checker coverage.
3. **No silent fallbacks.** Operations that can fail return nullable types (`T?`) or throw typed errors. The caller handles absence with `??` or `when`, and catches errors with `try/catch`.
4. **Tooling from day one.** Every feature is designed so the LSP can autocomplete, go-to-def, rename, and surface diagnostics.
5. **Escape hatches are explicit.** `dynamic`, `extern`, `prompt` exist for real needs but must be opted into visibly.

### Core vs. stdlib arbitrage test

> **For every feature, ask: can a library replicate this with identical safety, ergonomics, and performance?**
>
> - Yes → stdlib.
> - No → core.

Applied ruthlessly, this test keeps the reserved-keyword list small.

---

## 1. Program Structure

A Keel program is a `.keel` file containing top-level declarations. No `main()` — execution begins at the first top-level statement (typically `run(MyAgent)`).

```keel
# my_agent.keel

agent Greeter {
  @role "You greet people warmly"
}

run(Greeter)
```

### File extension: `.keel`
### Comments: `#` for single-line, `## ... ##` for multi-line

### Top-level declarations

A file may contain, in any order:
- `agent` declarations
- `task` declarations (free-standing)
- `type` declarations (structs, enums, aliases)
- `interface` declarations (protocols)
- `extern` declarations
- `use` imports
- Top-level statements (`run(...)`, variable bindings, etc.)

---

## 2. Type System

Keel uses a **structural type system with full inference** as its design target. In the current alpha, the checker covers the core language and falls back to `Unknown` in some unsupported cases; those gaps are tracked in [GitHub Issues](https://github.com/keel-lang/keel/issues).

### 2.1 Design principles

1. **Nominal identity for named types, structural compatibility for anonymous shapes.** Two declared struct types `A` and `B` with identical fields are distinct types — `A` is not assignable to `B`. Anonymous struct literals `{x: 1}` are still structurally compatible with any named struct that has the required fields. No explicit `implements`.
2. **Full inference.** Initializers, returns, and stdlib signatures drive inference. Explicit annotations override.
3. **Algebraic data types.** Enums can carry associated data per variant.
4. **Nullable safety.** Types are non-nullable by default. `?` marks a type as nullable.
5. **No implicit `any`.** `dynamic` is the one escape hatch, and it must be explicitly opted into.

### 2.2 Primitive types

| Type | Example | Notes |
|---|---|---|
| `int` | `42` | 64-bit integer |
| `float` | `3.14` | 64-bit float |
| `str` | `"hello"` | UTF-8, interpolation `{expr}` or `{expr:spec}`, escapes `\n \t \r \" \\ \{ \}` |
| `bool` | `true`, `false` | |
| `none` | `none` | Unit type / absence value |
| `duration` | `5.minutes`, `2.hours` | Duration literals |
| `datetime` | `@2026-04-15`, `@monday_9am` | Time literals |
| `Uuid`     | `uuid()` | UUID value; implements `Stringable` — interpolates as `"xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"` |
| `dynamic` | — | FFI/interop boundary only |

**Built-in constants:** `true`, `false`, `none`.

**`none` semantics.** `none` is both the unit type and the nullable-empty value. `none?` is equivalent to `none`. The tuple unit `()` is equivalent to `none`.

**Multi-line strings.** Triple-quoted `"""..."""` preserves newlines and indentation. Same interpolation and escape rules.

**Format specifiers.** An interpolation slot may include a format spec after a colon: `{expr:spec}`. The spec is a Python-style mini-language:

| Spec | Effect | Example |
|---|---|---|
| `:.Nf` | Float with N decimal places (auto-promotes `int` → `float`) | `{pi:.2f}` → `"3.14"` |
| `:>N` | Right-align in N chars, space-padded | `{n:>8}` → `"      42"` |
| `:<N` | Left-align in N chars, space-padded | `{s:<8}` → `"hi      "` |
| `:^N` | Center in N chars, space-padded | `{s:^8}` → `"   hi   "` |
| `:N` | Bare width — right-align shorthand | `{n:8}` → `"      42"` |

Specs may combine alignment and precision: `{x:>10.2f}`. A malformed spec is a runtime error. The `:` that starts a spec must be at the outermost bracket depth, so named arguments inside the slot (e.g., `{f(key: v):>10}`) are not confused with the spec separator.

### 2.3 Collection types

```keel
nums: list[int]      = [1, 2, 3]
info: map[str, str]  = {name: "Zied", role: "builder"}
ids: set[int]        = set[1, 2, 3]
```

Rules: `[...]` is a list. `{k: v}` is a map. `set[...]` is a set (the only place `set` is special: it's a keyword-like form, not a type application).

**Set semantics:** a set holds at most one element equal to any given value, using the same `==` every other comparison uses — so `set[1, 1, 2]` has two elements, and structs deduplicate by field values. Unlike map keys, the element type `T` is unrestricted: `set[float]` and `set[SomeStruct]` are legal. (One consequence of reusing `==`: `float` NaN is never equal to itself, so NaN elements never deduplicate.) Elements keep **first-insertion order**, which is the order a set displays, iterates (`for x in s`), and spreads (`...s`) in — no sorting is applied.

**Subscript access** (`expr[index]`): lists and strings support integer indexing. The result type is `T` for `list[T]` and `str` for strings. Out-of-bounds and negative indices are runtime errors — check `len()` first or use `try/catch` when the index may be invalid:

```keel
items = [10, 20, 30]
v = items[1]   # int — 20
# items[99]    # runtime error: index 99 out of bounds (length 3)

word = "keel"
ch = word[0]   # str — "k"
# word[10]     # runtime error: string index 10 out of bounds (length 4)
```

**Built-in collection operations** (methods, enabled by lambdas):

| Method | Signature | Notes |
|---|---|---|
| `.len()` / `.count()` | `int` | |
| `.first()`, `.last()` | `T?` | |
| `.is_empty()` | `bool` | |
| `.push(v)` | `list[T]` | returns new list |
| `.map(fn)` | `list[T].(T → U) → list[U]` | |
| `.filter(fn)` | `list[T].(T → bool) → list[T]` | |
| `.find(fn)` | `list[T].(T → bool) → T?` | first match or `none` |
| `.any(fn)`, `.all(fn)` | `list[T].(T → bool) → bool` | |
| `.reduce(fn, init)` | `list[T].((U, T) → U, U) → U` | `fn` receives `(acc, elem)` |
| `.sum()` | `int\|float` | numeric lists only |
| `.min()`, `.max()` | `T?` | `none` on empty; accepts optional `by: fn` key arg: `items.min(by: x => x.score)` |
| `.join(sep)` | `str` | `list[str]` preferred |
| `.sort()` | `list[T]` | natural order (int, float, str); or `Comparable` impl |
| `.sort(by: fn)` | `list[T]` | sort by key: `items.sort(by: x => x.score)`; key must be int, float, or str; ascending only |
| `.reverse()` | `list[T]` | |
| `.flatten()` | `list[T]` | unwraps one nesting level |
| `.take(n)` | `list[T]` | first `n` elements |
| `.skip(n)` | `list[T]` | all but first `n` |
| `.contains(v)` | `bool` | |
| `.zip(list[U])` | `list[(T, U)]` | stops at the shorter list |

Maps expose `.count`/`.len`/`.size`, `.keys`, `.values`, `.get(k) → V?`, `.contains(k)`/`.has(k)`, `.is_empty`, and `.insert(k, v) → map[K, V]`. Sets expose `.count`/`.len`/`.size`, `.contains(v)`, `.is_empty`, and `.add(v) → set[T]`.

**Mutation methods return new containers.** `.push`, `.insert`, and `.add` never modify the receiver — like every value method, they return a fresh container and the result must be bound to take effect. `s.add(v)` on its own is a no-op; write `s = s.add(v)`. `.insert` overwrites an existing key; `.add` on an element already present is a no-op, not an error.

Sets also accept the read-only list methods — `.map`, `.filter`, `.find`, `.any`, `.all`, `.reduce`, `.sum`, `.min`, `.max`, `.join`, `.sort`, `.reverse`, `.flatten`, `.take`, `.skip`, `.zip`, `.first`, `.last` — operating over the elements in insertion order. Each yields a list or a scalar, never a set. `.push` is not among them: use `.add`.

**Map key constraints:** The key type `K` in `map[K, V]` must be a hashable primitive — `str`, `int`, or `bool`. Using `float` is a compile-time error (NaN violates hash equality). Nullable types (`K?`) are rejected as keys. Struct and enum keys are not supported in v0.1; they will be enabled in v0.2 via `interface Hashable`. The runtime stores each distinct key type — `map[int, str]` has integer keys, `map[bool, str]` has boolean keys; `.keys()` returns values of the declared key type.

**String methods:** `.len()` / `.length`, `.is_empty()`, `.contains(s)`, `.starts_with(s)`, `.ends_with(s)`, `.trim()`, `.trim_start()`, `.trim_end()`, `.upper()`, `.lower()`, `.repeat(n)`, `.slice(start, end?)`, `.index_of(needle)` → `int?`, `.split(sep)`, `.replace(old, new)`, `.to_int()` → `int?`, `.to_float()` → `float?`, `.to_str()`, `.truncate(max)` → `str`, `.pad(width, char?)` → `str`, `.matches(pattern)` → `bool`, `.extract(pattern)` → `str?`, `.find_all(pattern)` → `list[str]`, `.sub(pattern, replacement)` → `str`. Patterns (`matches`, `extract`, `find_all`, `sub`) use Rust `regex` crate syntax — no look-behind.

**Conversions:** `.to_int()`, `.to_float()`, `.to_str()`. Fallible conversions return nullable (`str.to_int() -> int?`).

**Numeric value methods** (available on both `int` and `float`; `floor`/`ceil`/`round` are no-ops on `int` and return the same type):

| Method | Types | Returns | Notes |
|---|---|---|---|
| `.abs()` | `int`, `float` | same type | Absolute value; `-3.abs()` → `3` |
| `.floor()` | `int`, `float` | same type | Round toward −∞; `3.7.floor()` → `3.0`; no-op on `int` |
| `.ceil()` | `int`, `float` | same type | Round toward +∞; `3.2.ceil()` → `4.0`; no-op on `int` |
| `.round()` | `int`, `float` | same type | Round to nearest; `3.5.round()` → `4.0`; no-op on `int` |

```keel
price = -3.75
price.abs()           # 3.75
price.abs().ceil()    # 4.0  — chains naturally
count = 5
count.abs()           # 5    — no-op, returns int
```

### 2.4 Struct types (structural records)

```keel
type EmailInfo {
  sender: str
  subject: str
  body: str
  unread: bool
}

# Inline
task triage(email: {body: str, from: str}) -> Urgency { ... }
```

**Named struct identity.** Each declared struct type is a distinct type. `type Point { x: int, y: int }` and `type Offset { x: int, y: int }` are not interchangeable even though they have the same fields.

**Anonymous literals are structurally compatible.** An untyped literal `{x: 1, y: 2}` is assignable to any named struct type that has the required fields. Assign to a typed variable or pass to a typed parameter to tag the value with its declared type.

**Width subtyping.** An anonymous literal assigned to a named struct type `B` must have all required fields of `B` with compatible types. Extra fields are allowed.

**Field expressions evaluate in source order.** A struct literal's fields are matched to the target type by *name*, so they may be written in any order — but their expressions are evaluated left to right in the order written, not in the target type's declared field order. The distinction is observable when a field expression has a side effect:

```keel
type P { x: int, y: int }

task note(s: str) -> int {
  io.show(s)
  return 1
}

p: P = { y: note("first"), x: note("second") }   # prints "first" then "second"
```

**Impl dispatch requires a type tag.** `impl` methods are dispatched by the value's declared type name. A bare literal `{val: 30}` is an untagged map with no type name — to dispatch an `impl` method, the value must first be tagged by assigning to a typed variable or passing to a typed parameter:

```keel
type Score { val: int }
impl Comparable for Score {
  task compare(self, other: Score) -> int { self.val - other.val }
}

task run() {
  scores: list[Score] = [{ val: 30 }, { val: 10 }, { val: 20 }]
  sorted = scores.sort()   # uses Comparable.compare
}
```

**Generic structs:**

```keel
type Paginated[T] {
  items: list[T]
  page: int
  has_more: bool
}
```

**Spread-update:** Create a new value that copies all fields from a base expression and overrides specific fields. The base must be a non-`none` struct or map value.

```keel
type Order { id: str, status: str, amount: float }

o: Order = { id: "ord-1", status: "pending", amount: 9.99 }
filled   = { ...o, status: "filled" }   # id and amount copied, status replaced
copy     = { ...o }                     # full copy, no overrides

# Also works on map[K, V] — keys are unrestricted, values must match the map's value type.
m: map[str, int] = { "a": 1, "b": 2 }
m2 = { ...m, "c": 3 }                  # adds key "c"; result is still map[str, int]
```

Rules:
- Exactly one `...base` spread, and it must appear first.
- Zero or more `field: value` overrides follow, separated by commas or newlines.
- **Struct base:** override field names must exist in the base struct; unknown fields are a type error (and a runtime error on dynamic paths). The result preserves the base's type tag so `impl` dispatch continues to work.
- **Map base:** any key may be added or overridden; override values must match the map's declared value type. The result is the same `map[K, V]` type.
- Spreading `none` raises at runtime.
- **Evaluation order:** the base is evaluated once, first; then each override in the order written — the same source-order rule as a plain struct literal.

### 2.5 Enum types (algebraic data types)

```keel
# Simple variants
type Urgency = low | medium | high | critical

# Rich variants with associated data
type Action =
  | reply { to: str, tone: str }
  | forward { to: str }
  | archive
  | escalate { reason: str, urgency: Urgency }
```

**Construction:** `Action.reply { to: "x", tone: "y" }`. Data-less variants: `Action.archive`.

**Pattern matching** is exhaustive (see §8.2). Rich variant fields are destructured in `when` arms, not accessed via dot. A destructured name must be a field the variant declares; naming an unknown field (e.g. a typo) is a compile-time error rather than a silent `none` binding.

**Generic enums:**

```keel
type Pair[A, B] =
  | both { first: A, second: B }
  | only_first { value: A }
  | only_second { value: B }
```

### 2.6 Type aliases

```keel
type Timestamp = datetime
type ContactEmail = str
type Handler = (Message) -> str
```

Aliases are structurally transparent. `ContactEmail` and `str` are interchangeable. For nominal distinction, use a wrapper struct.

### 2.7 Nullable types

```keel
name: str   = "Keel"
alias: str? = none

subject = email?.subject           # str? — none-safe field access
subject = email?.subject ?? "(none)"  # str — default via ??
subject = email!.subject           # str — throws NullError on none (unsafe)
```

Stdlib and AI operations return nullable types when they can fail. Use `??` or `when` to handle absence. AI call failures throw typed `AiError` variants; use `try/catch` to distinguish causes.

### 2.8 Tuple types

```keel
pair: (str, int) = ("hello", 42)
(name, count) = pair             # destructure
x = pair.0                        # positional access
```

Tuples are structural, immutable. Single-element tuples are not a thing (`(str)` is `str`). `()` is `none`.

Positional access is bounds-checked at compile time — `pair.2` on a 2-tuple is a type error, not a runtime one — and `.N` is valid only on tuples. Lists and maps use subscripts (`xs[0]`), which is why `xs.0` is rejected. `?.` works too: `maybe_pair?.0`.

Nested positional access chains to any depth, with or without parentheses:

```keel
t: ((int, int), int) = ((1, 2), 3)
b = t.0.1                         # 2
c = (t.0).1                       # 2 — identical
```

Each index is bounds-checked against its own tuple. `?.` applies to the index it is written on, so a nullable at every level needs `maybe?.0?.1`.

### 2.9 The `dynamic` type (FFI/interop only)

`dynamic` exists for untyped boundaries: `extern` returns, `prompt as dynamic`, raw SQL rows, and JSON/cache interop. It must always be explicitly written — there is no implicit path to `dynamic`.

```keel
extern task parse_legacy(data: str) -> dynamic from "legacy"

raw = ai.prompt(...) as dynamic       # must opt in
info: MyStruct = raw as MyStruct      # narrow with runtime check
```

`dynamic` defeats autocomplete and type checking. Narrow as early as possible. The compiler warns on `dynamic` use outside the explicit escape hatches.

**Strict runtime arguments.** Runtime APIs enforce their declared arguments. A dynamic value passed to `file.read(path: str)`, `cache.get(key: str)`, `json.parse(s: str)`, or another typed API is rejected when its runtime type does not match. Required namespace and value-method arguments also raise an error when omitted. Display formatting is explicit: interpolation, `io.*`, and `log.*` may render arbitrary values, while data APIs do not silently stringify them.

**`json.parse` return-type semantics.** `json.parse(s)` returns an untyped value — the type checker does not infer a precise return type. Narrow with `as T` before use, or annotate the binding as `dynamic` to opt out of static typing at that site. The JSON-to-Keel runtime mapping is:

| JSON type | Keel runtime value |
|---|---|
| object `{}` | field-accessible map — `parsed.fieldName` works at runtime |
| array `[]` | `list[dynamic]` — index with `parsed[i]`, iterate with `for` |
| number (integer) | `int` |
| number (float) | `float` |
| string | `str` |
| boolean | `bool` |
| null | `none` |

Named-field access (`parsed.price`) resolves at runtime; a missing key raises rather than returning `none`. Narrow as early as possible with `as T` or by reading individual fields:

```keel
body = http.get("https://api.example.com/ticker")?.body ?? ""
data = json.parse(body) as dynamic
price  = (data.price as str).to_float() ?? 0.0  # str field → float
volume = data.volume as int                      # int field
rows   = data.candles as list[dynamic]           # array field
for row in rows {
  close = (row as list[dynamic])[4] as str       # nested array element
}
```

In strict mode (`keel check --strict`), an unannotated `json.parse` binding is flagged because its type cannot be statically inferred. Two accepted escape hatches silence the warning:

```keel
# Cast form — annotate the cast target
data = json.parse(body) as dynamic

# Annotation form — declare the binding type explicitly
data: dynamic = json.parse(body)
```

Both are accepted by strict mode because `dynamic` is an intentional programmer choice, not a checker gap. `Unknown` (an unannotated, un-narrowed result) is what triggers the strict diagnostic.

**`cache.get` return-type semantics.** `cache.get(key: str) -> dynamic?` returns the stored value at its original type, or `none` if the key is absent or the entry has expired. The stored type is preserved exactly — a value written as `str` is read back as `str`, a value written as `int` is read back as `int`. Use `as T` to recover a concrete type:

```keel
cache.set("price", "50000.12")
raw = cache.get("price")                # dynamic?
if raw != none {
  price = raw as str                    # "50000.12"
}

cache.set("count", 42)
n = (cache.get("count") ?? 0) as int   # 42
```

### 2.10 Built-in runtime types

Built-in runtime types, available without imports:

```keel
type Message {
  from: str
  body: str
  channel: str?
  timestamp: datetime?
}

type SearchResult { title: str, url: str, snippet: str }

type HttpResponse {
  status: int
  body: str
  headers: map[str, str]
}
# HttpResponse.is_ok : bool
# HttpResponse.json_as[T]() : T?

type Decision[T] { choice: T, reason: str, confidence: float }

# Uuid — distinct type, not str; implements Stringable
# Construction via uuid.v4(), uuid.v7(), uuid.v5(ns:, name:), or uuid.v4() shorthand
# uuid.v4() is an alias for uuid.v4()
# Methods: .version() -> int, .to_str() -> str, .format(as: "hyphenated"|"simple"|"urn") -> str
# uuid.parse(s: str) -> Uuid?  — none if invalid format
# Predefined namespace constants: uuid.DNS, uuid.URL, uuid.OID, uuid.X500

type Error =
  # Namespace-specific — catch these to handle a specific failure domain
  | FileError { message: str }
  | CsvError { message: str }
  | DbError { message: str }
  | CacheError { message: str }
  | MathError { message: str }
  | MemoryError { message: str }
  | EmailError { message: str }
  | HttpError { message: str }
  | ShellError { message: str }
  | JsonError { message: str }
  | EnvError { message: str }
  | AiError { message: str, reason: str }   # reason: "unavailable" | "provider"
  | AiSchemaError { message: str, got: str }
  # Cross-namespace — catch these for general conditions
  | CapabilityError { message: str }   # @tools restriction
  | TimeoutError { message: str }      # control.with_timeout exceeded
  | DeadlineError { message: str }     # control.with_deadline exceeded
  | UserRaised { message: str }        # raise statement
  | RuntimeBusy { message: str }       # event queue full
# All variants carry at minimum message: str
```

### 2.11 Variable bindings and mutability

Immutable by default. `=` creates a binding. Rebinding in the same scope **shadows** the previous binding (the old value is untouched, the name now points to a new value — Rust-style).

```keel
name = "Keel"
name = "Other"    # shadowing, not mutation
```

**The one exception: agent `state` fields**, accessed via `self.`:

```keel
self.count = self.count + 1
```

`self` is only available inside agent bodies. Top-level tasks have no `self`.

---

## 3. The Standard Library

The Keel standard library lives in a set of modules imported with `use std/<name>` (§20). `use std/ai` binds `ai`; `ai.classify(...)` is then an ordinary function call.

### 3.1 Why modules

- **Small core.** The compiler doesn't know about `classify`, `fetch`, or `every`. Those are stdlib function calls. Parser, lexer, and type checker stay free of domain-specific special cases.
- **Explicit dependencies.** A file's imports are its capability surface — auditable at a glance, and gated per agent via `@tools`.
- **Keyword feel.** `ai.classify(...)` stays ceremony-free; the import is one line and autocomplete takes care of the rest.
- **Swappable implementations.** Stdlib functions dispatch through **interfaces** (§5). Users can install their own LLM provider, scheduler, memory store, or HTTP client without leaving the language.
- **No grammatical ambiguity.** `fetch x where y` required whole-grammar disambiguation. `http.get(x, where: y)` is unambiguous and tool-friendly.

### 3.2 std modules (v0.1)

| Module | Purpose | Key operations |
|---|---|---|
| `std/ai` | LLM-backed operations | `classify`, `extract`, `summarize`, `draft`, `translate`, `decide`, `prompt`, `embed` |
| `std/io` | Human interaction | `ask`, `confirm`, `notify`, `show` |
| `std/http` | HTTP client | `get`, `post`, `request` |
| `std/email` | IMAP/SMTP | `fetch`, `send`, `archive` |
| `std/search` | Web search providers | `web(query)`, custom providers via interface |
| `std/db` | SQLite databases | `connect(url) -> DbConnection`, `conn.query(sql, params?) -> list[map[str,dynamic]]`, `conn.exec(sql, params?) -> int` |
| `std/memory` | Per-agent key-value store | `remember(key, value)`, `recall(key) -> Value?`, `forget(key)` |
| `std/file` | Local filesystem | `read`, `write`, `exists`, `list`, `mkdir`, `remove`, `copy`, `move`, `glob`, `mktemp` |
| `std/schedule` | Time-based scheduling | `every`, `after`, `at`, `cron` |
| `std/async` | Structured concurrency | `spawn`, `join_all`, `select`, `sleep` |
| `std/control` | Control combinators | `retry`, `with_timeout`, `with_deadline` |
| `std/env` | Environment and config | `get(name)`, `require(name)` |
| `std/time` | Time utilities | `now()`, `parse`, `format`, `diff`, duration math |
| `std/log` | Structured logging | `info`, `warn`, `error`, `debug` |
| `std/random` | Pseudo-random generation | `float()`, `int(min:, max:)`, `bool()` |
| `std/uuid` | UUID constructors (the `Uuid` *type* is built in) | `v4()`, `v7()`, `v5(ns:, name:)`, `parse(s)` |
| `std/crypto` | Cryptographic primitives | `sha256(data)`, `hmac_sha256(key, data)`, `token(bytes:)`, `random_bytes(n)` |
| `std/math` | Transcendental and power functions | `PI()`, `E()`, `sqrt(x)`, `pow(x, y)`, `exp(x)`, `log(x)`, `log2(x)`, `log10(x)`, `sin(x)`, `cos(x)`, `tan(x)`, `asin(x)`, `acos(x)`, `atan(x)`, `atan2(y, x)` |
| `std/csv` | CSV serialization | `parse(text)`, `parse_records(text)`, `stringify(rows)` |
| `std/testing` | Test doubles | `mock(module.method)` (§19) |

Agent lifecycle and messaging — `run`, `stop`, `send(target, message)`, `delegate`, `broadcast` — are **built into the language**, always in scope without imports.
| `std/shell` | Subprocess bridge | `run(cmd, stdin:?, cwd:?) -> { stdout, stderr, exit_code }` |

### 3.3 Built-in free functions

A small set of functions live directly in the root scope — no import needed:

| Function | Signature | Returns | Notes |
|---|---|---|---|
| `run(agent)` / `stop(agent)` | `(agent) -> none` | `none` | Agent lifecycle |
| `send(agent, msg)` / `delegate(...)` / `broadcast(team, msg)` | — | `none` | Agent messaging (§4) |
| `min(...)` | `(...items: T, by: ((T) -> any)? = none) -> T?` | `T?` | Minimum; `none` on empty |
| `max(...)` | `(...items: T, by: ((T) -> any)? = none) -> T?` | `T?` | Maximum; `none` on empty |
| `typeof(x)` | `(any) -> str` | `str` | Runtime type name: `"int"`, `"float"`, `"str"`, `"bool"`, `"none"`, `"list"`, `"map"`, `"duration"`, `"Uuid"`, or the declared name for structs and enums (`"Point"`, `"Color"`) |

```keel
min(3, 1, 4)                          # 1
max(3, 1, 4)                          # 4

scores = [4, 9, 2, 7]
min(...scores)                        # 2  — spread a list
max(...scores, 99)                    # 99 — spread + extra value
min(...scores, ...more_scores)        # merge two lists, find min

min(people, by: p => p.age)          # person with lowest age
max(products, by: p => p.price)      # most expensive product

typeof(42)                            # "int"
typeof(3.14)                          # "float"
typeof("hi")                          # "str"
type Point { x: int, y: int }
p: Point = { x: 1, y: 2 }
typeof(p)                             # "Point"
```

`min` / `max` return `T?` — an empty input (no args, or all spreads empty) yields `none`.

### 3.5 Module names are identifiers, not keywords

`ai`, `io`, `schedule`, etc. are **identifiers** bound by `use std/<name>` declarations — they do not appear in the reserved keyword list (§10). A lexical binding can shadow a module binding inside a body (`conn = db.connect(...)` is the idiomatic connection pattern; rebinding `db` itself is legal, if unwise). This is the crucial difference: the language doesn't know about `ai`. The standard library does. See §20 for the module system.

### 3.6 Example: three imports, everything you need

```keel
use std/ai
use std/io
use std/email
use std/schedule

type Urgency = low | medium | high | critical

agent EmailBot {
  @role "Professional email triage"

  state {
    processed: int = 0
  }

  on message(msg: Message) {
    urgency = ai.classify(msg.body, as: Urgency) ?? Urgency.medium

    when urgency {
      low, medium => {
        reply = ai.draft("response to {msg.body}", tone: "friendly")
        if io.confirm(reply) {
          email.send(reply, to: msg.from)
        }
      }
      high, critical => {
        io.notify("{urgency}: {msg.subject}")
        guidance = io.ask("How to respond?")
        reply = ai.draft("response to {msg.body}", guidance: guidance)
        if io.confirm(reply) {
          email.send(reply, to: msg.from)
        }
      }
    }

    self.processed = self.processed + 1
  }

  # Scheduling is a library call, not a keyword.
  # The block registers a recurring event on this agent's mailbox.
  @on_start {
    schedule.every(5.minutes, () => {
      for email in email.fetch(unread: true) {
        # deliver to this agent's message handler
        send(self, email.as_message())
      }
    })
  }
}

run(EmailBot)
```

---

## 4. Agents

The actor model is Keel's one concurrency primitive. Agents are isolated, serial, message-driven coroutines with mutable state accessible only through `self`.

### 4.1 Minimal agent

```keel
agent Greeter {
  @role "You greet people warmly"
}
```

### 4.2 Full agent anatomy

```keel
agent AgentName {
  # --- Attributes (stdlib-defined metadata) ---
  @role "Natural language description"
  @model "smart"                      # LLM binding for ai.* inside this agent
  @tools [email, Calendar]           # whole-namespace capability bindings
  # or with method-level guards:
  # @tools [email.fetch, email.send if self.confirmed, http]
  @memory persistent                 # stdlib memory binding (none | session | persistent)
  @rules [
    "Never reveal internal pricing",
    "Always disclaim medical advice"
  ]
  @limits {
    timeout: 30.seconds       # enforced — wraps task execution in a deadline
    max_tokens: 4096          # enforced — caps tokens sent to the LLM per call
    max_cost: 0.50            # enforced — caps estimated USD cost per call
    # max_cost_per_request and require_confirmation are planned but not yet
    # implemented; using them raises a compile-time error in v0.1.
  }

  # --- State (mutable via self.) ---
  state {
    processed: int = 0
    last_run: datetime? = none
  }

  # --- Agent tasks (methods) ---
  task greet(name: str) -> str {
    ai.draft("greeting for {name}", tone: "warm") ?? "Hello!"
  }

  # --- Event handlers ---
  on message(msg: Message) {
    response = self.greet(msg.from)
    email.send(response, to: msg)
    self.processed = self.processed + 1
  }

  # --- Lifecycle hooks (stdlib attribute) ---
  @on_start {
    schedule.every(1.day, at: @9am, () => {
      io.notify("Good morning — {self.processed} messages processed yesterday")
    })
  }
}
```

### 4.3 Attributes (`@name`)

Attributes are identifier-prefixed metadata clauses inside an agent body. The core language knows only two attributes:

| Attribute | Core-defined? | Semantics |
|---|---|---|
| `@role` | Yes | The agent's identity string, bound to the installed `LlmProvider` for all `ai.*` calls. |
| `@model` | Yes | The model name string, overrides the global default for this agent's `ai.*` calls. |

Everything else (`@tools`, `@memory`, `@rules`, `@limits`, `@on_start`, `@on_stop`, custom attributes) is **stdlib-defined**: libraries register attribute handlers at startup, and the runtime invokes them during agent initialization to wire up capabilities.

**`@tools` — capability gating (deny-by-default)**

`@tools` declares which **effectful** std modules the agent may call. Capabilities are **declared, never implied**: an agent with no `@tools` attribute may not call any effectful module. The developer reads the declaration and knows — there is nothing to guess.

A capability guards authority over the world outside the process; pure computation is never gated. The gated modules are `ai`, `io`, `http`, `email`, `file`, `shell`, `db`, `search`, and `env` (ambient secrets). Everything else — `json`, `csv`, `math`, `random`, `uuid`, `crypto`, `time`, `log`, `cache`, `memory`, `schedule`, `async`, `control`, `testing` — is pure computation or internal control flow and needs only its `use std/<name>` import.

```
@tools all                  # explicit unrestricted form (greppable)
@tools [entries]            # allowlist; each entry is one of:

mod                         # whole module, always allowed
mod.method                  # specific method, always allowed
mod if expr                 # whole module, allowed when expr is true
mod.method if expr          # specific method, allowed when expr is true
```

`expr` is any boolean expression evaluated at the start of each handler turn. `self.*` state, `self.task(...)`, and top-level task calls returning `bool` are valid.

Enforcement is two-layered:

- **Compile time:** a direct std call in the agent body that `@tools` does not cover is a type error naming the fix (`declare \`@tools [io]\` on the agent, or use \`@tools all\``). Conditional entries count as declared.
- **Runtime:** every effectful entry-point call during an agent turn — including calls reached through helper tasks in any module — is checked against the turn's evaluated allowlist. A blocked call raises `CapabilityError` naming the missing module. `@tools` must therefore cover the transitive effectful needs of the helpers an agent calls.

Gating applies to effectful std module entry points only, inside agent turns. Pure-compute modules, value methods (`conn.query(...)`), built-in agent verbs, local module tasks, top-level statements, and `test` blocks are not gated.

```keel
@tools [
  email.fetch,                      # always can read
  email.send if self.confirmed,      # send only after confirmation
  db.query,
  db.exec   if self.admin,
  http,                             # whole module, always
]
```

**Why attributes and not keywords?** A keyword requires a grammar rule and couples the compiler to a specific feature. An attribute is just a name. Adding `@my_custom_attr` requires no language change — only a handler in the library that provides it. The user's file parses identically regardless of which libraries are loaded.

### 4.4 Agent lifecycle

```keel
run(MyAgent)                 # start
run(MyAgent, background: true)  # non-blocking
stop(MyAgent)                # graceful shutdown
```

`run` and `stop` are **built-in agent verbs**, always in scope — like `send`, `delegate`, and `broadcast` (§20.9).

### 4.5 State and thread safety

- Agent `state` fields are mutable **only via `self.`**.
- Event handlers for one agent run **sequentially**. No concurrent access to `state`.
- Different agents run concurrently but share no state.
- Cross-agent data flows through `Agent.delegate`, `Agent.broadcast`, or `memory.*`.

#### Readonly state fields

A state field annotated with `readonly` after its colon is **compiler-enforced read-only**: any `self.field = ...` assignment inside the agent is a compile-time error. The default value is still required (it is the field's initial value) but can never be overwritten by the agent itself.

```keel
agent SessionBot {
  state {
    turns:      int          = 0
    session_id: readonly str = "default-session"
  }

  on message(msg: str) {
    self.turns = self.turns + 1          # ok — writable
    # self.session_id = "x"             # compile error: field is declared readonly
    io.show(self.session_id)             # reading is fine
  }
}
```

Readonly fields are useful for:
- Runtime-provided context (session IDs, request metadata) that the agent must not modify.
- Invariants that should never change once initialized.

The runtime also enforces the restriction: if a readonly assignment somehow bypasses the type checker (e.g. via dynamic dispatch), a runtime error is raised.

### 4.6 Composition over monoliths

Top-level tasks are reusable and testable. Prefer small agents that call top-level tasks over large agents with inline logic.

```keel
task triage(email: EmailInfo) -> Urgency {
  ai.classify(email.body, as: Urgency) ?? Urgency.medium
}

agent EmailAssistant {
  @role "Triage and respond"
  on message(msg: Message) {
    urgency = triage(msg)
    io.show({urgency: urgency, subject: msg.subject})
  }
}
```

---

## 5. Interfaces (Protocols)

Interfaces declare a set of method signatures. A type satisfies an interface **structurally** — if it has all the required methods with compatible signatures, it is an instance.

### 5.1 Declaration

Any user can define a new interface with `interface`:

```keel
interface Printable {
  task print(self) -> str
}

interface Summable {
  task total(self) -> float
}
```

The interface body lists method signatures — `task name(self, ...) -> ReturnType`. The `self` parameter is written without a type annotation; the type is inferred from the `for TypeName` clause of each `impl` block.

Reserved built-in interfaces (`Stringable`, `Comparable`, `Equatable`, `Serializable`, `Iterable`) are declared by the runtime itself; they do not need a user-level `interface` declaration but follow the same `impl` rules.

### 5.2 `impl` blocks

A struct type satisfies an interface by providing an `impl` block:

```keel
type Point {
  x: float
  y: float
}

impl Printable for Point {
  task print(self) -> str {
    "({self.x}, {self.y})"
  }
}

p: Point = { x: 1.5, y: 2.0 }
io.show(p.print())    # → "(1.5, 2.0)"
```

**Rules for `impl` blocks:**

- `impl` and `for` are reserved keywords.
- The named interface must be declared (either user-defined or a built-in like `Stringable`) before any `impl` references it — unless both appear in the same file, in which case order does not matter (the compiler pre-collects all interface declarations).
- Every method listed in the interface must be provided. Missing methods are a compile-time error.
- Extra methods not listed in the interface are a compile-time error.
- Parameter count must match (excluding `self`).
- Parameter types must match the interface signature. `dynamic` in the interface's parameter position is a wildcard — an `impl` may narrow it to any concrete type (this is how `Comparable`/`Equatable` accept `other: dynamic` while the `impl` declares `other: Score`). A concrete interface parameter type requires an exact match.
- Return types must match exactly (`dynamic` in the return position is likewise a wildcard).
- `self` inside the block receives the struct value. Use `self.field` to access fields.
- Method bodies are type-checked like any other task body.

**`self` inside a method body.** `self` is the receiver *value*, of the type named by the `for` clause — not an agent reference. So:

- `self.field` has the field's declared type; naming a field the type does not declare is a compile-time error.
- `self` may be passed anywhere a value of the implementing type is expected.
- `self.field = value` is a compile-time error. The receiver is passed by value, so the write could never outlive the call — return an updated value instead.
- `self.method(...)` is a compile-time error. That form is agent syntax and always dispatches to the enclosing agent's task, never to another `impl` method. Move the logic into a top-level task and call it as `method(self)`.

**Dispatch rule.** The runtime dispatches `impl` methods by the value's declared type tag. A value acquires its tag at the first typed boundary it crosses: a `let x: TypeName = ...` binding, a task parameter with a named type annotation, a task return with a named return type, or an `ai.extract(…, as: TypeName)` call. List elements are promoted to `Value::Struct` when the list is assigned to a `list[TypeName]` variable. Untagged maps (struct literals not yet passed through a typed boundary) do not dispatch to any `impl` method.

### 5.3 Built-in interfaces

Five interfaces are built into the runtime. They cannot be redeclared with `interface`, but any struct type can provide an `impl` block for them.

**`Stringable`** — types that implement `Stringable` can appear inside string interpolation `"{expr}"`:

```keel
interface Stringable {
  task to_str(self) -> str
}
```

All primitives (`int`, `float`, `bool`, `datetime`, `duration`, `Uuid`) implement `Stringable` by default. User-defined types opt in via an explicit `impl` block:

```keel
impl Stringable for Point {
  task to_str(self) -> str { "({self.x}, {self.y})" }
}

p: Point = { x: 3, y: 4 }
io.show("origin is {p}")    # → "origin is (3, 4)"
```

**`Comparable`** — enables sorting and comparison for user-defined types:

```keel
interface Comparable {
  task compare(self, other: dynamic) -> int
}
```

`compare(self, other)` returns negative/zero/positive. Wired into `list.sort()`, `list.min()`, `list.max()`, and the global `min()`/`max()`.

**`Equatable`** — typed equality check:

```keel
interface Equatable {
  task equals(self, other: dynamic) -> bool
}
```

Method-only. `==` remains structural comparison.

**`Serializable`** — override `json.stringify`:

```keel
interface Serializable {
  task to_json(self) -> str
}
```

When a type implements `Serializable`, `json.stringify(value)` calls `to_json()` instead of the default serialiser.

**`Iterable`** — use a struct in a `for` loop:

```keel
interface Iterable {
  task items(self) -> list[dynamic]
}
```

The concrete return type may be `list[T]` for any `T` — `list[dynamic]` is a wildcard in the conformance check. `items()` materialises the full list before iteration begins (not a generator).

```keel
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
for n in Range { lo: 1, hi: 3 } { io.show("{n}") }   # 1, 2, 3
```

### 5.4 Why interfaces are core

- `ai.classify` needs to dispatch to *some* LLM implementation. Hard-coding a single provider into the runtime locks users out of self-hosted, proprietary, or novel backends.
- `Memory` in v0.1 is a plain K/V store (JSON file); in v0.2 it will dispatch through a `VectorStore` interface so users can swap backends.
- `log.info` needs a sink — users want OTel, Datadog, or plain stdout.

The language can't know about every provider. Interfaces let stdlib declare the *protocol*, ship a default implementation, and, once the runtime registry is wired, let users swap implementations.

### 5.5 Selecting an LLM provider

`ai.*` dispatches through swappable backends. Three are built in — `ollama`
(default, local), `openai`, and `anthropic` (Claude) — and a program selects
between them with **zero extra code**, three ways, most-specific wins:

```keel
# Per-call: a `provider:` prefix on the model tag.
agent Assistant {
  @model "anthropic:claude-opus-4-8"   # routes this agent's ai.* calls to Claude
}

agent Helper {
  @provider openai                     # per-agent default backend…
  @model "gpt-4o"                       # …for bare (unprefixed) tags
}
```

- **Per-call** — a model tag of the form `"<provider>:<model>"`
  (`"openai:gpt-4o"`, `"anthropic:claude-opus-4-8"`, `"ollama:llama3"`), set via
  `@model` or a `using:` argument.
- **Per-agent** — `@provider <name>` (`ollama` | `openai` | `anthropic`) sets the
  backend for that agent's bare tags. An unknown name is a compile-time error.
- **Per-program** — the `KEEL_PROVIDER` environment variable sets the default
  backend (otherwise `ollama`).

OpenAI and Anthropic read their keys from `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`;
a missing key surfaces as `AiError { reason: "provider" }`, never a silent
fallback. `@limits { max_tokens }` caps generation for the backend.

#### User-authored providers

For the long tail of proprietary or self-hosted backends, write a provider **in
Keel** and install it. Any field-less type with `impl LlmProvider` becomes a
backend `ai.*` can dispatch through:

```keel
type MyProvider {}

impl LlmProvider for MyProvider {
  task complete(self, req: CompletionRequest) -> str {
    # `req` carries `system`, `user`, `model`, and `max_tokens`. Configuration
    # (endpoints, keys) is read from env.* — the provider is constructed with
    # no fields, so it holds no state of its own.
    key = env.get("MY_LLM_KEY")!
    http.post("https://my-llm.example/complete", body: { prompt: req.user })["text"]
  }
}

ai.install(MyProvider)        # program-wide default (lowest precedence)

agent Assistant {
  @provider MyProvider        # …or per-agent, like the built-in names
}
```

`complete(self, req: CompletionRequest) -> str` returns the raw model output;
Keel's prompt construction and output parsing (enum matching, schema validation)
are applied identically to built-in and user providers, so `??`, `when`, and the
typed `AiError`/`AiSchemaError` errors behave the same. `CompletionRequest` is a
built-in struct with fields `system: str`, `user: str`, `model: str`, and
`max_tokens: int`.

`ai.install(X)` and `@provider X` require `X` to be a type implementing
`LlmProvider`; anything else is a compile-time error. A user provider must not
call `ai.*` from inside its own `complete()` — that re-entry is rejected with an
`AiError` rather than recursing without bound.

---

## 6. Tasks

Tasks are named, reusable operations. They are Keel's functions.

### 6.1 Basic task

```keel
task greet(name: str) -> str {
  "Hello, {name}!"
}
```

The last expression is the implicit return. Explicit `return` is supported for early exit.

### 6.2 Task with AI operations

```keel
task triage(email: {body: str}) -> Urgency {
  ai.classify(email.body, as: Urgency) ?? Urgency.medium
}
```

### 6.3 Pipelines

The `|>` operator passes the left value as the first argument to the right function.

```keel
email |> triage |> respond |> log
# equivalent to:
log(respond(triage(email)))
```

With extra arguments:

```keel
email |> triage |> respond(tone: "friendly")
```

### 6.4 Signatures

```keel
task cleanup() { ... }                            # no params, returns none
task greet(name: str) { "Hello, {name}!" }        # inferred return
task triage(e: EmailInfo) -> Urgency { ... }      # annotated
task compose(e: EmailInfo, tone: str = "pro") {...}  # default params
task quick(d: {body: str}) -> str { d.body }      # structural param
```

### 6.5 Generic tasks

Tasks can be parameterised over type variables. Type arguments are **inferred at call sites** from the concrete argument types — you never write `swap[str, int](p)` unless inference fails.

```keel
task swap[A, B](p: Pair[A, B]) -> Pair[B, A] {
  { first: p.second, second: p.first }
}

task identity[T](x: T) -> T { x }

task map_items[T, U](items: list[T], f: (T) -> U) -> list[U] {
  items.map(f)
}
```

Type parameters are scoped to the task body and are substituted by the type checker when the concrete call-site types are known.

Generic tasks are allowed inside agent bodies under the same rules as non-generic tasks.

### 6.6 Variadic parameters

A task may declare a variadic positional parameter by prefixing it with `...`. The variadic param collects all positional call-site arguments into a `list[T]` inside the body. It must be the last parameter in the declaration.

```keel
task greet(...names: str) -> str {
  names.map(n => "Hello, {n}!").join(", ")
}

greet("Alice", "Bob")              # names = ["Alice", "Bob"]
greet()                            # names = []
```

**Spread at call sites:** prefix any `list[T]` or `set[T]` with `...` to expand it into individual variadic slots:

```keel
more = ["Dave", "Eve"]
greet("Alice", ...more)            # names = ["Alice", "Dave", "Eve"]
greet(...names, ...other_names)    # merge two lists
```

**Named args after variadics:** the `identifier:` suffix unambiguously terminates the positional section:

```keel
min(a, b, c, by: x => x.score)
min(...scores, 99, by: x => x)
```

**Type checking:** all variadic args must be the same type `T`; `...expr` requires `expr: list[T]` or `set[T]`.

Note: `min(scores)` where `scores: list[T]` is a **single-argument shorthand** — a lone list argument is auto-spread, so `min(scores)` and `min(...scores)` are equivalent. This matches `min(people, by: p => p.age)` where `people` is a list.

### 6.7 Agent-local task calls

Tasks declared inside an agent are methods of the current agent. Invoke them
with `self.task(...)`:

```keel
agent Mailbox {
  task summarize(subject: str) -> str {
    "Subject: {subject}"
  }

  on message(msg: Message) {
    io.show(self.summarize(msg.subject))
  }
}
```

Inside an agent, an unqualified `task(...)` call resolves only through lexical
and top-level scope. Agent-local tasks are not injected into bare-name lookup.
`MyAgent.task(...)` is not a cross-agent call form; use `Agent.send`,
`Agent.delegate`, or `Agent.broadcast` for mailbox-based coordination.

### 6.8 `Agent.delegate` — type-safe handler dispatch

`Agent.delegate` posts a named event to a target agent's mailbox. Two forms are supported:

**Symbol form (preferred):** `delegate(TargetAgent.handlerName, data)`

The handler reference `TargetAgent.handlerName` is resolved at compile time. The type
checker verifies:
1. `TargetAgent` is a declared agent.
2. `handlerName` is a declared `on` handler on that agent.
3. `data` matches the handler's declared parameter type (when the parameter is typed).

```keel
agent Worker {
  on process(task: Task) {
    log.info("processing {task.id}")
  }
}

agent Boss {
  @on_start {
    run(Worker)
    delegate(Worker.process, my_task)   # ✓ type-checked at compile time
    delegate(Worker.typo, my_task)      # ✗ compile error: no handler `typo`
  }
}
```

**String form (legacy):** `delegate(TargetAgent, "handlerName", data)`

The handler name is a string literal. The type checker validates it when the string
is a plain literal (no interpolation). Handler renames do not update string literals
automatically — prefer the symbol form for all new code.

```keel
delegate(Worker, "process", my_task)   # checked when literal is plain
```

Both forms enqueue the event on the target agent's mailbox and return immediately;
the handler runs later in the target's serialized context.

---

## 7. Lambdas and first-class functions

```keel
# Single-param shorthand
triaged = emails.map(e => triage(e))

# Multi-param
pairs = left.zip_with(right, (a, b) => a + b)

# Block body
scored = emails.map(e => {
  urgency = triage(e)
  {email: e, urgency: urgency}
})

# Named function as a value
results = emails.map(triage)
```

### Lambdas are non-capturing

A lambda body sees only its own parameters, `self` (inside an agent context —
ambient per-call state, not lexical capture), and global names (tasks, agents,
imported symbols). It cannot read a local variable declared in an enclosing
scope, including an enclosing lambda's parameters:

```keel
n = 10
add_n = x => x + n        # check error: `n` is declared outside this lambda

outer = x => (y => x + y) # check error: inner lambda can't see `x` either

double = n => n * 2       # fine — `n` here is the lambda's own parameter
```

To use an outer value inside a lambda, pass it in explicitly — as a parameter,
or by having the caller supply it as an argument to the function the lambda is
passed to. Lambdas compile to plain function pointers; there is no captured
environment to allocate or reference-count.

### Function types

```keel
type Handler = (Message) -> str
type Predicate[T] = (T) -> bool

task process_all(emails: list[EmailInfo], handler: (EmailInfo) -> str) {
  emails.map(handler)
}
```

---

## 8. Control Flow

### 8.1 `if` / `else` (expression)

```keel
# As statement (else optional)
if urgency == Urgency.high { escalate(email) }

# As expression (else REQUIRED, branches must produce compatible types)
reply = if guidance != none {
  ai.draft("response", guidance: guidance)
} else {
  ai.draft("response", tone: "friendly")
} ?? "(draft failed)"
```

An `if` without `else` used as an expression is a compile error.

More generally, **every branch of an `if` or `when` used where a value is expected must produce a value.** A branch produces a value when it ends in an expression, or in a nested `if`/`when`/`try` whose own branches all do. A branch that ends in anything else — an empty block, a `let`, a loop — is a compile error, because it would evaluate to `none` regardless of the type inferred for the expression as a whole. The `??` operator does not exempt an `if` from this rule.

A branch that exits via `return` or `raise` is exempt: it never falls through to produce a value, so the remaining branches determine the expression's type.

```keel
score: int = if ready { compute() } else { return 0 }   # OK — else diverges
label = if ready { "go" }                               # Error — no else
label = if ready { "go" } else { }                      # Error — else produces no value
tally = when n { 0 => { }  _ => 1 }                     # Error — arm produces no value
```

### 8.2 `when` (pattern matching)

`when` works as both a **statement** and an **expression**.

**Statement form** — branches execute side effects:

```keel
when urgency {
  low, medium => auto_reply(email)
  high        => flag_and_draft(email)
  critical    => escalate(email)
}
```

**Expression form** — evaluates to the matched arm's value:

```keel
label = when urgency {
  low    => "low priority"
  medium => "medium priority"
  high   => "high priority"
}
```

All arms must produce the same type. The expression form is valid anywhere an expression is expected (assignment RHS, argument, return value).

**Rich variant matching:**

```keel
when action {
  reply { to, tone }   => email.send(ai.draft("reply", tone: tone), to: to)
  forward { to }       => email.send(email, to: to)
  archive              => email.archive(email)
  escalate { reason, urgency }
    where urgency == Urgency.critical => page_oncall(reason)
  escalate { reason, _ } => io.notify("Escalation: {reason}")
}
```

**Struct patterns** — bind named fields from a struct value into the arm scope:

```keel
when signal {
  { price, volume } where price > 1000.0 and volume > 0.0 => "active"
  { price }         where price > 1000.0                  => "thin"
  _                                                        => "quiet"
}
```

- `{ field1, field2 }` binds those fields; they are available in the `where` guard and arm body.
- Use `_` as a field name to skip a field without binding it.
- The subject **must be a struct type**, and every named field **must exist** on it. A struct pattern against a non-struct subject, or naming a field the struct does not declare, is a compile-time type error.
- An **unguarded** struct arm is total only when the subject is a **non-nullable** struct (matches any value of that struct) — no separate `_` arm is required. Against a nullable struct (`Signal?`) the `none` case is still uncovered, so a `_` or `none` arm is required.
- A **guarded** struct arm is not total; add a `_` fallback or another unguarded arm to satisfy exhaustiveness.

**Non-enum matching (primitives, strings):** wildcard `_` is **required** (the compiler can't prove exhaustiveness on unbounded types).

**Exhaustiveness:** All enum variants must be covered (or `_` present) in both statement and expression forms. For non-nullable struct subjects, an unguarded `{ ... }` arm satisfies exhaustiveness. A struct arm never satisfies exhaustiveness for an enum or other non-struct subject — it cannot match those values.

### 8.3 `for` loops

```keel
for email in emails { process(email) }
for email in emails if email.unread { triage(email) }
```

### 8.4 Destructuring

```keel
{urgency, category} = result                # struct
{urgency: u, category: c} = result          # rename
(urgency, summary) = triage_full(email)     # tuple

for {from, subject} in emails { ... }       # in for
task handle({body, from}: EmailInfo) { ... }  # in params
```

### 8.5 `try` / `catch`

Catches by **variant matching**. `Error` is the catch-all type; every stdlib error is a subtype of it. Use a specific type name to handle a particular failure domain:

```keel
try {
  data = file.read("config.json")
} catch e: FileError {
  data = "{}"                   # handle missing file specifically
} catch e: Error {
  io.show("unexpected: {e.message}")
}

try {
  resp = http.get("https://api.example.com/data")
} catch e: HttpError {
  io.show("network error: {e.message}")
}

try {
  rows = csv.parse_records(raw)
} catch e: CsvError {
  io.show("bad CSV: {e.message}")
}

try {
  control.with_timeout(5.seconds, () => { slow_operation() })
} catch e: TimeoutError {
  io.show("timed out: {e.message}")
}
```

All stdlib error types and their fields are listed in the error type registry (§2.10).

### 8.6 `raise`

Throws an error from a value. Symmetric with `try`/`catch` — you can throw as well as catch.

```keel
raise "validation failed"

# raise produces UserRaised; caught by any UserRaised or Error clause
try {
  raise "quota exceeded"
} catch err: UserRaised {
  io.notify("User raised: {err.message}")
} catch err: Error {
  io.notify("Other error: {err.message}")
}
```

`raise` accepts any expression. If the value is a string it is used as the error message directly. Otherwise the value's display representation becomes the message. `raise` produces a `UserRaised` error, which is also caught by `catch err: Error`.

### 8.7 Augmented assignment (`+=`, `-=`, `*=`, `/=`)

Shorthand for `x = x op expr`. Works for local bindings and agent state fields.

```keel
count = 0
count += 1          # same as: count = count + 1

self.total += price # same as: self.total = self.total + price
retries -= 1
scale *= 2.0
ratio /= 100.0
```

Type annotations are not permitted on augmented assignments — use plain `x: T = expr` for the initial declaration.

### 8.8 `break` and `continue`

Exit or skip within the nearest enclosing `for` or `while` loop.

```keel
# break — exit the loop immediately
for item in items {
    if item == target {
        break
    }
    process(item)
}

# continue — skip to the next iteration
for n in 1..100 {
    if n % 2 == 0 {
        continue
    }
    process_odd(n)
}

# together — skip even, stop at 50
for n in 1..100 {
    if n % 2 == 0 { continue }
    if n > 50      { break }
    process_odd(n)
}
```

Both statements affect only the **nearest enclosing loop** — there are no labeled jumps in v0.1.

`break` and `continue` are reserved keywords; using either outside a loop is a runtime error.

### 8.9 `while` loops

Unbounded iteration — repeat the body as long as the condition is `true`.

```keel
# Basic countdown
n = 5
while n > 0 {
    io.show("tick: {n}")
    n -= 1
}

# Accumulate with break
total = 0
i = 1
while true {
    total += i
    i += 1
    if total > 10 {
        break
    }
}

# Skip even numbers with continue
x = 0
while x < 10 {
    x += 1
    if x % 2 == 0 { continue }
    process_odd(x)
}
```

The condition must be `bool`. `break` and `continue` work identically to their `for`-loop counterparts.

---

## 9. Concurrency

### 9.1 Core primitives

The core runtime exposes exactly three concurrency primitives, surfaced via `Async`:

| Primitive | Type | Behavior |
|---|---|---|
| `async.spawn(fn)` | `() -> T` returning `Task[T]` | Start a child task. Parent-cancels-children semantics. |
| `Task[T].await()` | `T` | Block the current handler until the task completes. |
| `Task[T].cancel()` | `none` | Cancel the task. |

Everything else is a library combinator:

```keel
async.join_all(tasks: list[Task[T]]) -> list[T]   # all-or-nothing; cancels siblings on error
async.select(tasks: list[Task[T]]) -> T            # first to complete wins
async.sleep(d: duration) -> none
```

### 9.2 Structured concurrency

Cancellation is structured: when a parent task cancels or errors, all spawned children cancel. This is the one contract the runtime upholds.

### 9.3 No `parallel` / `race` keywords

Concurrent composition is expressed through library functions, not grammar:

```keel
[urgency, sentiment] = async.join_all([
  async.spawn(() => ai.classify(body, as: Urgency) ?? Urgency.medium),
  async.spawn(() => ai.classify(body, as: Sentiment) ?? Sentiment.neutral)
])
```

Trade-off: a dedicated `parallel { ... }` block would read slightly nicer. The library form is more honest about what's happening and extensible — users can write `join_first_n`, `join_settled`, etc. without a language change.

### 9.4 Agent event queue

```
  event ──>   ┌──────────────────┐
  event ──>   │  mailbox         │ ──> handler (sequential)
  timer ──>   │  (per agent)     │
              └──────────────────┘
```

Events land in the agent's mailbox. The runtime processes them one at a time. A handler that calls `io.ask`, `async.sleep`, or `Agent.delegate` suspends — other *agents* continue. Other events for the *same* agent queue behind.

---

### 9.5 Test blocks

Top-level test blocks are executed by `keel test`:

```keel
use std/testing

type Severity = low | medium | critical

task classify(text: str) -> Severity {
  ai.classify(text, as: Severity) ?? Severity.low
}

test "mocked classify returns critical" {
  testing.mock(ai.classify).returns(Severity.critical)
  assert classify("payment outage") == Severity.critical
}
```

`keel test file.keel` type-checks the file, registers declarations, and executes each `test "name" { ... }` block. Parameterized tests use `test "name" for case in cases { ... }`, where `cases` must evaluate to a list; each list item runs as a separate test case named `name [index]` with `case` bound for `setup` and the test body. `keel test dir/` recursively discovers `.keel` files with test blocks and skips files without tests. `keel test file.keel --filter text` executes only tests whose names contain `text`; if no tests match, the command fails. `keel test file.keel --list` lists test names without running them, and can be combined with `--filter`. `--fail-fast` stops after the first failing test. `--quiet` suppresses passing result lines while still printing failures and the final summary. Failed tests print the source location of the failing statement when available. If an unfiltered file or directory has no tests, the command prints `0 tests found` and exits successfully. Top-level statements such as `run(A)` are not executed by `keel test`. Normal `keel run` ignores test blocks.

Inside a test block:

- `use std/testing` brings the `testing` namespace into scope for test helpers.
- `testing.mock(module.method).returns(value)` overrides one std module method for the current test only.
- Repeating the same mock target returns values in order; after the sequence is exhausted, the final value repeats.
- Mocked methods expose test-local metadata: `Namespace.method.called: bool`, `Namespace.method.call_count: int`, and `Namespace.method.called_with(...): bool`.
- `setup { ... }` runs before the assertion/body statements in the same test and can bind values used by the body.
- `assert expr` requires `expr: bool`; `false` fails the current test. `assert expr, message` uses a custom `str` failure message.
- Each test gets its own mock set, so mocks do not leak between tests.
- Capability checks still apply. A mock replaces the method result; it does not grant an agent access to a namespace disallowed by `@tools`.

`test`, `setup`, and `assert` are contextual syntax words, not reserved keywords. They remain legal identifiers outside the positions above.

---

## 10. Reserved Keywords

This is the complete set. If a word is not on this list, it is an identifier.

```
agent task interface impl type extern
use from
state on self
if else when where
for while in break continue
try catch return raise
as and or not
true false none
set
```

That's it.

Module bindings (`ai`, `io`, `http`, `schedule`, `async`, …) are identifiers bound by `use std/<name>`, not keywords (§20). Same for `run`, `stop`, `send`, `delegate`, `broadcast` — built-in agent verbs — and `min`, `max`, `typeof`.

Attribute names (`@role`, `@model`, `@tools`, …) are identifiers. Only the `@` prefix is syntax.

Testing words (`test`, `setup`, `assert`) are contextual syntax words, not reserved keywords.

Duration units (`seconds`, `minutes`, `hours`, `days`, `weeks`) are **identifiers recognized by the lexer in the `INT "."` position**, not reserved words.

---

## 11. Error Handling

### 11.1 Error types

`Error` is the catch-all type. All error values carry `message: str` implicitly. Catch clauses match by type name.

The model — **absence is a value, failure is an error**:

| Condition | Result | Handle with |
|---|---|---|
| Model returned no answer / no model configured / mock mode | Returns `none` | `??` or `when` |
| Provider unreachable, network failure | Throws `AiError` (`reason: "unavailable"`) | `try/catch` |
| Model not mapped / provider misconfiguration | Throws `AiError` (`reason: "provider"`) | `try/catch` |
| LLM call exceeded its time budget | Throws `TimeoutError` | `try/catch` (set via `@limits timeout` / `control.with_timeout`) |
| LLM output didn't match the expected schema | Throws `AiSchemaError` | `try/catch` |
| Event queue full (`send` / `delegate` / `broadcast`) | Throws `RuntimeBusy` | `try/catch` |

A real provider failure **throws** rather than returning `none`, so an outage is never silently masked by a `??` default — `ai.classify(...) ?? Urgency.medium` yields `medium` only when the model genuinely had no answer, not when the model was down.

`AiError` carries `message: str` and `reason: str` — `"unavailable"` (network / provider unreachable) or `"provider"` (model not mapped / configuration fault). It is caught by `catch err: AiError` or the catch-all `catch err: Error`.

`AiSchemaError` carries `message: str` and `got: str` (the raw LLM output that failed to match). It is caught by `catch err: AiSchemaError` or the catch-all `catch err: Error`.

`RuntimeBusy` carries `message: str`. Thrown by `Agent.send`, `Agent.delegate`, and `Agent.broadcast` when the interpreter event queue is at capacity. The queue depth defaults to 1024 and is configurable via `KEEL_EVENT_QUEUE_CAPACITY`. HTTP requests that arrive when the queue is full receive a `503` response automatically — no `RuntimeBusy` is thrown to user code in that path. `Agent.broadcast` fails on the first `RuntimeBusy` and leaves remaining recipients undelivered.

### 11.2 Nullable-aware stdlib

`ai.*` calls return `T?` for genuine **absence** — the model returned no answer, no model is configured, or mock mode is active. A provider **failure** (network, unreachable, misconfiguration) instead *throws* `AiError`, so a `??` default never silently hides an outage. Use `??` or `when` for the absence case:

```keel
# Simple default via ??
summary = ai.summarize(article, in: 3, unit: sentences) ?? "No summary available"
urgency = ai.classify(text, as: Urgency) ?? Urgency.medium

# Explicit when
when ai.classify(text, as: Urgency) {
  some(u)  => handle(u)
  none     => io.notify("Could not classify")
}
```

When you need to distinguish *why* a call failed, use `try/catch`:

```keel
try {
  urgency = ai.classify(email.body, as: Urgency) ?? Urgency.medium
} catch err: AiSchemaError {
  io.notify("Unexpected LLM output: {err.got}")
  urgency = Urgency.medium
} catch err: AiError {
  control.retry(3, () => {
    urgency = ai.classify(email.body, as: Urgency) ?? Urgency.medium
  })
} catch err: Error {
  io.notify("Unexpected failure: {err.message}")
}
```

### 11.3 Retry

`control.retry` is a stdlib function:

```keel
control.retry(3, backoff: exponential, () => {
  email.send(reply, to: addr)
})

control.retry(5, delay: 10.seconds, () => {
  http.get("https://api.example.com/data")
})
```

### 11.4 Limits and rules

Both are stdlib-defined attributes (`@limits`, `@rules`):

- **`@limits`** are **deterministic** constraints enforced by the runtime: cost per request, token caps, timeouts, required-confirmation action lists. Violations are rejected.
- **`@rules`** are **natural-language instructions** the stdlib injects into LLM prompts. LLM compliance is best-effort, not guaranteed.

The separation is intentional: limits are verifiable, rules are aspirational. Mixing them would hide the difference.

---

## 12. Memory

`Memory` is a per-agent key-value store. Agents opt in with `@memory persistent` (survives restarts), `@memory session` (in-process, default), or `@memory none` (disables `memory.*` entirely).

```keel
agent Counter {
  @memory persistent

  @on_start {
    count = memory.recall("visits")
    next = if count == none { 1 } else { count + 1 }
    memory.remember("visits", next)
    io.show("Visit {next}")
    stop(self)
  }
}
```

### Operations

| Call | Returns | Notes |
|---|---|---|
| `memory.remember(key, value)` | `none` | Store any Keel value under `key`, scoped to this agent |
| `memory.recall(key)` | `Value?` | Return stored value or `none` if absent |
| `memory.forget(key)` | `none` | Delete the key |

### Scope and isolation

Keys are namespaced per `(program, agent)` pair — two programs that happen to share an agent name (`Counter`) each get their own memory bucket. Two agents within the same program with different names also get separate buckets.

`memory.*` is only valid inside an agent body. Calling it from a top-level statement or a plain `task` raises a runtime error.

### Persistence mode

| Attribute | Behaviour |
|---|---|
| `@memory session` | In-process HashMap; cleared at process exit (default when attribute is omitted) |
| `@memory persistent` | JSON file at `~/.keel/memory/<stem>_<hash12>/<agent>.json`; survives restarts |
| `@memory none` | Any `memory.*` call raises `MemoryError` |

#### Persistent storage path

The directory name is `<stem>_<hash12>` where `<stem>` is the sanitized basename of the source file and `<hash12>` is the first 12 hex characters of the SHA-256 hash of the canonicalized file path. This ensures two programs with the same filename in different directories never share storage.

Special sources that have no stable on-disk path use fixed namespace names:

| Source | Namespace |
|---|---|
| File (e.g. `counter.keel`) | `counter_<hash12>` |
| REPL | `__repl__` |
| stdin / inline | `__stdin__` / `__inline__` |

#### Multi-process safety

Each `memory.*` operation acquires an advisory `flock` on a sidecar `<agent>.lock` file (exclusive for writes, shared for reads). Concurrent `keel run` processes against the same program/agent are safe — writes are serialized by the kernel lock. The lock target is a stable sidecar file that is never renamed.

### v0.2 note: semantic search

v0.1 `Memory` is a plain K/V store. The planned v0.2 upgrade adds a `VectorStore` interface (see §5.1) that backs `recall` with nearest-neighbour embedding search. The v0.1 API surface is a strict subset — existing programs will keep working when the backend is upgraded.

---

## 13. Time

The `Time` namespace provides datetime construction and parsing. Datetimes are RFC 3339 strings with an explicit timezone offset — naive strings (no offset) are rejected. Methods `parts()` and `format()` live on the datetime value itself.

```keel
now      = time.now()                          # UTC, millisecond precision
ny       = time.now(tz: "America/New_York")    # offset-shifted RFC 3339
parsed   = time.parse("2026-05-01T09:00:00Z") # datetime? — none if bad/no TZ
coerced  = time.parse("2026-05-01", tz: "UTC") # naive + tz: → datetime?
ts       = time.epoch_ms()                     # int — ms since Unix epoch

p = parsed.parts()   # {year, month, day, hour, minute, second, millisecond, tz}
s = parsed.format(as: "%Y-%m-%d")  # str? — none if receiver is not a datetime

elapsed  = finish - start   # datetime - datetime → duration
deadline = time.now() + 3.days
ago      = time.now() - 1.hour
```

### Factories (namespace)

| Call | Returns | Notes |
|---|---|---|
| `time.now()` | `datetime` | Current UTC time, millisecond-precision RFC 3339 |
| `time.now(tz: name)` | `datetime` | Offset-shifted; IANA name e.g. `"America/New_York"` |
| `time.parse(str)` | `datetime?` | Accepts RFC 3339 with explicit TZ offset; returns `none` on failure |
| `time.parse(str, tz: name)` | `datetime?` | Coerces a naive string into the given timezone |
| `time.epoch_ms()` | `int` | Unix timestamp in milliseconds (suitable for JS interop, BIGINT columns, signed payloads) |

### Methods (on value)

| Call | Returns | Notes |
|---|---|---|
| `dt.parts()` | map | `{year, month, day, hour, minute, second, millisecond, tz}` |
| `dt.format(as: pattern)` | `str?` | strftime-style (e.g. `"%Y-%m-%d"`, `"%H:%M"`); `none` if not a datetime |

### Duration literals

| Literal | Aliases |
|---|---|
| `500.ms` | `millis`, `millisecond`, `milliseconds` |
| `5.seconds` | `second`, `sec`, `s` |
| `2.minutes` | `minute`, `min`, `m` |
| `1.hour` | `hours`, `hr`, `h` |
| `3.days` | `day`, `d` |
| `1.week` | `weeks`, `w` |

### Arithmetic and comparison

```keel
# datetime ± duration → datetime
deadline = time.now() + 7.days
ago      = time.now() - 30.minutes

# datetime - datetime → duration
elapsed  = finish - start

# comparison
if deadline > time.now() {
  io.show("still time left")
}
```

---

## 14. Math

The `Math` namespace provides transcendental and power functions. All functions accept `int` or `float` arguments and always return `float`. The value-level methods `.abs()`, `.floor()`, `.ceil()`, `.round()` remain on the value itself (e.g. `(-3).abs()`) and are not duplicated here.

```keel
h   = math.sqrt(math.pow(3, 2) + math.pow(4, 2))  # 5.0  (Pythagoras)
ln2 = math.log(2)                                   # ≈ 0.693
deg = 45.0
rad = deg * math.PI() / 180.0
s   = math.sin(rad)                                 # ≈ 0.707
```

### Constants

| Call | Returns | Value |
|---|---|---|
| `math.PI()` | `float` | π ≈ 3.14159265358979 |
| `math.E()` | `float` | e ≈ 2.71828182845905 |

### Functions

| Call | Returns | Notes |
|---|---|---|
| `math.sqrt(x)` | `float` | Square root; raises if `x < 0` |
| `math.pow(x, y)` | `float` | `x` raised to the power `y` |
| `math.exp(x)` | `float` | e^x |
| `math.log(x)` | `float` | Natural logarithm (ln); raises if `x ≤ 0` |
| `math.log2(x)` | `float` | Base-2 logarithm; raises if `x ≤ 0` |
| `math.log10(x)` | `float` | Base-10 logarithm; raises if `x ≤ 0` |
| `math.sin(x)` | `float` | Sine (radians) |
| `math.cos(x)` | `float` | Cosine (radians) |
| `math.tan(x)` | `float` | Tangent (radians) |
| `math.asin(x)` | `float` | Arc-sine (radians); raises if `x ∉ [-1, 1]` |
| `math.acos(x)` | `float` | Arc-cosine (radians); raises if `x ∉ [-1, 1]` |
| `math.atan(x)` | `float` | Arc-tangent (radians) |
| `math.atan2(y, x)` | `float` | `atan(y/x)` with correct quadrant; two positional args |

---

## 15. Csv

The `Csv` namespace parses and produces RFC 4180–compliant CSV text. It is always available — no `@tools` annotation required.

```keel
raw = "symbol,price,volume\nBTC,67000,1234.5\nETH,3500,5678.9"

# Raw parse — list[list[str]], first row is whatever the input contains
rows = csv.parse(raw)         # [["symbol","price","volume"], ["BTC","67000","1234.5"], …]

# Header parse — list[map[str, str]], first row becomes map keys
trades = csv.parse_records(raw)   # [{symbol: "BTC", price: "67000", …}, …]
for trade in trades {
    log.info("{trade["symbol"]} @ {trade["price"] as float:.2f}")
}

# Stringify — list[list[str]] → CSV string (include a header row as the first inner list)
out = [["symbol", "price"], ["BTC", "67000"], ["ETH", "3500"]]
text = csv.stringify(out)
```

### Functions

| Call | Returns | Notes |
|---|---|---|
| `csv.parse(text: str)` | `list[list[str]]` | Parse CSV; every cell is a `str`. Raises `CsvError` on malformed input. |
| `csv.parse_records(text: str)` | `list[map[str, str]]` | First row becomes header keys; remaining rows become maps. Returns `[]` when only a header row is present. |
| `csv.stringify(rows: list[list[str]])` | `str` | Convert rows to CSV text. Each inner list is one row; every cell must be a `str`. Cells containing commas, quotes, or newlines are automatically quoted per RFC 4180. Raises `CsvError` if a row element is not a list or a cell is not a `str`. |

### Notes

- `csv.stringify` only accepts `list[list[str]]`. To convert `list[map[str, str]]` to CSV, project the fields you want into lists first:
  ```keel
  lines = trades.map(t => [t["symbol"], t["price"]])
  text  = csv.stringify([["symbol", "price"]] + lines)
  ```
- Empty input to `csv.parse` returns `[]`.
- `csv.parse_records` with only a header row (no data rows) returns `[]`.

---

## 16. Random, Uuid, and Crypto

### 15.1 `Random` — pseudo-random generation

`Random` produces non-cryptographic pseudo-random values. Use it for simulation, sampling, games, and any context where security is not a concern.

| Call | Returns | Notes |
|---|---|---|
| `random.float()` | `float` | Uniform in `[0.0, 1.0)` |
| `random.int(min:, max:)` | `int` | Inclusive range |
| `random.bool()` | `bool` | 50/50 |

```keel
random.float()              # 0.7341...
random.int(min: 1, max: 6)  # dice roll
random.bool()               # true or false
```

### 15.2 `Uuid` — UUID generation

`Uuid` is a distinct type (not `str`). It implements `Stringable` so it interpolates cleanly.

All constructors require `use std/uuid`; the `Uuid` type itself is built in.

| Call | Returns | Notes |
|---|---|---|
| `uuid.v4()` | `Uuid` | Random (CSPRNG) |
| `uuid.v7()` | `Uuid` | Time-ordered — monotonically increasing, B-tree friendly |
| `uuid.v5(ns:, name:)` | `Uuid` | Deterministic — SHA-1 of namespace + name |
| `uuid.parse(s)` | `Uuid?` | `none` if invalid format |

**Namespace constants:** `uuid.DNS`, `uuid.URL`, `uuid.OID`, `uuid.X500` — for use with `uuid.v5`.

**Value methods:**

| Method | Returns | Notes |
|---|---|---|
| `.version()` | `int` | 4, 7, or 5 |
| `.to_str()` | `str` | Hyphenated lowercase |
| `.format(as:)` | `str` | `"hyphenated"` (default), `"simple"` (no hyphens), `"urn"` |

```keel
id = uuid.v4()                                        # Uuid v4
log.info("created {id}")                           # interpolates via Stringable
uuid.v7()                                          # time-ordered
uuid.v5(ns: uuid.DNS, name: "keel-lang.dev")       # deterministic
uuid.parse("f47ac10b-58cc-4372-a567-0e02b2c3d479") # Uuid?
id.format(as: "simple")                            # "f47ac10b58cc4372a5670e02b2c3d479"
```

### 15.3 `Crypto` — cryptographic primitives

`Crypto` provides security-grade operations backed by a CSPRNG. It is **distinct from `Random`** — use `Crypto` wherever the output affects security (tokens, signatures, key derivation).

| Call | Returns | Notes |
|---|---|---|
| `crypto.sha224(data)` | `str` | SHA-224 hex digest |
| `crypto.sha256(data)` | `str` | SHA-256 hex digest |
| `crypto.sha384(data)` | `str` | SHA-384 hex digest |
| `crypto.sha512(data)` | `str` | SHA-512 hex digest |
| `crypto.sha512_224(data)` | `str` | SHA-512/224 hex digest |
| `crypto.sha512_256(data)` | `str` | SHA-512/256 hex digest |
| `crypto.hmac_sha224(key, data)` | `str` | HMAC-SHA-224 hex signature |
| `crypto.hmac_sha256(key, data)` | `str` | HMAC-SHA-256 hex signature |
| `crypto.hmac_sha384(key, data)` | `str` | HMAC-SHA-384 hex signature |
| `crypto.hmac_sha512(key, data)` | `str` | HMAC-SHA-512 hex signature |
| `crypto.hmac_sha512_224(key, data)` | `str` | HMAC-SHA-512/224 hex signature |
| `crypto.hmac_sha512_256(key, data)` | `str` | HMAC-SHA-512/256 hex signature |
| `crypto.token(bytes: 32)` | `str` | Cryptographically secure random hex token |
| `crypto.random_bytes(n)` | `list[int]` | `n` CSPRNG bytes |

```keel
crypto.sha256("hello")                        # "2cf24db..."
crypto.sha384("hello")
crypto.hmac_sha256(secret, "msg")
crypto.token()                                # 64-char hex string (32 bytes)
crypto.token(bytes: 16)                       # 32-char hex string
crypto.random_bytes(16)                       # list[int] of 16 bytes
```

`Crypto` intentionally exposes fixed safe SHA-2 methods only. MD5, SHA-1, and string-selected hash algorithms are not available through `Crypto`.

---

## 17. Shell — Subprocess Bridge

`Shell` lets agents invoke external commands and capture their output. It is gated by `@tools [shell]` — an agent must declare the capability before any `shell.run` call is allowed.

### `shell.run`

```
shell.run(cmd: str, stdin: str? = none, cwd: str? = none) -> { stdout: str, stderr: str, exit_code: int }
```

`cmd` is passed to `/bin/sh -c`, so pipes, redirects, and shell builtins work as expected.

| Argument | Type | Notes |
|---|---|---|
| `cmd` | `str` | Shell command (positional, required) |
| `stdin:` | `str?` | Text piped to the process's standard input |
| `cwd:` | `str?` | Working directory for the process (defaults to the interpreter's working directory) |

**Return value** — always a map with three keys:

| Key | Type | Notes |
|---|---|---|
| `stdout` | `str` | Captured standard output (UTF-8; invalid bytes replaced with `?`) |
| `stderr` | `str` | Captured standard error |
| `exit_code` | `int` | Exit code; `0` on success, non-zero on failure; `-1` if the OS cannot report one |

**Error semantics:**

- If `/bin/sh` cannot be spawned (e.g. missing in `PATH`), `shell.run` **raises** at runtime.
- A non-zero exit code is **not** an error — it is returned in `exit_code`. The caller decides whether to raise.

```keel
agent Builder {
    @tools [shell]

    @on_start {
        r = shell.run("cargo test --quiet 2>&1")
        if r.exit_code != 0 {
            raise "build failed:\n{r.stdout}"
        }
        io.show("Tests passed.")
    }
}
run(Builder)
```

**Capability gating:** `@tools` restricts an agent to the listed namespaces. If an agent declares `@tools [io]` but not `Shell`, any `shell.run` call raises `CapabilityError` at runtime. An agent with no `@tools` declaration is unrestricted. This gating is process-level, not OS-level — future releases may add stricter sandboxing.

**Environment isolation:** The subprocess runs with a clean environment. Only `PATH`, `HOME`, `SHELL`, `TMPDIR`, `USER`, and `LANG` are forwarded from the keel process. All other variables — including secrets, API keys, or credentials present in the keel process environment — are not visible to the shell command. To read the keel process environment from within a script, use `env.*` instead.

**Security note:** `cmd` is passed directly to `/bin/sh -c`. Never interpolate untrusted user input into `cmd` without sanitisation.

---

## 18. Escape Hatches


### 17.1 `ai.prompt` — raw LLM access

```keel
score = ai.prompt(
  system: "Rate sentiment 1–10.",
  user: "Text: {review}",
  response_format: json
) as SentimentScore
# score: SentimentScore? — parsing/validation may fail
```

`ai.prompt(...)` **must be followed by `as T`**. A bare `ai.prompt(...)` that tries to use the result is a compile error. Use `as dynamic` to explicitly opt out of typing.

### 17.2 `http.request` — raw HTTP

```keel
r = http.request(
  method: POST,
  url: "https://api.example.com/v2",
  headers: {Authorization: "Bearer {env.require("API_KEY")}"},
  body: {text: review},
  timeout: 10.seconds
)
# r: HttpResponse?
```

### 17.3 `db.query` — raw SQL

```keel
rows = db.query(
  "SELECT * FROM interactions WHERE contact = ? AND created_at > ?",
  params: [email.from, 30.days.ago]
)
# rows: list[dynamic]
```

### 17.4 `extern` — call external code

```keel
extern task tokenize(text: str) -> list[str] from "nlp_utils"

tokens = tokenize(document.body)
```

`extern` is the one place type annotations are mandatory — the compiler can't infer across a language boundary. Runtime dispatches via a plugin ABI (shared library or subprocess+JSON).

---

## 19. Environment & Configuration

### 18.1 Environment variables

```keel
api_key = env.require("OPENAI_API_KEY")   # fails at startup if missing
db_url  = env.get("DATABASE_URL")          # str? — none if missing
```

`std/env` is backed by the host environment; `use std/env` binds `env`.

### 18.2 Configuration file

`keel.config` (YAML) is loaded by the runtime at startup and populates default attribute values:

```yaml
model: "smart"
ai:
  default_temperature: 0.7
  cost_limit_daily: 10.00
memory:
  backend: sqlite
  path: .keel/memory.db
log:
  level: info
```

---

## 20. Modules & Imports

Keel has one source file type: `.keel`. Every file is both **runnable** (an
entrypoint when executed directly) and **importable** (a module when another
file `use`s it). Stdlib modules and local modules are the same concept.

### 20.1 The module model

```keel
use std/testing
use std/file
use "./validation.keel"

test "valid email" {
  assert validation.email("ada@example.com")
}

task load_config() -> str {
  file.read("config.json")
}
```

A module's identity is its resolved path (`std/<name>` for stdlib modules,
the canonical file path for local ones). Each file in a program is parsed
and loaded exactly once, no matter how many files import it.

### 20.2 `use` forms

| Form | Binds | Notes |
|---|---|---|
| `use std/file` | `file` | last path segment |
| `use std/file as f` | `f` | alias |
| `use "./validation.keel"` | `validation` | file stem |
| `use "./validation.keel" as v` | `v` | alias |
| `use A, B as C from "./m.keel"` | `A`, `C` | symbol import, per-item `as` |
| `use parse from std/json` | `parse` | symbol import from std |

Namespace derivation is predictable: the explicit alias, otherwise the file
stem (local) or last segment (std). Imports never put names into scope
implicitly; `use ... from ...` is the explicit opt-in for unqualified names.

### 20.3 Names, shadowing, collisions

Module bindings are identifiers, not keywords (§3.5). Within a body, a
lexical binding may shadow a module binding — `conn = db.connect(url)` is
the idiomatic pattern (the linter warns when a binding shadows an import).

Collisions are compile errors with an alias fix-it:

- two imports binding the same name in one file;
- a top-level declaration reusing a name bound by an import in that file.

### 20.4 Exports

Every top-level declaration — `task`, `type`, `agent`, `interface`,
`extern` — is exported. There is no `pub` keyword (reserved; see §20.11).
Top-level *statements* are not declarations: they are never exported and
never run on import (§20.5).

Module constants do not exist yet; use a zero-arg task. A `const`
declaration is reserved for a future release.

### 20.5 Execution: the implicit main

Importing a module loads its declarations only. Top-level statements form
the file's **implicit main** and execute — in order, sharing one scope —
only when that file is the entry file of `keel run` or `keel test`. Every
`.keel` file is therefore both a library and a runnable script with zero
boilerplate, and an import can never have side effects.

Agents are exported like any declaration: `run(watchers.Watcher)` starts an
imported agent; `use Watcher from "./watchers.keel"` then `run(Watcher)`
works identically.

`impl` blocks travel with their module: importing a module activates its
impls program-wide. An impl must live in the same module as the type or the
interface it implements.

**Modules gate entry points; values carry their methods.** `conn.query(...)`,
`dt.format(...)`, `id.to_str()`, and every other value method dispatch on
the value's type and need no import. Only entry-point calls
(`db.connect(...)`, `time.now()`, `uuid.v4()`) require the module binding.

### 20.6 Resolution

- Relative imports (`"./..."`, `"../..."`) resolve from the **importing
  file's** directory and must name a `.keel` file. There are no search
  paths.
- `std/<name>` resolves only against the catalog compiled into the `keel`
  binary. No `KEEL_PATH`; a local `./std/` directory cannot shadow the
  stdlib. An unknown std name is an error listing the available modules.
- Resolution is case-sensitive.

### 20.7 Circular imports

Cycles are a compile error reporting the full path
(`a.keel → b.keel → a.keel`) with the remediation: move the shared
declarations into a third file both can import. This restriction may be
relaxed in a later release; programs that compile today keep compiling.

### 20.8 Tests and modules

`keel test file.keel` runs only that file's `test` blocks. Imported modules
contribute declarations — test helpers are ordinary tasks — never their
tests. `keel test <dir>` runs each file's own tests (§19).

### 20.9 The standard library as modules

Every stdlib namespace is a module under `std/`:

`std/ai`, `std/io`, `std/http`, `std/email`, `std/file`, `std/shell`,
`std/json`, `std/csv`, `std/cache`, `std/search`, `std/db`, `std/memory`,
`std/schedule`, `std/async`, `std/control`, `std/env`, `std/time`,
`std/log`, `std/random`, `std/uuid`, `std/crypto`, `std/math`,
`std/testing`.

Always in scope without imports (language-level, not library):

- agent verbs: `run`, `stop`, `send`, `delegate`, `broadcast`;
- generic utilities: `min`, `max`, `typeof`;
- built-in types (`str`, `int`, `datetime`, `duration`, `Uuid`, …),
  duration literals (`5.minutes`), and built-in interfaces (§18);
- `self`, attributes (`@role`, `@tools`, …), and the contextual test words.

`Uuid` is split: the type is built in; the constructors (`uuid.v4()`,
`uuid.parse()`, `uuid.DNS`, …) live in `std/uuid`. There is no `std/agent`
module — agent verbs are language-level.

`@tools` capability lists name modules: `@tools [shell, http]`.

The REPL pre-imports the entire stdlib for convenience.

### 20.10 One global namespace (v0.1)

The runtime registers every module's declarations in one flat global table;
modules are a visibility discipline enforced statically. Consequently a
name must mean the same thing across the whole program:

- two modules may not declare the same top-level name;
- two files may not bind the same import name to different targets.

Both are compile errors naming the conflicting files. Types, enums, and
interfaces are accessed by symbol import (`use Urgency from ...`), keep
their declared identity (no `as` on type imports), and may only appear in
annotations of files that declare or import them. Module-private scoping is
planned.

### 20.11 Reserved

- `community/...` package paths parse but error — registry, versioning,
  and resolution are unspecified.
- Nested std paths (`std/http/server`) parse but no nested module exists.
- `pub` / export lists and module-level `const` declarations.

---

## 21. Operators

| Operator | Meaning |
|---|---|
| `\|>` | Pipeline — left value as first arg to right function |
| `=>` | Case mapping in `when` |
| `->` | Return type annotation |
| `??` | Null coalesce |
| `?.` | Null-safe field access |
| `!.` | Null assertion (throws) |
| `..` | Inclusive range |
| `in` | Membership |
| `as` | Type coercion / narrowing — see coercion table below |
| `==` `!=` `<` `>` `<=` `>=` | Comparison |
| `and` `or` `not` | Boolean logic |
| `+` `-` `*` `/` `%` | Arithmetic |
| `+=` `-=` `*=` `/=` | Augmented assignment — desugars to `x = x op rhs` |
| `...expr` | Spread — expands `list[T]` or `set[T]` into variadic slots at a call site |

`and`/`or` short-circuit: the right operand is evaluated only when the
left one doesn't already decide the result (`false` for `and`, `true` for
`or`). A right operand with a side effect does not run when short-circuited:

```keel
task log_and_true() -> bool {
  io.show("evaluated")
  true
}

false and log_and_true()   # "evaluated" never prints — result is false
true or log_and_true()     # "evaluated" never prints — result is true
```

### `as T` coercion rules

`expr as T` coerces the value at runtime. Unsupported conversions raise.

| From | To | Result |
|---|---|---|
| `int` | `float` | Widens: `5 as float` → `5.0` |
| `float` | `int` | Truncates toward zero: `1.9 as int` → `1`, `-1.9 as int` → `-1` |
| `int` / `float` / `bool` | `str` | Display string: `42 as str` → `"42"` |
| `str` | `int` | Parses; raises if not a valid integer |
| `str` | `float` | Parses; raises if not a valid float |
| `str` | `bool` | `"true"` → `true`, `"false"` → `false`; raises otherwise |
| `Uuid` | `str` | Hyphenated string: `"f47ac10b-..."` |
| `str` | `Uuid` | Validates UUID format; raises if invalid |
| same type | same type | Identity |
| `list` | `list[T]` | Validates the value is a list, then recurses the element cast to `T`; raises on a non-list value or any element that cannot cast |
| `map` | `map[K, V]` | Validates the value is a map, then recurses the value cast to `V`; keys pass through unchanged (v0.1 does not coerce or re-validate keys); raises on a non-map value |
| `list` | `(T1, T2, …)` | Tuple target: validates the value is a list of the same arity, then recurses each element cast position-wise; raises on a non-list, an arity mismatch, or any element that cannot cast |
| `dynamic` | any | Narrowed at runtime by the rules above — the runtime value must fit the target, otherwise it raises. This is how `json.parse` and `ai.prompt` results are narrowed, e.g. `json.parse(body) as list[dynamic]`. |
| `none` | any | Raises |
| anything else | | Raises |

A `dynamic` element or value type (`list[dynamic]`, `map[str, dynamic]`) is a
pass-through — the elements are not re-checked — so narrowing a parsed JSON
payload to `list[dynamic]` only asserts the top-level shape. Container casts
always assert the runtime shape: `json.parse("42") as list[dynamic]` raises
because the value is an integer, not a list.

```keel
1 as float          # ok — float
1.7 as int          # ok — 1 (truncated)
"42" as int         # ok — 42
"3.14" as float     # ok — 3.14
"abc" as int        # raises: cannot cast "abc" to int
none as int         # raises: cannot cast none to int

json.parse("[1,2,3]") as list[dynamic]    # ok — [1, 2, 3]
json.parse("[1,2,3]") as list[int]        # ok — recurses each element
json.parse("{\"a\":1}") as map[str, dynamic]  # ok — {a: 1}
json.parse("[1,2]") as (int, int)        # ok — tuple (1, 2)
json.parse("42") as list[dynamic]         # raises: cannot cast int to list
```

---

## 22. Execution Model

Keel runs on the **Keel Runtime** (Rust, Tokio).

```
v0.1 (alpha):   .keel → Lexer → Parser → Typechecker → Interpreter
(in progress):  Typechecker → KIR (mid-level IR, scalar subset today) → `keel build --emit=kir`
(later, TBD):   KIR → LLVM → native binary (`keel build`; see designs/llvm-compilation.md)
```

The interpreter is the only execution path v0.1 ships; `keel run` and `keel test` use it exclusively, and it remains the reference semantics for the native backend once that lands (`designs/llvm-compilation.md` §3). The bytecode-VM stub that previously reserved the `keel build` verb (`src/vm/`) has been removed — KIR takes over that role.

### Runtime services

The runtime is intentionally small. It provides only what stdlib needs to exist on top of it:

1. **Event loop** (Tokio).
2. **Agent scheduler** — mailboxes, handler sequencing, structured cancellation.
3. **Timer primitives** — `sleep`, `deadline`. Stdlib `schedule.*` is built on these.
4. **Interface dispatch** — registry of installed implementations per interface.
5. **Plugin ABI** — for `extern` and dynamically loaded stdlib backends.
6. **Tracer hook** — emits structured events at task/handler boundaries; stdlib `log.*` subscribes.

Everything else — HTTP, IMAP/SMTP, LLM clients, databases, vector stores — is stdlib and ships with the runtime binary but is replaceable.

---

## 23. Compile-Time Errors

| Error | Severity |
|---|---|
| Type mismatch | Error |
| Non-exhaustive match | Error |
| Nullable access without `?.` / `??` / `when` | Error |
| Unknown identifier | Error |
| Missing `else` on `if`-expression | Error |
| Branch of a value-position `if`/`when` that produces no value | Error |
| Missing `_` in non-enum `when` | Error |
| `self` outside an agent | Error |
| `ai.prompt(...)` without `as T` | Error |
| Unused variable | Warning |
| Shadowed built-in name | Warning |
| Unreachable code / catch | Warning |
| Deprecated attribute | Warning |

| Incompatible operand types (`"x" + 5`, `true + 1`, etc.) | Error |

All `keel check` errors and warnings include a source-span pointer (line:column) and an underlined excerpt. Arity errors list the expected parameter names as a correction hint.

`check_binop` validates that arithmetic and comparison operands are type-compatible; `Unknown`/`Dynamic` operands are always accepted (gradual typing escape hatch). Augmented assignment (`+=`, `-=`, etc.) is checked with the same rules.

---

## 24. Lint Rules (`keel lint`)

`keel lint` checks for style and best-practice issues that are not type errors. The program may still run; lint warnings indicate dead code or likely misuse patterns.

| Rule | Trigger | Suppression |
|------|---------|-------------|
| Unused variable | binding assigned but never read | prefix name with `_` |
| Uncalled task | `task` declared but never invoked | — |
| `ai.*` outside agent | LLM method called without `@role` / `@model` context | — |
| State written, never read | `self.x =` appears but `self.x` never used | — |

`keel lint --fix` auto-removes unused variable assignment lines.

---

## 25. IDE Contract

Every feature is designed for tooling.

| Context | Autocomplete |
|---|---|
| After `ai.` | `classify`, `draft`, `summarize`, etc. |
| After `ai.classify(x, as: ` | In-scope enum types |
| After `@` inside agent body | Registered attribute names |
| After `email.` | Fields of email's structural type |
| After `when urgency { ` | Variants of the enum, marking covered/uncovered |
| After `delegate(` | In-scope agent names |
| After `using: ` | Known model strings |

**Hover:** infers and displays types, signatures, attribute docs.

**Go-to-definition:** works through types, module bindings, interface implementations.

**Refactoring:** rename is variant-aware and interface-aware.

---

## 26. Formal Grammar (PEG summary, condensed)

```peg
Program     <- (Decl / Stmt)* EOF
Decl        <- Agent / TaskDecl / TestDecl / TypeDecl / InterfaceDecl / ExternDecl / UseDecl

Agent       <- "agent" IDENT "{" (Attribute / StateBlock / TaskDecl / OnHandler)* "}"
Attribute   <- "@" IDENT AttributeBody
AttributeBody <- STRING / Expr / Block / (IDENT "[" (Expr ",")* "]")   # flexible per handler
OnHandler   <- "on" IDENT "(" Params? ")" Block
StateBlock  <- "state" "{" (IDENT ":" "readonly"? Type ("=" Expr)? ","?)* "}"

TaskDecl    <- "task" IDENT TypeParams? "(" Params? ")" ("->" Type)? Block
Params      <- Param ("," Param)* ("," VariadicParam)? ("," NamedParam)*
             / VariadicParam ("," NamedParam)*
VariadicParam <- "..." IDENT ":" Type                                    # collected as list[T] inside body
NamedParam  <- IDENT ":" Type ("=" Expr)?                               # named, may have default
TestDecl    <- "test" STRING TestParam? "{" (SetupBlock / Stmt)* "}"
TestParam   <- "for" IDENT "in" Expr
SetupBlock  <- "setup" Block
InterfaceDecl <- "interface" IDENT "{" (TaskSig)* "}"
TaskSig     <- "task" IDENT "(" Params? ")" ("->" Type)?

TypeDecl    <- "type" IDENT TypeParams? "=" EnumDef                      # enum
             / "type" IDENT TypeParams? "{" FieldDef* "}"                # struct
             / "type" IDENT TypeParams? "=" Type                         # alias

TypeParams  <- "[" IDENT ("," IDENT)* "]"                               # e.g. [T], [A, B]
EnumDef     <- EnumVariant ("|" EnumVariant)*
EnumVariant <- IDENT ("{" FieldDef* "}")?

ExternDecl  <- "extern" "task" IDENT "(" Params? ")" "->" Type "from" STRING
UseDecl     <- "use" ImportItem ("," ImportItem)* "from" UseSource     # symbol import
             / "use" UseSource ("as" IDENT)?                            # module import
UseSource   <- STRING / ModulePath
ModulePath  <- IDENT ("/" IDENT)+
ImportItem  <- IDENT ("as" IDENT)?

# --- Expressions ---
Expr        <- NullCoalesce
NullCoalesce <- PipeExpr ("??" PipeExpr)?
PipeExpr    <- OrExpr ("|>" OrExpr)*
OrExpr      <- AndExpr ("or" AndExpr)*
AndExpr     <- NotExpr ("and" NotExpr)*
NotExpr     <- "not"? CompExpr
CompExpr    <- RangeExpr (("==" / "!=" / "<" / ">" / "<=" / ">=") RangeExpr)?
RangeExpr   <- AddExpr (".." AddExpr)?
AddExpr     <- MulExpr (("+" / "-") MulExpr)*
MulExpr     <- UnaryExpr (("*" / "/" / "%") UnaryExpr)*
UnaryExpr   <- ("-" / "not")? PostfixExpr
PostfixExpr <- PrimaryExpr (FieldAccess / NullAccess / AssertAccess / Call / Index / Cast)*
FieldAccess <- "." (IDENT / INT_LIT)
NullAccess  <- "?." IDENT
AssertAccess<- "!." IDENT / "!"                                              # bare ! = null assert
Call        <- "(" Args? ")"
Index       <- "[" Expr "]"
Cast        <- "as" Type
Args        <- Arg ("," Arg)*
Arg         <- (IDENT ":")? Expr                                         # named args supported
             / "..." Expr                                                # spread: expands list/set into variadic slots

PrimaryExpr <- Literal / SelfExpr / IDENT / Lambda
             / TupleLit / ListLit / MapLit / SetLit
             / IfExpr / TryExpr
             / "(" Expr ")"

SelfExpr    <- "self" "." IDENT                                          # field access → value of that state field
             / "self"                                                     # bare self → AgentRef for the current agent and task-call receiver

Lambda      <- IDENT "=>" (Expr / Block)
             / "(" LambdaParams? ")" "=>" (Expr / Block)

IfExpr      <- "if" Expr Block ("else" (IfExpr / Block))?
WhenArm     <- Pattern ("," Pattern)* ("where" Expr)? "=>" (Expr / Block)
Pattern     <- VariantPat / StructPat / IDENT / "_" / Literal
VariantPat  <- IDENT ("{" IDENT ("," IDENT)* ","? "}")?   # enum variant; optional field bindings
StructPat   <- "{" IDENT ("," IDENT)* ","? "}"            # struct field bindings (no leading ident)

# --- Statements ---
Stmt        <- ReturnStmt / RaiseStmt / AssertStmt / AugAssignStmt / AugSelfAssign / AssignStmt / SelfAssign / ForStmt / TryStmt / ExprStmt
ReturnStmt  <- "return" Expr?
RaiseStmt   <- "raise" Expr
AssertStmt  <- "assert" Expr ("," Expr)?
AugOp       <- "+=" / "-=" / "*=" / "/="
AugAssignStmt <- IDENT AugOp Expr                   # desugars to x = x op rhs
AugSelfAssign <- "self" "." IDENT AugOp Expr        # desugars to self.f = self.f op rhs
AssignStmt  <- AssignTarget (":" Type)? "=" Expr
SelfAssign  <- "self" "." IDENT "=" Expr
ForStmt     <- "for" (IDENT / DestructPat) "in" Expr ("if" Expr)? Block
TryStmt     <- "try" Block CatchClause+
CatchClause <- "catch" IDENT ":" Type Block

# --- Terminals, literals, duration as before (§2.2) ---
```

Notably absent: dedicated grammar for `ClassifyExpr`, `ExtractExpr`, `SummarizeExpr`, `DraftExpr`, `TranslateExpr`, `DecideExpr`, `PromptExpr`, `HttpExpr`, `SqlExpr`, `AskExpr`, `ConfirmExpr`, `NotifyStmt`, `ShowStmt`, `FetchExpr`, `SearchExpr`, `SendStmt`, `ArchiveStmt`, `RememberStmt`, `ForgetStmt`, `RecallExpr`, `EveryBlock`, `AfterStmt`, `WaitStmt`, `RetryStmt`, `ParallelExpr`, `RaceExpr`, `DelegateExpr`, `ConnectStmt`, `BroadcastStmt`, `RunStmt`, `RulesBlock`, `ConfigBlock`, `ToolsClause`, `TeamClause`, `RoleClause`, `ModelClause`, `MemoryClause`. All are ordinary function calls in the stdlib or stdlib attribute handlers.

That keeps the parser small, type inference uniform (no hard-coded primitive signatures), and the IDE free of per-keyword special cases.

---

## 27. What's Next

Keel is in the **0.2.x alpha**. `keel run` accepts the surface described in this document.

The path to v1.0 is deliberately left **un-planned** — real usage drives what gets scoped next. See the [GitHub project board](https://github.com/orgs/keel-lang/projects/1) for planned work and [NON-GOALS.md](NON-GOALS.md) for what's been set aside.

Breaking changes are expected between 0.x versions. Don't build production systems on it yet. Play, prototype, break things, and send feedback.

---

*This specification is a living document. Syntax and semantics will evolve through alpha and beta phases.*
