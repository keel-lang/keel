# Types

> **Alpha (v0.1).** Breaking changes expected.

Keel is **statically typed with full inference as the design target**. In the current alpha, the checker catches core mismatches before your code runs and deliberately leaves some unsupported cases as `Unknown`; [ROADMAP.md](../../ROADMAP.md) tracks current checker coverage.

## Primitive types

| Type | Example | Notes |
|------|---------|-------|
| `int` | `42` | 64-bit integer |
| `float` | `3.14` | 64-bit float |
| `str` | `"hello"` | UTF-8, supports interpolation |
| `bool` | `true`, `false` | |
| `none` | `none` | Absence of value |
| `duration` | `5.minutes` | Time duration |

```keel
count = 42          # inferred as int
name = "Keel"       # inferred as str
ratio = 3.14        # inferred as float
active = true       # inferred as bool
```

## Enums

Enums define a closed set of variants. The compiler enforces exhaustive handling.

```keel
type Urgency = low | medium | high | critical

type Category = bug | feature | question | billing
```

Enum values are accessed by name:

```keel
u = high                    # Urgency.high
c = bug                     # Category.bug
label = Urgency.high        # explicit qualified access
```

## Generic types

Type declarations can be parameterised over one or more type variables.
The type checker substitutes the concrete arguments at each use site.

**Generic structs:**

```keel
type Paginated[T] {
  items: list[T]
  page: int
  has_more: bool
}

task show_page(p: Paginated[str]) {
  Io.show("{p.items.len()} item(s) on page {p.page}")
}
```

**Multi-parameter generics:**

```keel
type Pair[A, B] {
  first: A
  second: B
}

task t(p: Pair[str, int]) {
  a: str = p.first    # type-checked as str
  b: int = p.second   # type-checked as int
}
```

**Generic aliases:**

```keel
type Bag[T] = list[T]

task t(tags: Bag[str]) {
  n: int = tags.len()
}
```

**Generic enums:**

```keel
type Pair[A, B] =
  | both { first: A, second: B }
  | only_first { value: A }
  | only_second { value: B }
```

Variant names are registered for exhaustiveness checking. When you destructure a
variant binding, the field type is resolved using the substituted type arguments:

```keel
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

## Structs

Structs are structural types — any value with matching fields satisfies the type.

```keel
type EmailInfo {
  sender: str
  subject: str
  body: str
  unread: bool
}

# Inline struct types in parameters
task triage(email: {body: str, from: str}) -> Urgency {
  classify email.body as Urgency
}
```

You don't need to declare a struct to use it:

```keel
info = {name: "Alice", age: 30}   # inferred as {name: str, age: int}
notify user info.name              # "Alice"
```

When a struct literal is assigned to a named struct type, the checker
verifies every required field is present. Extra fields are allowed
(structural subtyping):

```keel
type Person { name: str, age: int }

task t() {
  p: Person = { name: "Alice" }                        # error: missing field `age`
  q: Person = { name: "Bob", age: 30 }                 # ok
  r: Person = { name: "Eve", age: 25, extra: true }    # ok — extras allowed
}
```

### Spread-update

To create a modified copy of a struct (or map) without repeating every field, use the `{ ...base, field: new }` syntax:

```keel
type Order { id: str, status: str, amount: float }

o: Order = { id: "ord-1", status: "pending", amount: 9.99 }
filled   = { ...o, status: "filled" }   # id and amount copied unchanged
copy     = { ...o }                     # full copy, no overrides
```

Rules:
- The `...base` spread must appear **first**, exactly once.
- Zero or more `field: value` overrides follow, separated by commas or newlines.
- Spreading a `none` value raises at runtime.

**Struct base** — override field names must exist in the base struct; unknown fields are a compile-time error (and a runtime error on dynamic paths). The result preserves the base's type tag so `impl` dispatch continues to work.

**Map base** — any key may be added or overridden freely (like Python's `{**d, "k": v}`); override values must match the map's declared value type. The result is the same `map[K, V]` type.

```keel
m: map[str, int] = { "a": 1, "b": 2 }
m2 = { ...m, "c": 3 }   # adds key "c"; result is still map[str, int]
```

Spread-update is especially useful when updating one field of a struct or building configuration variants:

```keel
type Config { host: str, port: int, debug: bool }

base: Config = { host: "localhost", port: 8080, debug: false }
dev  = { ...base, debug: true }
prod = { ...dev, host: "api.example.com", debug: false }
```

## Nullable types

Types are **non-nullable by default**. Append `?` to allow `none`:

```keel
name: str       # cannot be none
alias: str?     # can be none

# Null-safe access
subject = email?.subject           # str? — none if email is none

# Null coalescing
subject = email?.subject ?? "(no subject)"   # str — guaranteed non-none
```

The checker enforces the `?` boundary at every assignment, return, and
call site. Passing a nullable where a non-nullable is expected is a
compile-time error — use `!` to assert non-null (raises `NullError` at
runtime if the value is `none`) or `??` to coalesce to a default.

```keel
task t() {
  x: str = Env.get("KEY")          # error: expected str, got str?
  y: str = Env.get("KEY")!         # ok — raises NullError if missing
  z: str = Env.get("KEY") ?? ""    # ok — falls back to ""
}
```

Call sites are also checked — a nullable argument where a non-nullable
parameter is declared is a type error:

```keel
task process(text: str) { ... }

task t() {
  val: str? = Env.get("PROMPT")
  process(val)          # error: task `process` arg `text`: expected str, got str?
  process(val!)         # ok
  process(val ?? "")    # ok
}
```

AI operations return nullable types when they can fail:

```keel
result = Ai.classify(text, as: Urgency)   # Urgency? — might be none
safe = result ?? Urgency.medium            # Urgency — guaranteed

# Or supply the default inline:
safe = Ai.classify(text, as: Urgency) ?? Urgency.medium   # Urgency
```

## Collections

```keel
nums = [1, 2, 3]                         # list[int]
names = ["alice", "bob"]                  # list[str]
info = {name: "Zied", role: "builder"}   # map[str, str]
```

**Map key types.** The key type `K` in `map[K, V]` must be a hashable primitive: `str`, `int`, or `bool`. Other types are compile-time errors:

```keel
scores:  map[str,  int]  = {alice: 100, bob: 95}   # valid
lookup:  map[int,  str]  = {1: "one", 2: "two"}    # valid
flags:   map[bool, str]  = {true: "on"}            # valid

# bad: map[float, str]   — float is not hashable (NaN)
# bad: map[str?,  int]   — nullable key type
# bad: map[Point, str]   — struct keys require interface Hashable (v0.2)
```

**Subscript access** (`list[i]`): integer index, returns `T`. Out-of-bounds and negative indices are runtime errors — use `len()` to guard or `try/catch` when the index may be invalid:

```keel
items = [10, 20, 30]
v = items[1]   # int — 20
# items[99]    # runtime error: index 99 out of bounds
```

String subscript (`str[i]`) returns a single-character `str` by the same rules.

**List properties:**

| Property | Returns | Description |
|----------|---------|-------------|
| `.count` | `int` | Number of elements |
| `.first` | `T?` | First element or none |
| `.last` | `T?` | Last element or none |
| `.is_empty` | `bool` | True if count == 0 |

## Function types

Function types describe callable values. Write the parameter types in parentheses followed by `->` and the return type:

```keel
type Handler      = (str) -> bool
type Reducer      = (str, int) -> str
type Thunk        = () -> none
type Predicate[T] = (T) -> bool   # generic function type

task t(pred: Predicate[str]) {
  ok: bool = pred("hello")
}
```

Tuples and function types share the `(...)` syntax — if `->` follows the closing paren it is a function type; otherwise it is a tuple.

## Type conversions

```keel
port_str = "8080"
port = port_str.to_int() ?? 3000     # int — 8080 or default
ratio = 3.to_float() / 4.to_float()  # float — 0.75
label = Urgency.high.to_str()        # str — "high"
```

Conversions that can fail return nullable types (`str.to_int()` → `int?`). Conversions that always succeed return non-nullable (`int.to_str()` → `str`).

## Numeric value methods

`int` and `float` values expose four built-in methods. The return type always matches the receiver — calling a method on an `int` returns an `int`, and calling it on a `float` returns a `float`.

| Method | Returns | Notes |
|---|---|---|
| `.abs()` | same type | Absolute value |
| `.floor()` | same type | Round toward −∞; no-op on `int` |
| `.ceil()` | same type | Round toward +∞; no-op on `int` |
| `.round()` | same type | Round to nearest; no-op on `int` |

```keel
price = -3.75
price.abs()           # 3.75
price.abs().ceil()    # 4.0  — methods chain naturally
count = -5
count.abs()           # 5    — int stays int
3.7.floor()           # 3.0
3.2.ceil()            # 4.0
3.5.round()           # 4.0
```

## Duration literals

```keel
5.seconds    30.minutes    2.hours    1.day    7.days

# Short forms
30.sec       1.min         2.hr       1.d
```

## Type coercions — `as T`

`expr as T` coerces the value at runtime. Unsupported conversions raise a runtime error.

| From | To | Result |
|---|---|---|
| `int` | `float` | Widens: `5 as float` → `5.0` |
| `float` | `int` | Truncates toward zero: `1.9 as int` → `1` |
| `int` / `float` / `bool` | `str` | Display string: `42 as str` → `"42"` |
| `str` | `int` | Parses; raises if not a valid integer |
| `str` | `float` | Parses; raises if not a valid float |
| `str` | `bool` | `"true"` → `true`, `"false"` → `false`; raises otherwise |
| `Uuid` | `str` | Hyphenated string: `"f47ac10b-..."` |
| `str` | `Uuid` | Validates UUID format; raises if invalid |
| `dynamic` | any | Pass-through — used with `Ai.prompt(...) as T` and `Json.parse` |
| `none` | any | Raises |

```keel
1 as float          # 1.0
1.7 as int          # 1  (truncated, not rounded)
-1.7 as int         # -1
42 as str           # "42"
"3.14" as float     # 3.14
"99" as int         # 99

"abc" as int        # raises: cannot cast "abc" to int
none as int         # raises: cannot cast none to int
```

## `typeof(x)`

The prelude function `typeof(x)` returns the runtime type name as a `str`. For struct and enum values it returns the declared type name, not the generic `"struct"` or `"enum"` tag.

```keel
typeof(42)          # "int"
typeof(3.14)        # "float"
typeof("hello")     # "str"
typeof(true)        # "bool"
typeof(none)        # "none"
typeof([1, 2, 3])   # "list"

type Point { x: int, y: int }
p: Point = { x: 1, y: 2 }
typeof(p)           # "Point"

type Color = red | green | blue
c: Color = Color.red
typeof(c)           # "Color"
```
