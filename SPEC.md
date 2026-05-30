# Keel Language Specification — v0.1 (Alpha)

> **Status: Alpha.** Keel is in early design. The language is **not yet stable** and has **no production users**. Expect breaking changes between 0.x releases. This document is the design target for v0.1; [ROADMAP.md](ROADMAP.md) tracks shipped, partial, and planned implementation status.

---

## 0. The Shape of Keel

Keel is a programming language for building AI agents. Two ideas define it:

1. **The actor model is core.** An `agent` is the primitive unit of concurrency — a serial-handler mailbox with isolated mutable state. This is the only primitive that can't be a library.
2. **Everything else is a library.** AI calls, scheduling, I/O, HTTP, memory, search, tool integration — all live in a standard library that ships with the runtime and is **auto-imported** (the *prelude*). Users don't write `use keel/ai`; they just write `Ai.classify(...)` and it works.

Because the prelude is auto-imported, `Ai.classify(...)` reads as if `classify` were a keyword — but the compiler doesn't know or care what `Ai` is. That keeps the core language small (fewer keywords, fewer parser special cases, fewer type-inference rules) while keeping the ergonomics.

### Design principles

1. **Small core, deep stdlib.** Every feature that can be a library is one. The core earns its keep through the type system, the compiler, or the actor runtime — not through surface syntax convenience.
2. **Static typing with full inference as the design target.** The alpha checker already catches core mismatches and deliberately leaves some unsupported cases as `Unknown`; [ROADMAP.md](ROADMAP.md) is the status source for current checker coverage.
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

Keel uses a **structural type system with full inference** as its design target. In the current alpha, the checker covers the core language and falls back to `Unknown` in some unsupported cases; those gaps are tracked in [ROADMAP.md](ROADMAP.md).

### 2.1 Design principles

1. **Structural typing.** Types are shapes, not names. A value matches a type if it has the required fields. No explicit `implements`.
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

Maps expose `.count`, `.keys`, `.values`, `.get(k) → V?`, `.contains(k)`. Sets expose `.count`, `.contains(v)`, `.is_empty`.

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

**Width subtyping.** A value of type `A` is assignable to `B` if `A` has all fields of `B` with compatible types. Extra fields are allowed.

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

**Pattern matching** is exhaustive (see §8.2). Rich variant fields are destructured in `when` arms, not accessed via dot.

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

### 2.9 The `dynamic` type (FFI/interop only)

`dynamic` exists for untyped boundaries: `extern` returns, `prompt as dynamic`, raw SQL rows, and JSON/cache interop. It must always be explicitly written — there is no implicit path to `dynamic`.

```keel
extern task parse_legacy(data: str) -> dynamic from "legacy"

raw = Ai.prompt(...) as dynamic       # must opt in
info: MyStruct = raw as MyStruct      # narrow with runtime check
```

`dynamic` defeats autocomplete and type checking. Narrow as early as possible. The compiler warns on `dynamic` use outside the explicit escape hatches.

**Strict runtime arguments.** Runtime APIs enforce their declared arguments. A dynamic value passed to `File.read(path: str)`, `Cache.get(key: str)`, `Json.parse(s: str)`, or another typed API is rejected when its runtime type does not match. Required namespace and value-method arguments also raise an error when omitted. Display formatting is explicit: interpolation, `Io.*`, and `Log.*` may render arbitrary values, while data APIs do not silently stringify them.

**`Json.parse` return-type semantics.** `Json.parse(s)` returns an untyped value — the type checker does not infer a precise return type. Narrow with `as T` before use, or annotate the binding as `dynamic` to opt out of static typing at that site. The JSON-to-Keel runtime mapping is:

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
body = Http.get("https://api.example.com/ticker")?.body ?? ""
data = Json.parse(body) as dynamic
price  = (data.price as str).to_float() ?? 0.0  # str field → float
volume = data.volume as int                      # int field
rows   = data.candles as list[dynamic]           # array field
for row in rows {
  close = (row as list[dynamic])[4] as str       # nested array element
}
```

In strict mode (`keel check --strict`), an unannotated `Json.parse` binding is flagged because its type cannot be statically inferred. Two accepted escape hatches silence the warning:

```keel
# Cast form — annotate the cast target
data = Json.parse(body) as dynamic

# Annotation form — declare the binding type explicitly
data: dynamic = Json.parse(body)
```

Both are accepted by strict mode because `dynamic` is an intentional programmer choice, not a checker gap. `Unknown` (an unannotated, un-narrowed result) is what triggers the strict diagnostic.

**`Cache.get` return-type semantics.** `Cache.get(key: str) -> dynamic?` returns the stored value at its original type, or `none` if the key is absent or the entry has expired. The stored type is preserved exactly — a value written as `str` is read back as `str`, a value written as `int` is read back as `int`. Use `as T` to recover a concrete type:

```keel
Cache.set("price", "50000.12")
raw = Cache.get("price")                # dynamic?
if raw != none {
  price = raw as str                    # "50000.12"
}

Cache.set("count", 42)
n = (Cache.get("count") ?? 0) as int   # 42
```

### 2.10 Built-in runtime types

Provided by the prelude, available without imports:

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
# Construction via Uuid.v4(), Uuid.v7(), Uuid.v5(ns:, name:), or uuid() shorthand
# uuid() is an alias for Uuid.v4()
# Methods: .version() -> int, .to_str() -> str, .format(as: "hyphenated"|"simple"|"urn") -> str
# Uuid.parse(s: str) -> Uuid?  — none if invalid format
# Predefined namespace constants: Uuid.DNS, Uuid.URL, Uuid.OID, Uuid.X500

type Error =
  | AIError { model: str, tokens_used: int }
  | NetworkError { status: int?, url: str }
  | TimeoutError { duration: duration }
  | NullError
  | TypeError { expected: str, got: str }
  | ParseError { position: int }
# All variants implicitly carry message: str, source: str?
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

## 3. The Prelude (the Stdlib as Keywords)

The Keel standard library lives in a set of namespaces that are **auto-imported into every program**. Users don't write `use keel/ai` to get `Ai.classify` — the name is already in scope.

### 3.1 Why a prelude

- **Small core.** The compiler doesn't know about `classify`, `fetch`, or `every`. Those are stdlib function calls that happen to always be in scope. Parser, lexer, and type checker stay free of domain-specific special cases.
- **Keyword feel.** Users still write `Ai.classify(...)` without ceremony. The namespace qualifier is short; autocomplete takes care of the rest.
- **Swappable implementations.** Stdlib functions dispatch through **interfaces** (§5). Users can install their own LLM provider, scheduler, memory store, or HTTP client without leaving the language.
- **No grammatical ambiguity.** `fetch x where y` required whole-grammar disambiguation. `Http.get(x, where: y)` is unambiguous and tool-friendly.

### 3.2 Prelude namespaces (v0.1)

| Namespace | Purpose | Key operations |
|---|---|---|
| `Ai` | LLM-backed operations | `classify`, `extract`, `summarize`, `draft`, `translate`, `decide`, `prompt`, `embed` |
| `Io` | Human interaction | `ask`, `confirm`, `notify`, `show` |
| `Http` | HTTP client | `get`, `post`, `request` |
| `Email` | IMAP/SMTP | `fetch`, `send`, `archive` |
| `Search` | Web search providers | `web(query)`, custom providers via interface |
| `Db` | SQLite databases | `connect(url) -> DbConnection`, `db.query(sql, params?) -> list[map[str,dynamic]]`, `db.exec(sql, params?) -> int` |
| `Memory` | Per-agent key-value store | `remember(key, value)`, `recall(key) -> Value?`, `forget(key)` |
| `File` | Local filesystem | `read`, `write`, `exists`, `list`, `mkdir`, `remove`, `copy`, `move`, `glob`, `mktemp` |
| `Schedule` | Time-based scheduling | `every`, `after`, `at`, `cron` |
| `Async` | Structured concurrency | `spawn`, `join_all`, `select`, `sleep` |
| `Control` | Control combinators | `retry`, `with_timeout`, `with_deadline` |
| `Env` | Environment and config | `get(name)`, `require(name)` |
| `Time` | Time utilities | `now()`, `parse`, `format`, `diff`, duration math |
| `Log` | Structured logging | `info`, `warn`, `error`, `debug` |
| `Agent` | Agent lifecycle | `run`, `stop`, `send(target, message)`, `delegate`, `broadcast` (also exposed as bare `run`/`stop` at top level) |
| `Random` | Pseudo-random generation | `float()`, `int(min:, max:)`, `bool()` |
| `Uuid` | UUID generation | `v4()`, `v7()`, `v5(ns:, name:)`, `parse(s)` |
| `Crypto` | Cryptographic primitives | `sha256(data)`, `hmac_sha256(data, key:)`, `token(bytes:)`, `random_bytes(n)` |
| `Math` | Transcendental and power functions | `PI()`, `E()`, `sqrt(x)`, `pow(x, y)`, `exp(x)`, `log(x)`, `log2(x)`, `log10(x)`, `sin(x)`, `cos(x)`, `tan(x)`, `asin(x)`, `acos(x)`, `atan(x)`, `atan2(y, x)` |
| `Csv` | CSV serialization | `parse(text)`, `parse_records(text)`, `stringify(rows)` |
| `Shell` | Subprocess bridge | `run(cmd, stdin:?, cwd:?) -> { stdout, stderr, exit_code }` |

### 3.3 Prelude free functions

A small set of functions live directly in the root scope — no namespace qualifier needed:

| Function | Signature | Returns | Notes |
|---|---|---|---|
| `uuid()` | `() -> Uuid` | `Uuid` | Alias for `Uuid.v4()` |
| `min(...)` | `(...items: T, by: ((T) -> any)? = none) -> T?` | `T?` | Minimum; `none` on empty |
| `max(...)` | `(...items: T, by: ((T) -> any)? = none) -> T?` | `T?` | Maximum; `none` on empty |
| `typeof(x)` | `(any) -> str` | `str` | Runtime type name: `"int"`, `"float"`, `"str"`, `"bool"`, `"none"`, `"list"`, `"map"`, `"duration"`, `"Uuid"`, or the declared name for structs and enums (`"Point"`, `"Color"`) |

```keel
id = uuid()                           # Uuid

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

### 3.5 Prelude surface is identifiers, not keywords

`Ai`, `Io`, `Schedule`, etc. are **identifiers** whose bindings are installed by the runtime into the root scope. A user program can shadow them (`Ai = my_module` is legal, if unwise). They do not appear in the reserved keyword list (§10). This is the crucial difference: the language doesn't know about `Ai`. The runtime does.

### 3.6 Example: everything you need, no imports

```keel
# Zero imports. All namespaces are in scope.

type Urgency = low | medium | high | critical

agent EmailBot {
  @role "Professional email triage"

  state {
    processed: int = 0
  }

  on message(msg: Message) {
    urgency = Ai.classify(msg.body, as: Urgency) ?? Urgency.medium

    when urgency {
      low, medium => {
        reply = Ai.draft("response to {msg.body}", tone: "friendly")
        if Io.confirm(reply) {
          Email.send(reply, to: msg.from)
        }
      }
      high, critical => {
        Io.notify("{urgency}: {msg.subject}")
        guidance = Io.ask("How to respond?")
        reply = Ai.draft("response to {msg.body}", guidance: guidance)
        if Io.confirm(reply) {
          Email.send(reply, to: msg.from)
        }
      }
    }

    self.processed = self.processed + 1
  }

  # Scheduling is a library call, not a keyword.
  # The block registers a recurring event on this agent's mailbox.
  @on_start {
    Schedule.every(5.minutes, () => {
      for email in Email.fetch(unread: true) {
        # deliver to this agent's message handler
        Agent.send(self, email.as_message())
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
  @model "smart"                      # LLM binding for Ai.* inside this agent
  @tools [Email, Calendar]           # whole-namespace capability bindings
  # or with method-level guards:
  # @tools [Email.fetch, Email.send if self.confirmed, Http]
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
    Ai.draft("greeting for {name}", tone: "warm") ?? "Hello!"
  }

  # --- Event handlers ---
  on message(msg: Message) {
    response = self.greet(msg.from)
    Email.send(response, to: msg)
    self.processed = self.processed + 1
  }

  # --- Lifecycle hooks (stdlib attribute) ---
  @on_start {
    Schedule.every(1.day, at: @9am, () => {
      Io.notify("Good morning — {self.processed} messages processed yesterday")
    })
  }
}
```

### 4.3 Attributes (`@name`)

Attributes are identifier-prefixed metadata clauses inside an agent body. The core language knows only two attributes:

| Attribute | Core-defined? | Semantics |
|---|---|---|
| `@role` | Yes | The agent's identity string, bound to the installed `LlmProvider` for all `Ai.*` calls. |
| `@model` | Yes | The model name string, overrides the global default for this agent's `Ai.*` calls. |

Everything else (`@tools`, `@memory`, `@rules`, `@limits`, `@on_start`, `@on_stop`, custom attributes) is **stdlib-defined**: libraries register attribute handlers at startup, and the runtime invokes them during agent initialization to wire up capabilities.

**`@tools` — capability gating**

`@tools` restricts which namespace methods the agent may call. Each entry is one of:

```
Ns                          # whole namespace, always allowed
Ns.method                   # specific method, always allowed
Ns if expr                  # whole namespace, allowed when expr is true
Ns.method if expr           # specific method, allowed when expr is true
```

`expr` is any boolean expression evaluated at the start of each handler turn. `self.*` state, `self.task(...)`, and top-level task calls returning `bool` are valid. Calling a blocked method raises `CapabilityError`.

```keel
@tools [
  Email.fetch,                      # always can read
  Email.send if self.confirmed,      # send only after confirmation
  Db.query,
  Db.exec   if self.admin,
  Http,                             # whole namespace, always
]
```

**Why attributes and not keywords?** A keyword requires a grammar rule and couples the compiler to a specific feature. An attribute is just a name. Adding `@my_custom_attr` requires no language change — only a handler in the library that provides it. The user's file parses identically regardless of which libraries are loaded.

### 4.4 Agent lifecycle

```keel
run(MyAgent)                 # start
run(MyAgent, background: true)  # non-blocking
stop(MyAgent)                # graceful shutdown
```

`run` and `stop` are **prelude functions** in the `Agent` namespace, re-exported at top level for convenience.

### 4.5 State and thread safety

- Agent `state` fields are mutable **only via `self.`**.
- Event handlers for one agent run **sequentially**. No concurrent access to `state`.
- Different agents run concurrently but share no state.
- Cross-agent data flows through `Agent.delegate`, `Agent.broadcast`, or `Memory.*`.

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
    Io.show(self.session_id)             # reading is fine
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
  Ai.classify(email.body, as: Urgency) ?? Urgency.medium
}

agent EmailAssistant {
  @role "Triage and respond"
  on message(msg: Message) {
    urgency = triage(msg)
    Io.show({urgency: urgency, subject: msg.subject})
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
Io.show(p.print())    # → "(1.5, 2.0)"
```

**Rules for `impl` blocks:**

- `impl` and `for` are reserved keywords.
- The named interface must be declared (either user-defined or a built-in like `Stringable`) before any `impl` references it — unless both appear in the same file, in which case order does not matter (the compiler pre-collects all interface declarations).
- Every method listed in the interface must be provided. Missing methods are a compile-time error.
- Extra methods not listed in the interface are a compile-time error.
- Return types must match exactly.
- `self` inside the block receives the struct value. Use `self.field` to access fields.

**Dispatch rule.** The runtime identifies the concrete type by matching the struct's registered field names against the value's keys. When two types share identical field sets, method dispatch is ambiguous — add a distinguishing field or call `.method()` via an explicit variable with a declared type annotation.

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
Io.show("origin is {p}")    # → "origin is (3, 4)"
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

**`Serializable`** — override `Json.stringify`:

```keel
interface Serializable {
  task to_json(self) -> str
}
```

When a type implements `Serializable`, `Json.stringify(value)` calls `to_json()` instead of the default serialiser.

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
for n in Range { lo: 1, hi: 3 } { Io.show("{n}") }   # 1, 2, 3
```

### 5.4 Why interfaces are core

- `Ai.classify` needs to dispatch to *some* LLM implementation. Hard-coding a single provider into the runtime locks users out of self-hosted, proprietary, or novel backends.
- `Memory` in v0.1 is a plain K/V store (JSON file); in v0.2 it will dispatch through a `VectorStore` interface so users can swap backends.
- `Log.info` needs a sink — users want OTel, Datadog, or plain stdout.

The language can't know about every provider. Interfaces let stdlib declare the *protocol*, ship a default implementation, and, once the runtime registry is wired, let users swap implementations.

### 5.5 Installing implementations <span class="badge badge-soon">Planned</span>

Custom implementation installation is planned, but not registered in the v0.1 runtime yet. The intended startup shape is:

```keel
# At startup — swap the default LLM provider
Ai.install(MyCustomProvider)

# Per-agent override
agent Specialist {
  @model "my-custom-model"
  @provider MyOllamaProvider      # stdlib attribute that installs for this agent
}
```

Installation is scoped: per-program, per-agent, or per-call (via an explicit `using:` argument to stdlib functions).

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
  Ai.classify(email.body, as: Urgency) ?? Urgency.medium
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
    Io.show(self.summarize(msg.subject))
  }
}
```

Inside an agent, an unqualified `task(...)` call resolves only through lexical
and top-level scope. Agent-local tasks are not injected into bare-name lookup.
`MyAgent.task(...)` is not a cross-agent call form; use `Agent.send`,
`Agent.delegate`, or `Agent.broadcast` for mailbox-based coordination.

### 6.8 `Agent.delegate` — type-safe handler dispatch

`Agent.delegate` posts a named event to a target agent's mailbox. Two forms are supported:

**Symbol form (preferred):** `Agent.delegate(TargetAgent.handlerName, data)`

The handler reference `TargetAgent.handlerName` is resolved at compile time. The type
checker verifies:
1. `TargetAgent` is a declared agent.
2. `handlerName` is a declared `on` handler on that agent.
3. `data` matches the handler's declared parameter type (when the parameter is typed).

```keel
agent Worker {
  on process(task: Task) {
    Log.info("processing {task.id}")
  }
}

agent Boss {
  @on_start {
    Agent.run(Worker)
    Agent.delegate(Worker.process, my_task)   # ✓ type-checked at compile time
    Agent.delegate(Worker.typo, my_task)      # ✗ compile error: no handler `typo`
  }
}
```

**String form (legacy):** `Agent.delegate(TargetAgent, "handlerName", data)`

The handler name is a string literal. The type checker validates it when the string
is a plain literal (no interpolation). Handler renames do not update string literals
automatically — prefer the symbol form for all new code.

```keel
Agent.delegate(Worker, "process", my_task)   # checked when literal is plain
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
  Ai.draft("response", guidance: guidance)
} else {
  Ai.draft("response", tone: "friendly")
} ?? "(draft failed)"
```

An `if` without `else` used as an expression is a compile error.

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
  reply { to, tone }   => Email.send(Ai.draft("reply", tone: tone), to: to)
  forward { to }       => Email.send(email, to: to)
  archive              => Email.archive(email)
  escalate { reason, urgency }
    where urgency == Urgency.critical => page_oncall(reason)
  escalate { reason, _ } => Io.notify("Escalation: {reason}")
}
```

**Tuple and struct patterns, `where` guards** — see §24 grammar for the full form.

**Non-enum matching (primitives, strings):** wildcard `_` is **required** (the compiler can't prove exhaustiveness on unbounded types).

**Exhaustiveness:** All enum variants must be covered (or `_` present) in both statement and expression forms.

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

Catches by **variant matching** — the same mechanism as `when` on any enum. `Error` is the catch-all type.

```keel
try {
  Email.send(reply, to: email.from)
} catch err: NetworkError {
  Control.retry(3, backoff: exponential, () => { Email.send(reply, to: email.from) })
} catch err: Error {
  Io.notify("Send failed: {err.message}")
}
```

### 8.6 `raise`

Throws an error from a value. Symmetric with `try`/`catch` — you can throw as well as catch.

```keel
raise "validation failed"

# Caught by catch err: Error; err.message == "validation failed"
try {
  raise "quota exceeded"
} catch err: Error {
  Io.notify("Caught: {err.message}")
}
```

`raise` accepts any expression. If the value is a string it is used as the error message directly. Otherwise the value's display representation becomes the message. `raise` is always caught by `catch err: Error`.

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
    Io.show("tick: {n}")
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
| `Async.spawn(fn)` | `() -> T` returning `Task[T]` | Start a child task. Parent-cancels-children semantics. |
| `Task[T].await()` | `T` | Block the current handler until the task completes. |
| `Task[T].cancel()` | `none` | Cancel the task. |

Everything else is a library combinator:

```keel
Async.join_all(tasks: list[Task[T]]) -> list[T]   # all-or-nothing; cancels siblings on error
Async.select(tasks: list[Task[T]]) -> T            # first to complete wins
Async.sleep(d: duration) -> none
```

### 9.2 Structured concurrency

Cancellation is structured: when a parent task cancels or errors, all spawned children cancel. This is the one contract the runtime upholds.

### 9.3 No `parallel` / `race` keywords

Concurrent composition is expressed through library functions, not grammar:

```keel
[urgency, sentiment] = Async.join_all([
  Async.spawn(() => Ai.classify(body, as: Urgency) ?? Urgency.medium),
  Async.spawn(() => Ai.classify(body, as: Sentiment) ?? Sentiment.neutral)
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

Events land in the agent's mailbox. The runtime processes them one at a time. A handler that calls `Io.ask`, `Async.sleep`, or `Agent.delegate` suspends — other *agents* continue. Other events for the *same* agent queue behind.

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

Namespaces (`Ai`, `Io`, `Http`, `Schedule`, `Async`, …) are identifiers, not keywords. Same for `run`, `stop`, `spawn`, `delegate`, `broadcast` — prelude functions.

Attribute names (`@role`, `@model`, `@tools`, …) are identifiers. Only the `@` prefix is syntax.

Duration units (`seconds`, `minutes`, `hours`, `days`, `weeks`) are **identifiers recognized by the lexer in the `INT "."` position**, not reserved words.

---

## 11. Error Handling

### 11.1 Error types

`Error` is the catch-all type. All error values carry `message: str` implicitly. Catch clauses match by type name.

The two-tier model:

| Failure | Result | Handle with |
|---|---|---|
| Network failure / mock mode / timeout | Returns `none` | `??` or `when` |
| LLM output didn't match the expected schema | Throws `AiSchemaError` | `try/catch` |

`AiSchemaError` carries `message: str` and `got: str` (the raw LLM output that failed to match). It is caught by `catch err: AiSchemaError` or the catch-all `catch err: Error`.

### 11.2 Nullable-aware stdlib

`Ai.*` calls return `T?` for genuine absence (e.g. the model returned nothing parseable). Use `??` or `when` for the simple fallback case:

```keel
# Simple default via ??
summary = Ai.summarize(article, in: 3, unit: sentences) ?? "No summary available"
urgency = Ai.classify(text, as: Urgency) ?? Urgency.medium

# Explicit when
when Ai.classify(text, as: Urgency) {
  some(u)  => handle(u)
  none     => Io.notify("Could not classify")
}
```

When you need to distinguish *why* a call failed, use `try/catch`:

```keel
try {
  urgency = Ai.classify(email.body, as: Urgency) ?? Urgency.medium
} catch err: AiSchemaError {
  Io.notify("Unexpected LLM output: {err.got}")
  urgency = Urgency.medium
} catch err: AiError {
  Control.retry(3, () => {
    urgency = Ai.classify(email.body, as: Urgency) ?? Urgency.medium
  })
} catch err: Error {
  Io.notify("Unexpected failure: {err.message}")
}
```

### 11.3 Retry

`Control.retry` is a stdlib function:

```keel
Control.retry(3, backoff: exponential, () => {
  Email.send(reply, to: addr)
})

Control.retry(5, delay: 10.seconds, () => {
  Http.get("https://api.example.com/data")
})
```

### 11.4 Limits and rules

Both are stdlib-defined attributes (`@limits`, `@rules`):

- **`@limits`** are **deterministic** constraints enforced by the runtime: cost per request, token caps, timeouts, required-confirmation action lists. Violations are rejected.
- **`@rules`** are **natural-language instructions** the stdlib injects into LLM prompts. LLM compliance is best-effort, not guaranteed.

The separation is intentional: limits are verifiable, rules are aspirational. Mixing them would hide the difference.

---

## 12. Memory

`Memory` is a per-agent key-value store. Agents opt in with `@memory persistent` (survives restarts), `@memory session` (in-process, default), or `@memory none` (disables `Memory.*` entirely).

```keel
agent Counter {
  @memory persistent

  @on_start {
    count = Memory.recall("visits")
    next = if count == none { 1 } else { count + 1 }
    Memory.remember("visits", next)
    Io.show("Visit {next}")
    stop(self)
  }
}
```

### Operations

| Call | Returns | Notes |
|---|---|---|
| `Memory.remember(key, value)` | `none` | Store any Keel value under `key`, scoped to this agent |
| `Memory.recall(key)` | `Value?` | Return stored value or `none` if absent |
| `Memory.forget(key)` | `none` | Delete the key |

### Scope and isolation

Keys are namespaced per `(program, agent)` pair — two programs that happen to share an agent name (`Counter`) each get their own memory bucket. Two agents within the same program with different names also get separate buckets.

`Memory.*` is only valid inside an agent body. Calling it from a top-level statement or a plain `task` raises a runtime error.

### Persistence mode

| Attribute | Behaviour |
|---|---|
| `@memory session` | In-process HashMap; cleared at process exit (default when attribute is omitted) |
| `@memory persistent` | JSON file at `~/.keel/memory/<stem>_<hash12>/<agent>.json`; survives restarts |
| `@memory none` | Any `Memory.*` call raises `CapabilityError` |

#### Persistent storage path

The directory name is `<stem>_<hash12>` where `<stem>` is the sanitized basename of the source file and `<hash12>` is the first 12 hex characters of the SHA-256 hash of the canonicalized file path. This ensures two programs with the same filename in different directories never share storage.

Special sources that have no stable on-disk path use fixed namespace names:

| Source | Namespace |
|---|---|
| File (e.g. `counter.keel`) | `counter_<hash12>` |
| REPL | `__repl__` |
| stdin / inline | `__stdin__` / `__inline__` |

#### Multi-process safety

Each `Memory.*` operation acquires an advisory `flock` on a sidecar `<agent>.lock` file (exclusive for writes, shared for reads). Concurrent `keel run` processes against the same program/agent are safe — writes are serialized by the kernel lock. The lock target is a stable sidecar file that is never renamed.

### v0.2 note: semantic search

v0.1 `Memory` is a plain K/V store. The planned v0.2 upgrade adds a `VectorStore` interface (see §5.1) that backs `recall` with nearest-neighbour embedding search. The v0.1 API surface is a strict subset — existing programs will keep working when the backend is upgraded.

---

## 13. Time

The `Time` namespace provides datetime construction and parsing. Datetimes are RFC 3339 strings with an explicit timezone offset — naive strings (no offset) are rejected. Methods `parts()` and `format()` live on the datetime value itself.

```keel
now      = Time.now()                          # UTC, millisecond precision
ny       = Time.now(tz: "America/New_York")    # offset-shifted RFC 3339
parsed   = Time.parse("2026-05-01T09:00:00Z") # datetime? — none if bad/no TZ
coerced  = Time.parse("2026-05-01", tz: "UTC") # naive + tz: → datetime?
ts       = Time.epoch_ms()                     # int — ms since Unix epoch

p = parsed.parts()   # {year, month, day, hour, minute, second, millisecond, tz}
s = parsed.format(as: "%Y-%m-%d")  # str? — none if receiver is not a datetime

elapsed  = finish - start   # datetime - datetime → duration
deadline = Time.now() + 3.days
ago      = Time.now() - 1.hour
```

### Factories (namespace)

| Call | Returns | Notes |
|---|---|---|
| `Time.now()` | `datetime` | Current UTC time, millisecond-precision RFC 3339 |
| `Time.now(tz: name)` | `datetime` | Offset-shifted; IANA name e.g. `"America/New_York"` |
| `Time.parse(str)` | `datetime?` | Accepts RFC 3339 with explicit TZ offset; returns `none` on failure |
| `Time.parse(str, tz: name)` | `datetime?` | Coerces a naive string into the given timezone |
| `Time.epoch_ms()` | `int` | Unix timestamp in milliseconds (suitable for JS interop, BIGINT columns, signed payloads) |

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
deadline = Time.now() + 7.days
ago      = Time.now() - 30.minutes

# datetime - datetime → duration
elapsed  = finish - start

# comparison
if deadline > Time.now() {
  Io.show("still time left")
}
```

---

## 14. Math

The `Math` namespace provides transcendental and power functions. All functions accept `int` or `float` arguments and always return `float`. The value-level methods `.abs()`, `.floor()`, `.ceil()`, `.round()` remain on the value itself (e.g. `(-3).abs()`) and are not duplicated here.

```keel
h   = Math.sqrt(Math.pow(3, 2) + Math.pow(4, 2))  # 5.0  (Pythagoras)
ln2 = Math.log(2)                                   # ≈ 0.693
deg = 45.0
rad = deg * Math.PI() / 180.0
s   = Math.sin(rad)                                 # ≈ 0.707
```

### Constants

| Call | Returns | Value |
|---|---|---|
| `Math.PI()` | `float` | π ≈ 3.14159265358979 |
| `Math.E()` | `float` | e ≈ 2.71828182845905 |

### Functions

| Call | Returns | Notes |
|---|---|---|
| `Math.sqrt(x)` | `float` | Square root; raises if `x < 0` |
| `Math.pow(x, y)` | `float` | `x` raised to the power `y` |
| `Math.exp(x)` | `float` | e^x |
| `Math.log(x)` | `float` | Natural logarithm (ln); raises if `x ≤ 0` |
| `Math.log2(x)` | `float` | Base-2 logarithm; raises if `x ≤ 0` |
| `Math.log10(x)` | `float` | Base-10 logarithm; raises if `x ≤ 0` |
| `Math.sin(x)` | `float` | Sine (radians) |
| `Math.cos(x)` | `float` | Cosine (radians) |
| `Math.tan(x)` | `float` | Tangent (radians) |
| `Math.asin(x)` | `float` | Arc-sine (radians); raises if `x ∉ [-1, 1]` |
| `Math.acos(x)` | `float` | Arc-cosine (radians); raises if `x ∉ [-1, 1]` |
| `Math.atan(x)` | `float` | Arc-tangent (radians) |
| `Math.atan2(y, x)` | `float` | `atan(y/x)` with correct quadrant; two positional args |

---

## 15. Csv

The `Csv` namespace parses and produces RFC 4180–compliant CSV text. It is always available — no `@tools` annotation required.

```keel
raw = "symbol,price,volume\nBTC,67000,1234.5\nETH,3500,5678.9"

# Raw parse — list[list[str]], first row is whatever the input contains
rows = Csv.parse(raw)         # [["symbol","price","volume"], ["BTC","67000","1234.5"], …]

# Header parse — list[map[str, str]], first row becomes map keys
trades = Csv.parse_records(raw)   # [{symbol: "BTC", price: "67000", …}, …]
for trade in trades {
    Log.info("{trade["symbol"]} @ {trade["price"] as float:.2f}")
}

# Stringify — list[list[str]] → CSV string (include a header row as the first inner list)
out = [["symbol", "price"], ["BTC", "67000"], ["ETH", "3500"]]
text = Csv.stringify(out)
```

### Functions

| Call | Returns | Notes |
|---|---|---|
| `Csv.parse(text: str)` | `list[list[str]]` | Parse CSV; every cell is a `str`. Raises `CsvError` on malformed input. |
| `Csv.parse_records(text: str)` | `list[map[str, str]]` | First row becomes header keys; remaining rows become maps. Returns `[]` when only a header row is present. |
| `Csv.stringify(rows: list[list[str]])` | `str` | Convert rows to CSV text. Each inner list is one row; every cell must be a `str`. Cells containing commas, quotes, or newlines are automatically quoted per RFC 4180. Raises `CsvError` if a row element is not a list or a cell is not a `str`. |

### Notes

- `Csv.stringify` only accepts `list[list[str]]`. To convert `list[map[str, str]]` to CSV, project the fields you want into lists first:
  ```keel
  lines = trades.map(t => [t["symbol"], t["price"]])
  text  = Csv.stringify([["symbol", "price"]] + lines)
  ```
- Empty input to `Csv.parse` returns `[]`.
- `Csv.parse_records` with only a header row (no data rows) returns `[]`.

---

## 16. Random, Uuid, and Crypto

### 15.1 `Random` — pseudo-random generation

`Random` produces non-cryptographic pseudo-random values. Use it for simulation, sampling, games, and any context where security is not a concern.

| Call | Returns | Notes |
|---|---|---|
| `Random.float()` | `float` | Uniform in `[0.0, 1.0)` |
| `Random.int(min:, max:)` | `int` | Inclusive range |
| `Random.bool()` | `bool` | 50/50 |

```keel
Random.float()              # 0.7341...
Random.int(min: 1, max: 6)  # dice roll
Random.bool()               # true or false
```

### 15.2 `Uuid` — UUID generation

`Uuid` is a distinct type (not `str`). It implements `Stringable` so it interpolates cleanly.

| Call | Returns | Notes |
|---|---|---|
| `uuid()` | `Uuid` | Prelude alias for `Uuid.v4()` |
| `Uuid.v4()` | `Uuid` | Random (CSPRNG) |
| `Uuid.v7()` | `Uuid` | Time-ordered — monotonically increasing, B-tree friendly |
| `Uuid.v5(ns:, name:)` | `Uuid` | Deterministic — SHA-1 of namespace + name |
| `Uuid.parse(s)` | `Uuid?` | `none` if invalid format |

**Namespace constants:** `Uuid.DNS`, `Uuid.URL`, `Uuid.OID`, `Uuid.X500` — for use with `Uuid.v5`.

**Value methods:**

| Method | Returns | Notes |
|---|---|---|
| `.version()` | `int` | 4, 7, or 5 |
| `.to_str()` | `str` | Hyphenated lowercase |
| `.format(as:)` | `str` | `"hyphenated"` (default), `"simple"` (no hyphens), `"urn"` |

```keel
id = uuid()                                        # Uuid v4
Log.info("created {id}")                           # interpolates via Stringable
Uuid.v7()                                          # time-ordered
Uuid.v5(ns: Uuid.DNS, name: "keel-lang.dev")       # deterministic
Uuid.parse("f47ac10b-58cc-4372-a567-0e02b2c3d479") # Uuid?
id.format(as: "simple")                            # "f47ac10b58cc4372a5670e02b2c3d479"
```

### 15.3 `Crypto` — cryptographic primitives

`Crypto` provides security-grade operations backed by a CSPRNG. It is **distinct from `Random`** — use `Crypto` wherever the output affects security (tokens, signatures, key derivation).

| Call | Returns | Notes |
|---|---|---|
| `Crypto.sha224(data)` | `str` | SHA-224 hex digest |
| `Crypto.sha256(data)` | `str` | SHA-256 hex digest |
| `Crypto.sha384(data)` | `str` | SHA-384 hex digest |
| `Crypto.sha512(data)` | `str` | SHA-512 hex digest |
| `Crypto.sha512_224(data)` | `str` | SHA-512/224 hex digest |
| `Crypto.sha512_256(data)` | `str` | SHA-512/256 hex digest |
| `Crypto.hmac_sha224(data, key:)` | `str` | HMAC-SHA-224 hex signature |
| `Crypto.hmac_sha256(data, key:)` | `str` | HMAC-SHA-256 hex signature |
| `Crypto.hmac_sha384(data, key:)` | `str` | HMAC-SHA-384 hex signature |
| `Crypto.hmac_sha512(data, key:)` | `str` | HMAC-SHA-512 hex signature |
| `Crypto.hmac_sha512_224(data, key:)` | `str` | HMAC-SHA-512/224 hex signature |
| `Crypto.hmac_sha512_256(data, key:)` | `str` | HMAC-SHA-512/256 hex signature |
| `Crypto.token(bytes: 32)` | `str` | Cryptographically secure random hex token |
| `Crypto.random_bytes(n)` | `list[int]` | `n` CSPRNG bytes |

```keel
Crypto.sha256("hello")                        # "2cf24db..."
Crypto.sha384("hello")
Crypto.hmac_sha256("msg", key: secret)
Crypto.token()                                # 64-char hex string (32 bytes)
Crypto.token(bytes: 16)                       # 32-char hex string
Crypto.random_bytes(16)                       # list[int] of 16 bytes
```

`Crypto` intentionally exposes fixed safe SHA-2 methods only. MD5, SHA-1, and string-selected hash algorithms are not available through `Crypto`.

---

## 17. Shell — Subprocess Bridge

`Shell` lets agents invoke external commands and capture their output. It is gated by `@tools [Shell]` — an agent must declare the capability before any `Shell.run` call is allowed.

### `Shell.run`

```
Shell.run(cmd: str, stdin: str? = none, cwd: str? = none) -> { stdout: str, stderr: str, exit_code: int }
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

- If `/bin/sh` cannot be spawned (e.g. missing in `PATH`), `Shell.run` **raises** at runtime.
- A non-zero exit code is **not** an error — it is returned in `exit_code`. The caller decides whether to raise.

```keel
agent Builder {
    @tools [Shell]

    @on_start {
        r = Shell.run("cargo test --quiet 2>&1")
        if r.exit_code != 0 {
            raise "build failed:\n{r.stdout}"
        }
        Io.show("Tests passed.")
    }
}
run(Builder)
```

**Capability gating:** `@tools` restricts an agent to the listed namespaces. If an agent declares `@tools [Io]` but not `Shell`, any `Shell.run` call raises `CapabilityError` at runtime. An agent with no `@tools` declaration is unrestricted. This gating is process-level, not OS-level — future releases may add stricter sandboxing.

**Environment isolation:** The subprocess runs with a clean environment. Only `PATH`, `HOME`, `SHELL`, `TMPDIR`, `USER`, and `LANG` are forwarded from the keel process. All other variables — including secrets, API keys, or credentials present in the keel process environment — are not visible to the shell command. To read the keel process environment from within a script, use `Env.*` instead.

**Security note:** `cmd` is passed directly to `/bin/sh -c`. Never interpolate untrusted user input into `cmd` without sanitisation.

---

## 18. Escape Hatches


### 17.1 `Ai.prompt` — raw LLM access

```keel
score = Ai.prompt(
  system: "Rate sentiment 1–10.",
  user: "Text: {review}",
  response_format: json
) as SentimentScore
# score: SentimentScore? — parsing/validation may fail
```

`Ai.prompt(...)` **must be followed by `as T`**. A bare `Ai.prompt(...)` that tries to use the result is a compile error. Use `as dynamic` to explicitly opt out of typing.

### 17.2 `Http.request` — raw HTTP

```keel
r = Http.request(
  method: POST,
  url: "https://api.example.com/v2",
  headers: {Authorization: "Bearer {Env.require("API_KEY")}"},
  body: {text: review},
  timeout: 10.seconds
)
# r: HttpResponse?
```

### 17.3 `Db.query` — raw SQL

```keel
rows = Db.query(
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
api_key = Env.require("OPENAI_API_KEY")   # fails at startup if missing
db_url  = Env.get("DATABASE_URL")          # str? — none if missing
```

`Env` is a prelude namespace backed by the host environment.

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

```keel
use "./email_utils.keel"               # import a local file
use Classifier from "./classifiers.keel"  # import a symbol
use community/crm                      # import a package
```

The prelude is always imported. `use` adds additional modules to scope.

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
| `dynamic` | any | Pass-through (runtime narrowing for `Ai.prompt`, `Json.parse`) |
| `none` | any | Raises |
| anything else | | Raises |

```keel
1 as float          # ok — float
1.7 as int          # ok — 1 (truncated)
"42" as int         # ok — 42
"3.14" as float     # ok — 3.14
"abc" as int        # raises: cannot cast "abc" to int
none as int         # raises: cannot cast none to int
```

---

## 22. Execution Model

Keel runs on the **Keel Runtime** (Rust, Tokio).

```
v0.1 (alpha):   .keel → Lexer → Parser → Typechecker → Interpreter
(later, TBD):   bytecode VM
(later, TBD):   LLVM AOT backend → native binary
```

### Runtime services

The runtime is intentionally small. It provides only what stdlib needs to exist on top of it:

1. **Event loop** (Tokio).
2. **Agent scheduler** — mailboxes, handler sequencing, structured cancellation.
3. **Timer primitives** — `sleep`, `deadline`. Stdlib `Schedule.*` is built on these.
4. **Interface dispatch** — registry of installed implementations per interface.
5. **Plugin ABI** — for `extern` and dynamically loaded stdlib backends.
6. **Tracer hook** — emits structured events at task/handler boundaries; stdlib `Log.*` subscribes.

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
| Missing `_` in non-enum `when` | Error |
| `self` outside an agent | Error |
| `Ai.prompt(...)` without `as T` | Error |
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
| `Ai.*` outside agent | LLM method called without `@role` / `@model` context | — |
| State written, never read | `self.x =` appears but `self.x` never used | — |

`keel lint --fix` auto-removes unused variable assignment lines.

---

## 25. IDE Contract

Every feature is designed for tooling.

| Context | Autocomplete |
|---|---|
| After `Ai.` | `classify`, `draft`, `summarize`, etc. |
| After `Ai.classify(x, as: ` | In-scope enum types |
| After `@` inside agent body | Registered attribute names |
| After `email.` | Fields of email's structural type |
| After `when urgency { ` | Variants of the enum, marking covered/uncovered |
| After `Agent.delegate(` | In-scope agent names |
| After `using: ` | Known model strings |

**Hover:** infers and displays types, signatures, attribute docs.

**Go-to-definition:** works through types, prelude namespaces, interface implementations.

**Refactoring:** rename is variant-aware and interface-aware.

---

## 26. Formal Grammar (PEG summary, condensed)

```peg
Program     <- (Decl / Stmt)* EOF
Decl        <- Agent / TaskDecl / TypeDecl / InterfaceDecl / ExternDecl / UseStmt

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
InterfaceDecl <- "interface" IDENT "{" (TaskSig)* "}"
TaskSig     <- "task" IDENT "(" Params? ")" ("->" Type)?

TypeDecl    <- "type" IDENT TypeParams? "=" EnumDef                      # enum
             / "type" IDENT TypeParams? "{" FieldDef* "}"                # struct
             / "type" IDENT TypeParams? "=" Type                         # alias

TypeParams  <- "[" IDENT ("," IDENT)* "]"                               # e.g. [T], [A, B]
EnumDef     <- EnumVariant ("|" EnumVariant)*
EnumVariant <- IDENT ("{" FieldDef* "}")?

ExternDecl  <- "extern" "task" IDENT "(" Params? ")" "->" Type "from" STRING
UseStmt     <- "use" STRING
             / "use" IDENT "from" STRING
             / "use" IDENT ("/" IDENT)+

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
# Note: `when` as an expression form is reserved for post-v0.1; today only the statement form is supported.
Pattern     <- VariantPat / StructPat / TuplePat / IDENT / "_" / Literal

# --- Statements ---
Stmt        <- ReturnStmt / RaiseStmt / AugAssignStmt / AugSelfAssign / AssignStmt / SelfAssign / ForStmt / TryStmt / ExprStmt
ReturnStmt  <- "return" Expr?
RaiseStmt   <- "raise" Expr
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

Notably absent: dedicated grammar for `ClassifyExpr`, `ExtractExpr`, `SummarizeExpr`, `DraftExpr`, `TranslateExpr`, `DecideExpr`, `PromptExpr`, `HttpExpr`, `SqlExpr`, `AskExpr`, `ConfirmExpr`, `NotifyStmt`, `ShowStmt`, `FetchExpr`, `SearchExpr`, `SendStmt`, `ArchiveStmt`, `RememberStmt`, `ForgetStmt`, `RecallExpr`, `EveryBlock`, `AfterStmt`, `WaitStmt`, `RetryStmt`, `ParallelExpr`, `RaceExpr`, `DelegateExpr`, `ConnectStmt`, `BroadcastStmt`, `RunStmt`, `RulesBlock`, `ConfigBlock`, `ToolsClause`, `TeamClause`, `RoleClause`, `ModelClause`, `MemoryClause`. All are ordinary function calls in the prelude or stdlib attribute handlers.

That keeps the parser small, type inference uniform (no hard-coded primitive signatures), and the IDE free of per-keyword special cases.

---

## 27. What's Next

v0.1 is the initial alpha. `keel run` accepts only the surface described in this document.

v0.2 and later are deliberately left **un-planned** until v0.1 ships and real usage reveals what to scope. See [ROADMAP.md](ROADMAP.md).

Breaking changes are expected between 0.x versions. Do not build production systems on v0.1. Play, prototype, break things, and send feedback.

---

*This specification is a living document. Syntax and semantics will evolve through alpha and beta phases.*
