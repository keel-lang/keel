# Interfaces

An `interface` declares a set of required methods. Any struct type can satisfy an interface by providing an `impl` block. The compiler validates that every required method is present, that arities match, and that return types match.

## Declaring an interface

```keel
interface Printable {
  task print(self) -> str
}

interface Converter {
  task to_json(self) -> str
  task to_csv(self) -> str
}
```

Each entry is a `task` signature. The `self` parameter is written without a type annotation — the type is inferred from the `for TypeName` clause of each `impl` block.

## Implementing an interface

Use `impl InterfaceName for TypeName { ... }` to provide an implementation:

```keel
use std/io

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

- `impl` and `for` are reserved keywords.
- `self` inside the block receives the struct value. Use `self.field` to access fields.
- Each method listed in the interface must be provided — missing methods are a compile-time error.
- Extra methods not listed in the interface are a compile-time error.
- Parameter count and parameter types must match the interface signature. `dynamic` in the interface's parameter position is a wildcard — an `impl` may narrow it to any concrete type (this is how `Comparable` declares `other: dynamic` while an `impl` writes `other: Score`). A concrete parameter type requires an exact match.
- Return types must match exactly (`dynamic` in the return position is likewise a wildcard).
- Interfaces and their `impl` blocks can appear in any order in the same file.
- Method **bodies** are type-checked like any other task body — see below.

## Inside a method body

An `impl` method body is checked exactly like a top-level task: annotations, return values, field access, arity, and enum exhaustiveness are all validated by `keel check`.

```keel
interface Shape {
  task area(self) -> float
  task grown(self) -> str
}

type Rect { w: float, h: float }

task scale(r: Rect, factor: float) -> Rect {
  { w: r.w * factor, h: r.h * factor }
}

impl Shape for Rect {
  task area(self) -> float {
    self.w * self.h          # fields keep their declared types
  }

  task grown(self) -> str {
    bigger = scale(self, 2.0)   # `self` is a value of the implementing type
    "{bigger.w}x{bigger.h}"
  }
}
```

These rules follow from `self` being the receiver **value**, not an agent:

- `self.field` resolves to the field's declared type; naming a field the type doesn't declare is a compile-time error.
- `self` can be passed anywhere a value of the implementing type is expected.
- `self.field = value` is a compile-time error — the receiver is passed by value, so the write could never outlive the call. Return an updated value instead.
- `self.method(...)` is a compile-time error. `self.method(...)` is agent syntax and always dispatches to the enclosing *agent's* task, never to another `impl` method. Move shared logic into a top-level task and call it as `method(self)`.

## Multiple types, one interface

Each type gets its own `impl` block:

```keel
interface Printable {
  task print(self) -> str
}

type Circle { radius: float }
type Rect   { width: float, height: float }

impl Printable for Circle {
  task print(self) -> str { "Circle(r={self.radius})" }
}

impl Printable for Rect {
  task print(self) -> str { "Rect({self.width}×{self.height})" }
}
```

## Conformance errors

```keel
interface Scorer {
  task score(self, weight: int) -> int
}

type Game { pts: int }

impl Scorer for Game {
  task score(self, weight: str) -> str {   # two errors: param `weight` must be
    "oops"                                  # int not str, and must return int not str
  }
  task extra(self) -> int { 0 }            # error: not part of interface Scorer
}
```

Each of these is a separate compile-time error. The runtime will not run a program with a non-conforming `impl`.

## Method priority

Impl methods take priority over built-in map methods. If a struct type has an interface method named `size`, it shadows the built-in `map.size()` length accessor for that type:

```keel
use std/io

interface Sizable {
  task size(self) -> int
}

type Grid { rows: int, cols: int }

impl Sizable for Grid {
  task size(self) -> int { self.rows * self.cols }
}

g: Grid = { rows: 3, cols: 4 }
io.show("{g.size()}")    # → "12"  (not the map key count)
```

## Built-in interfaces

Five interfaces are built into the language. You cannot redeclare them with `interface`, but you can provide `impl` blocks for them.

### `Stringable`

Enables `"{expr}"` string interpolation for user-defined types. See the [String Interpolation guide](strings.md#stringable-interface--custom-interpolation) for full details.

```keel
use std/io

impl Stringable for Point {
  task to_str(self) -> str { "({self.x}, {self.y})" }
}

p: Point = { x: 3.0, y: 4.0 }
io.show("Point: {p}")    # → "Point: (3.0, 4.0)"
```

### `Serializable`

Overrides `json.stringify` for user-defined types. Implement `to_json` to produce a custom JSON representation.

```keel
use std/io
use std/json

type Event { name: str, score: int }

impl Serializable for Event {
  task to_json(self) -> str {
    "name={self.name};score={self.score}"
  }
}

e: Event = { name: "goal", score: 3 }
io.show(json.stringify(e))   # → calls to_json instead of default serializer
```

### `Comparable`

Enables sorting and comparison for user-defined types. `compare(self, other)` must return:
- a negative integer when `self < other`
- zero when `self == other`
- a positive integer when `self > other`

Wired into `list.sort()`, `list.min()`, `list.max()`, and the global `min()` / `max()`.

```keel
use std/io

type Score { val: int }

impl Comparable for Score {
  task compare(self, other: Score) -> int {
    self.val - other.val
  }
}

# Declare the list type so each element is tagged as Score.
# Without the annotation, elements stay as anonymous maps and
# Comparable.compare is never found.
items: list[Score] = [{ val: 30 }, { val: 10 }, { val: 20 }]
io.show("{items.sort()}")   # sorted ascending by val
lo = items.min()
hi = items.max()
```

### `Equatable`

Adds a typed equality method. Note: `==` remains structural (field-by-field) comparison — `equals` is a separate, potentially richer check.

```keel
use std/io

type Point { x: int, y: int }

impl Equatable for Point {
  task equals(self, other: Point) -> bool {
    self.x == other.x and self.y == other.y
  }
}

a: Point = { x: 1, y: 2 }
b: Point = { x: 1, y: 2 }
io.show("{a.equals(b)}")   # → true
```

### `Iterable`

Lets a struct be used in a `for` loop. `items(self)` must return a list of the elements to iterate. The return type may be `list[T]` for any concrete `T` — `list[dynamic]` is accepted but so is `list[int]`, `list[str]`, etc.

```keel
use std/io

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

r: Range = { lo: 1, hi: 4 }
for n in r {
  io.show("{n}")   # → 1, 2, 3, 4
}
```

> **Note:** `Iterable` is not a generator protocol — `items()` materialises the entire collection into a list before iteration begins.

## What's not supported yet

- **Interface as a type annotation** — you cannot write `task format(x: Printable) -> str`. Values are structurally typed; interface names cannot appear in parameter or return type positions. This is deferred post-v0.1.
- **Generic interfaces** — `interface Container[T] { task item(self) -> T }` is not yet parsed. Deferred post-v0.1.
