# Collections & Lambdas

## Range operator `..`

`start..end` produces an **inclusive** `list[int]` containing every integer from `start` to `end`.

```keel
digits = 1..5        # [1, 2, 3, 4, 5]
single = 4..4        # [4]
empty  = 5..3        # []  (start > end → empty)
```

Both operands must be `int` — using a `float` or `str` is a type error.

Ranges work directly with `for`:

```keel
for i in 0..2 {
  io.notify("{i}")   # 0, 1, 2
}
```

Because the result is a plain `list[int]`, all list methods apply:

```keel
count  = (1..10).count()                  # 10
evens  = (1..10).filter(n => n % 2 == 0) # [2, 4, 6, 8, 10]
```

## Lists

```keel
nums = [1, 2, 3, 4, 5]
names = ["alice", "bob", "charlie"]
empty = []
```

### List concatenation and adding elements

Concatenate lists with `+` or add a single element with `push`:

```keel
base = ["a", "b"]
extra = ["c", "d"]
all = base + extra              # ["a", "b", "c", "d"]

more = all.push("e")            # ["a", "b", "c", "d", "e"]
```

`push` returns a new list — it does not mutate in place. Reassign if needed: `items = items.push(x)`.

## Collection methods

All methods accept lambdas (`x => expr`) or task references.

> **Lambdas don't capture outer variables.** A lambda body can only see its
> own parameters and global names (tasks, agents, `self` inside an agent). It
> cannot read a local declared outside it — `n = 10; xs.map(x => x + n)` is a
> check error. Pass the value in another way instead, e.g. as a second
> parameter via a named task: `xs.map(add_n)` where `add_n(x: int) -> int`
> closes over nothing and takes everything it needs as arguments. See
> [SPEC §7](https://github.com/keel-lang/keel/blob/main/SPEC.md#7-lambdas-and-first-class-functions).

### map — transform each element

```keel
doubled = [1, 2, 3].map(n => n * 2)         # [2, 4, 6]
names = contacts.map(c => c.name)            # extract names
```

### filter — keep matching elements

```keel
big = [1, 5, 10, 20].filter(n => n > 5)     # [10, 20]
urgent = emails.filter(e => e.urgency == high)
```

### find — first matching element

```keel
found = items.find(n => n > 15)              # first match or none
admin = users.find(u => u.role == "admin") ?? default_user
```

### any / all — boolean checks

```keel
has_urgent = emails.any(e => e.urgency == critical)    # true/false
all_done = tasks.all(t => t.status == "complete")
```

### reduce — fold to a single value

The first argument is the combining function `(accumulator, element) => ...`; the second is the initial accumulator value.

```keel
total = [1, 2, 3, 4, 5].reduce((acc, x) => acc + x, 0)   # 15
joined = words.reduce((acc, w) => acc + " " + w, "")
```

### sum / min / max — numeric aggregation

```keel
prices = [9.99, 4.50, 14.00]
prices.sum()   # 28.49
prices.min()   # 4.50
prices.max()   # 14.00
```

`min()` and `max()` return `none` on an empty list. `sum()` requires a numeric list; calling it on non-numeric elements is a runtime error.

### join — concatenate to string

```keel
["a", "b", "c"].join(", ")   # "a, b, c"
tags.join(" | ")
```

### sort — natural ordering

Returns a new sorted list. Integers, floats, and strings are compared by value. Structs with `impl Comparable` are sorted by their `compare` method.

```keel
[3, 1, 4, 1, 5].sort()            # [1, 1, 3, 4, 5]
["cherry", "apple", "banana"].sort()  # ["apple", "banana", "cherry"]
```

### sort(by:) — sort by key function

Pass an optional `by:` key function to `.sort()` — consistent with the `min(by:)` / `max(by:)` pattern. The key must return an `int`, `float`, or `str`. Ascending only; negate numeric keys for descending.

```keel
type Product { name: str, price: float, rating: float }

# Sort by a single field
by_price  = products.sort(by: p => p.price)          # cheapest first
by_name   = products.sort(by: p => p.name)           # alphabetical

# Descending — negate the key
by_rating = products.sort(by: p => 0.0 - p.rating)  # highest rated first
```

### reverse — flip order

```keel
[1, 2, 3].reverse()    # [3, 2, 1]
top3 = scores.sort().reverse().take(3)
```

### flatten — unwrap one level of nesting

```keel
[[1, 2], [3], [4, 5]].flatten()   # [1, 2, 3, 4, 5]
```

### zip — pair two lists

Returns a new list where each element is a 2-element tuple `[a, b]` drawn from the two input
lists in order. Stops when the shorter list is exhausted.

```keel
names  = ["alice", "bob", "carol"]
scores = [90, 85, 95]

pairs = names.zip(scores)
# → [["alice", 90], ["bob", 85], ["carol", 95]]

for (name, score) in names.zip(scores) {
    log.info("{name} scored {score}")
}
```

The return type is inferred as `list[(T, U)]`, so the loop variables `name` and `score` are
fully typed.

### take / skip — slice by count

```keel
[10, 20, 30, 40, 50].take(3)   # [10, 20, 30]
[10, 20, 30, 40, 50].skip(3)   # [40, 50]
```

## Lambda syntax

```keel
# Single parameter
n => n * 2

# Multi-parameter
(a, b) => a + b

# Block body
items.map(item => {
  urgency = triage(item)
  {item: item, urgency: urgency}
})
```

## Task references

Named tasks can be passed directly to collection methods:

```keel
task double(n: int) -> int { n * 2 }
task is_even(n: int) -> bool { n % 2 == 0 }

result = [1, 2, 3].map(double)        # [2, 4, 6]
evens = [1, 2, 3, 4].filter(is_even)  # [2, 4]
```

## Map / struct access

```keel
info = {name: "Alice", age: 30}
info.name                              # "Alice"
info.age                               # 30

# Nested
team = {lead: {name: "Bob", role: "eng"}}
team.lead.name                         # "Bob"
```

## Maps

A typed `map[K, V]` literal is built with the same `{k: v, ...}` form.
The same syntax serves both struct construction and `map[str, V]`
construction; the checker resolves which based on the declared type.

```keel
stock: map[str, int] = {apples: 12, pears: 5, plums: 0}
```

### Map methods

| Method | Returns |
|---|---|
| `map.get(k)` | `V?` — `none` if the key is absent |
| `map.keys()` | `list[K]` |
| `map.values()` | `list[V]` |
| `map.len()` / `map.count()` / `map.size()` | `int` |
| `map.is_empty()` | `bool` |
| `map.contains(k)` / `map.has(k)` | `bool` |
| `map.insert(k, v)` | `map[K, V]` — a **new** map |

```keel
stock: map[str, int] = {apples: 12, pears: 5}

stock.count()              # 2
stock.contains("apples")   # true
stock.keys()               # ["apples", "pears"]
stock.values()             # [12, 5]

# get returns V? — coalesce or assert before using as V.
pears = stock.get("pears") ?? 0      # 5
missing = stock.get("oranges") ?? 0  # 0
```

### insert — add or overwrite a key

Like `list.push`, `insert` returns a new map instead of modifying the
receiver, so the result has to be bound:

```keel
stock: map[str, int] = {apples: 12}

stock.insert("pears", 5)            # no-op — the result was discarded
stock = stock.insert("pears", 5)    # {apples: 12, pears: 5}
stock = stock.insert("apples", 20)  # overwrites: {apples: 20, pears: 5}
```

## Sets

A `set[T]` holds at most one element equal to any given value. Duplicates
collapse on the way in, whether from the literal or from `add`:

```keel
ids: set[int] = set[1, 2, 2, 3]
ids.count()    # 3 — the second 2 was dropped
```

Elements keep **first-insertion order** — that is the order a set prints,
iterates, and spreads in. Nothing is sorted.

```keel
set[3, 1, 2]   # set[3, 1, 2]
```

Equality is the same `==` used everywhere else, so the element type is
unrestricted — unlike map keys, which must be `str`, `int`, or `bool`:

```keel
type Point { x: int, y: int }

a: Point = {x: 1, y: 2}
b: Point = {x: 1, y: 2}
set[a, b].count()   # 1 — equal field values, one element
```

### Set methods

| Method | Returns |
|---|---|
| `set.add(v)` | `set[T]` — a **new** set |
| `set.contains(v)` | `bool` |
| `set.len()` / `set.count()` / `set.size()` | `int` |
| `set.is_empty()` | `bool` |

`add` follows the same value-method rule as `list.push` and `map.insert` —
it returns a new set, and adding an element that is already present is a
no-op rather than an error:

```keel
ids = set[1, 2]

ids.add(3)          # no-op — the result was discarded
ids = ids.add(3)    # set[1, 2, 3]
ids = ids.add(2)    # set[1, 2, 3] — already present
```

Sets iterate and spread like lists:

```keel
for id in set[3, 1, 2] {
  io.show(id)       # 3, then 1, then 2
}

total(...set[1, 2, 2, 3])   # spreads 3 values, not 4
```

They also accept the read-only list methods — `map`, `filter`, `find`,
`any`, `all`, `reduce`, `sum`, `min`, `max`, `join`, `sort`, `reverse`,
`take`, `skip`, and friends — each returning a list or a scalar, never a
set. `push` is not among them; use `add`.

```keel
set[1, 2, 2, 3].map(n => n * 2)   # [2, 4, 6] — a list
set[1, 2, 3].join("-")            # "1-2-3"
```

## String methods

```keel
s = "  Hello, World!  "
s.trim()                  # "Hello, World!"
s.upper()                 # "  HELLO, WORLD!  "
s.lower()                 # "  hello, world!  "
s.contains("Hello")       # true
s.starts_with("  H")      # true
s.split(", ")             # ["  Hello", "World!  "]
s.replace("World", "Keel") # "  Hello, Keel!  "
"42".to_int()             # 42 (int?)
```
