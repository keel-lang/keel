# Types

Keel is **statically typed with full inference as the design target**. In the current alpha, the checker catches core mismatches before your code runs. Where the type cannot be determined statically, the checker uses one of three internal fall-back markers — `Unknown(InferenceLimitation)`, `Unknown(ExternalDynamic)`, or `Unknown(UnsupportedFeature)` — which are distinct from the programmer-written `dynamic` annotation and are surfaced by `keel check --strict`.

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
use std/io

type Paginated[T] {
  items: list[T]
  page: int
  has_more: bool
}

task show_page(p: Paginated[str]) {
  io.show("{p.items.len()} item(s) on page {p.page}")
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
use std/io

task t(p: Pair[str, int]) {
  when p {
    both { first, second } => {
      f: str = first    # type-checked as str
      s: int = second   # type-checked as int
    }
    only_first { value } => { io.show(value) }
    only_second { value } => { io.show("{value}") }
  }
}
```

Each destructured name must be a field the variant declares. Naming a field the
variant does not have (a typo, say) is a compile-time error rather than a silent
`none` binding.

## Structs

Each declared struct type has a unique identity. Two types with the same fields
are distinct types and are not interchangeable:

```text
type Point  { x: int, y: int }
type Offset { x: int, y: int }

task move(p: Point) -> Point { p }

o: Offset = { x: 1, y: 2 }
move(o)                         # error — Offset is not assignable to Point
move({ x: 1, y: 2 })           # ok — anonymous literal is compatible with Point
```

Because fields are matched to the target type by **name**, a literal may list
them in any order. The expressions themselves are still evaluated left to right
in the order written — which matters as soon as a field expression has a side
effect:

```keel
use std/io

type P { x: int, y: int }

task note(s: str) -> int {
  io.show(s)
  return 1
}

p: P = { y: note("first"), x: note("second") }   # prints "first", then "second"
```

An untyped struct literal `{ x: 1, y: 2 }` is an anonymous shape. It is
structurally compatible with any named struct type that has the required fields.
Assign to a typed variable or pass to a typed parameter to tag it:

```keel
use std/email

type EmailInfo {
  sender: str
  subject: str
  body: str
  unread: bool
}

# Inline struct types in parameters
task triage(msg: {body: str, from: str}) -> Urgency {
  ai.classify(msg.body, as: Urgency) ?? Urgency.low
}
```

The checker verifies every required field is present. Extra fields on anonymous
literals are allowed:

```keel
type Person { name: str, age: int }

task t() {
  p: Person = { name: "Alice" }                        # error: missing field `age`
  q: Person = { name: "Bob", age: 30 }                 # ok
  r: Person = { name: "Eve", age: 25, extra: true }    # ok — extras allowed
}
```

### Impl dispatch and typed collections

`impl` methods are dispatched by the value's type tag. A struct literal only
acquires its tag at the first typed boundary it crosses. For a list of struct
values, declare the list type so each element is promoted:

```keel
type Score { val: int }
impl Comparable for Score {
  task compare(self, other: Score) -> int { self.val - other.val }
}

task run() {
  # Without the annotation, elements stay as untagged maps and .sort() falls
  # back to primitive ordering instead of using Comparable.compare.
  scores: list[Score] = [{ val: 30 }, { val: 10 }, { val: 20 }]
  sorted = scores.sort()   # [10, 20, 30] — uses Comparable.compare
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
- The base is evaluated once, first; then each override in the order written.

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

```text
name: str       # field cannot be none
alias: str?     # field can be none
```

```keel
type Email { subject: str, from: str }
task t(email: Email?) {
  # Null-safe access
  subject = email?.subject           # str? — none if email is none

  # Null coalescing
  subject = email?.subject ?? "(no subject)"   # str — guaranteed non-none
}
```

The checker enforces the `?` boundary at every assignment, return, and
call site. Passing a nullable where a non-nullable is expected is a
compile-time error — use `!` to assert non-null (raises a plain `Error`
at runtime if the value is `none`) or `??` to coalesce to a default.

```keel
use std/env

task t() {
  x: str = env.get("KEY")          # error: expected str, got str?
  y: str = env.get("KEY")!         # ok — raises Error if missing
  z: str = env.get("KEY") ?? ""    # ok — falls back to ""
}
```

Call sites are also checked — a nullable argument where a non-nullable
parameter is declared is a type error:

```text
task process(text: str) { ... }  # expects non-nullable str

val: str? = env.get("PROMPT")
process(val)          # error: task `process` arg `text`: expected str, got str?
process(val!)         # ok — null-assertion (raises if none)
process(val ?? "")    # ok — null coalescing
```

AI operations return nullable types when they can fail:

```keel
result = ai.classify(text, as: Urgency)   # Urgency? — might be none
safe = result ?? Urgency.medium            # Urgency — guaranteed

# Or supply the default inline:
safe = ai.classify(text, as: Urgency) ?? Urgency.medium   # Urgency
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

## Tuples

A tuple is a fixed-length, ordered group of values whose types can differ. Write the type as parenthesised element types:

```keel
pair: (str, int) = ("hello", 42)
```

Tuples are structural and immutable — two tuples with the same element types are the same type, and you build a new one rather than modifying an existing one. There is no single-element tuple: `(str)` is just `str`. The empty tuple `()` is `none`.

Read elements either by destructuring or by position:

```keel
use std/io

pair: (str, int) = ("hello", 42)

(name, count) = pair          # destructure into two bindings
io.show(name)                 # hello

x = pair.0                    # positional access — str
io.show("{x} {pair.1}")       # hello 42
```

Positional access is checked at compile time. The index must be within the tuple's arity, and `.N` is only valid on tuples:

```keel
pair: (str, int) = ("hello", 42)

# pair.5      # error: tuple index 5 is out of bounds for `(str, int)` (arity 2)
# pair.name   # error: `(str, int)` has no field `name` — read by position
```

Lists and maps use subscripts instead, so `.0` on a list is an error that points you at the right syntax:

```keel
xs: list[int] = [1, 2, 3]

y = xs[0]     # 1 — correct
# y = xs.0    # error: positional access `.0` is only valid on tuples;
              #        index `list[int]` with `[0]`
```

`?.` works on a nullable tuple, yielding a nullable element:

```keel
maybe: (str, int)? = ("hi", 7)
io.show("{maybe?.0}")         # hi
```

Nested positional access needs parentheses:

```keel
t: ((int, int), int) = ((1, 2), 3)

b = (t.0).1                   # 2
# b = t.0.1                   # parse error
```

This is a lexer limitation, not a language rule — `0.1` matches the float-literal pattern and is claimed as a single token before the grammar sees two separate indices. Tracked in [issue #185](https://github.com/keel-lang/keel/issues/185). The parenthesised form works and will keep working.

## Function types

Function types describe callable values. Write the parameter types in parentheses followed by `->` and the return type:

```keel
type Handler      = (str) -> bool
type Reducer      = (str, int) -> str
type Predicate[T] = (T) -> bool   # generic function type

task t(pred: Predicate[str]) {
  ok: bool = pred("hello")
}
```

> `() -> none` is not valid syntax — a function type's return must be a named type. Omit the return annotation on tasks to indicate "returns nothing".

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
a = 5.seconds
b = 30.minutes
c = 2.hours
d = 1.day
e = 7.days

# Short forms also work
f = 30.sec
g = 1.min
h = 2.hr
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
| `list` | `list[T]` | Asserts the value is a list, then recurses the element cast to `T`; raises on a non-list or any element that cannot cast |
| `map` | `map[K, V]` | Asserts the value is a map, then recurses the value cast to `V`; keys pass through unchanged; raises on a non-map |
| `list` | `(T1, T2, …)` | Tuple target: asserts a list of the same arity, then casts each element position-wise; raises on a non-list, arity mismatch, or any element that cannot cast |
| `dynamic` | any | Narrowed at runtime by the rules above — the value must fit, else it raises. This is how `ai.prompt(...) as T` and `json.parse(...) as list[dynamic]` results are narrowed |
| `none` | any | Raises |

A `dynamic` element/value type (`list[dynamic]`, `map[str, dynamic]`) is a
pass-through — the inner elements are not re-checked — so a container cast only
asserts the **top-level** shape. The shape is always checked:
`json.parse("42") as list[dynamic]` raises because the value is an integer.

```keel
1 as float          # 1.0
1.7 as int          # 1  (truncated, not rounded)
-1.7 as int         # -1
42 as str           # "42"
"3.14" as float     # 3.14
"99" as int         # 99

json.parse("[1,2,3]")     as list[dynamic]     # [1, 2, 3]
json.parse("[1,2,3]")     as list[int]         # recurses each element
json.parse("{\"a\":1}")   as map[str, dynamic] # {a: 1}
json.parse("[1,2]")       as (int, int)        # tuple (1, 2)

"abc" as int        # raises: cannot cast "abc" to int
none as int         # raises: cannot cast none to int
json.parse("42") as list[dynamic]   # raises: cannot cast int to list
```

> A tuple type is written with parentheses — `(int, int)`. There is no
> `tuple[...]` bracket form: `as tuple[int, int]` (or a wrong-arity `list[int,
> int]`) parses as a generic type and is not a valid cast target.

## `typeof(x)`

The built-in function `typeof(x)` — always in scope, no import needed — returns the runtime type name as a `str`. For struct and enum values it returns the declared type name, not the generic `"struct"` or `"enum"` tag.

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

## Type-mismatch diagnostics

When a `let` binding has an explicit type annotation and the assigned value does
not match, `keel check` underlines the **annotation** — not the whole statement —
so you can see at a glance which declared type is wrong:

```
error: `n`: expected int, got str
  --> example.keel:2:5
   |
 2 |   n: int = "hello"
   |      ^^^  — expected int here
```

This precision is available for all scalar, struct, and nullable mismatches on
annotated `let` bindings.
