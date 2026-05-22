# String Interpolation

> **Alpha (v0.1).** Breaking changes expected.

Keel strings support `{expr}` interpolation — variables and expressions inside strings are evaluated at runtime.

## Basic interpolation

```keel
name = "Keel"
notify user "Hello, {name}!"           # "Hello, Keel!"
notify user "Count: {items.count}"     # "Count: 3"
notify user "Sum: {a + b}"            # expression evaluation
```

## Dotted paths

```keel
notify user "From: {email.from}"
notify user "Status: {self.count}"
notify user "Key: {env.API_KEY}"
```

## Nested string literals

A `{...}` slot accepts any expression — including another string literal
with its own slots. The lexer tracks brace depth and recurses into
nested `"..."`, so quotes inside `{}` are not confused with the outer
string's terminator.

```keel
name = "world"
mood = "cheerful"

Io.show("hi {"there {name}"}")
# → "hi there world"

Io.show("tone: {"speaking in a {mood.to_str()} voice"}")
# → "tone: speaking in a cheerful voice"
```

`\"` inside an outer string still produces a literal quote without
opening a nested string.

## Escape sequences

| Sequence | Result |
|----------|--------|
| `\n` | Newline |
| `\t` | Tab |
| `\r` | Carriage return |
| `\\` | Backslash |
| `\"` | Double quote |
| `\{` | Literal `{` (prevents interpolation) |
| `\}` | Literal `}` |

```keel
notify user "Line 1\nLine 2"
notify user "Price: \{not interpolated\}"
```

## String methods

| Method | Returns | Example |
|--------|---------|---------|
| `.len()` / `.length` | `int` | `"hello".len()` → `5` |
| `.is_empty()` | `bool` | `"".is_empty()` → `true` |
| `.contains(s)` | `bool` | `"hello".contains("ell")` → `true` |
| `.starts_with(s)` | `bool` | `"hello".starts_with("hel")` → `true` |
| `.ends_with(s)` | `bool` | `"hello".ends_with("lo")` → `true` |
| `.trim()` | `str` | `" hi ".trim()` → `"hi"` |
| `.trim_start()` | `str` | `" hi ".trim_start()` → `"hi "` |
| `.trim_end()` | `str` | `" hi ".trim_end()` → `" hi"` |
| `.upper()` | `str` | `"hello".upper()` → `"HELLO"` |
| `.lower()` | `str` | `"HELLO".lower()` → `"hello"` |
| `.repeat(n)` | `str` | `"ha".repeat(3)` → `"hahaha"` |
| `.slice(start, end?)` | `str` | `"hello".slice(1, 3)` → `"el"` |
| `.index_of(needle)` | `int?` | `"hello world".index_of("world")` → `6` |
| `.split(sep)` | `list[str]` | `"a,b,c".split(",")` → `["a","b","c"]` |
| `.replace(old, new)` | `str` | `"hello".replace("l","r")` → `"herro"` |
| `.to_int()` | `int?` | `"42".to_int()` → `42`; `"bad".to_int()` → `none` |
| `.to_float()` | `float?` | `"3.14".to_float()` → `3.14`; `"x".to_float()` → `none` |
| `.to_str()` | `str` | identity — useful in generic contexts |
| `.truncate(max)` | `str` | Truncates to `max` chars; appends `"…"` if cut. `max` must be ≥ 0 |
| `.pad(width, char?)` | `str` | Left-pads to `width` with `char` (default `" "`). `width` must be ≥ 0 |
| `.matches(pattern)` | `bool` | `true` if regex matches anywhere in the string |
| `.extract(pattern)` | `str?` | First capture group of regex; `none` if no match |
| `.find_all(pattern)` | `list[str]` | All non-overlapping regex matches; empty list if none |
| `.sub(pattern, replacement)` | `str` | Replace all regex matches; supports `$1` backreferences |

Patterns use standard regex syntax (Rust `regex` crate — no look-behind).

```keel
text = "Invoice #1042 — Total: $3,750.00 due 2026-05-15"

# Regex test
if text.matches("\\d{4}-\\d{2}-\\d{2}") {
    Io.show("looks like a date")
}

# Extract first capture group (returns str?)
amount = text.extract("\\$(\\S+)")         # "3,750.00"

# All matches
dates = text.find_all("\\d{4}-\\d{2}-\\d{2}")  # ["2026-05-15"]

# Regex replace (global)
redacted = text.sub("\\$[\\d,]+\\.\\d{2}", "[REDACTED]")

# Truncate and pad
short  = text.truncate(20)         # "Invoice #1042 — Tot…"
padded = "42".pad(6)               # "    42"
zeroed = "42".pad(6, char: "0")   # "000042"
```

## Format specifiers

A slot may include a format spec after a colon — `{expr:spec}` — for precise number formatting and text alignment. The spec uses a Python-style mini-language.

### Float precision

```keel
pi = 3.14159
Io.show("{pi:.2f}")     # → "3.14"
Io.show("{pi:.4f}")     # → "3.1416"

n = 42
Io.show("{n:.2f}")      # → "42.00"  (int auto-promoted to float)
```

### Width and alignment

| Spec | Alignment | Example |
|------|-----------|---------|
| `:<N` | Left-align, space-padded to N chars | `{"hi":<8}` → `"hi      "` |
| `:>N` | Right-align, space-padded to N chars | `{42:>8}` → `"      42"` |
| `:^N` | Center, space-padded to N chars | `{"hi":^8}` → `"   hi   "` |
| `:N` | Bare width — right-align shorthand | `{42:8}` → `"      42"` |

### Combining alignment and precision

```keel
price = 19.5
Io.show("{price:>10.2f}")    # → "     19.50"
Io.show("{price:<10.2f}")    # → "19.50     "
```

### Building tables

```keel
Io.show("{"Name":<12} {"Score":>8}")
Io.show("{"Alice":<12} {0.975:>8.3f}")
Io.show("{"Bob":<12} {0.742:>8.3f}")
# →
# Name          Score
# Alice         0.975
# Bob           0.742
```

Named arguments inside a slot are not confused with the format spec — `{f(key: v):>10}` parses `f(key: v)` as the expression and `>10` as the spec, because the separator colon is only detected at outermost bracket depth.

## `Stringable` interface — custom interpolation

By default, only primitive values (`str`, `int`, `float`, `bool`) and built-in types (`Uuid`, `datetime`) can be used inside `"{...}"` interpolation slots. To make your own struct type work in interpolation, implement the `Stringable` interface:

```keel
type Point {
  x: float
  y: float
}

impl Stringable for Point {
  task to_str(self) -> str {
    "({self.x}, {self.y})"
  }
}

p: Point = { x: 1.5, y: 2.0 }
Io.show("Position: {p}")      # → "Position: (1.5, 2.0)"
s = p.to_str()                # explicit call — "(1.5, 2.0)"
```

### Syntax

```
impl InterfaceName for TypeName {
  task method(self) -> ReturnType {
    ...
  }
}
```

- `impl` is a reserved keyword.
- `for` is the same keyword used in `for` loops; there is no ambiguity because `impl` always precedes it here.
- `self` inside the block refers to the receiver value (typed as `TypeName`). Use `self.field` to access struct fields.
- Only methods matching a declared interface signature are valid inside `impl` blocks.

### Multiple types

Each type needs its own `impl` block:

```keel
type Color = red | green | blue

impl Stringable for Color {
  task to_str(self) -> str {
    when self {
      red   => "red"
      green => "green"
      blue  => "blue"
    }
  }
}

c: Color = Color.green
Io.show("Favourite: {c}")   # → "Favourite: green"
```

### Fallback behaviour

Values that do **not** implement `Stringable` still render in interpolation using their built-in display representation (the same output you see from `Io.show`). Implementing `Stringable` lets you override that representation for your types.

---

## `Cache` namespace — in-memory shared cache

`Cache` is a process-scoped, in-memory key-value store with optional TTL. It persists across agent restarts within the same process run but is cleared when the process exits.

```keel
Cache.set("session:abc", user_data, ttl: 30.minutes)
session = Cache.get("session:abc")   # value or none (if expired/missing)
Cache.delete("session:abc")
Cache.clear()                        # flush everything
```

| Method | Returns | Notes |
|--------|---------|-------|
| `Cache.set(key, value, ttl?)` | `none` | `ttl` is a duration literal; omit for no expiry |
| `Cache.get(key)` | `Value?` | `none` if missing or expired |
| `Cache.delete(key)` | `none` | No-op if key doesn't exist |
| `Cache.clear()` | `none` | Flushes all entries |

> **Scope:** `Cache` is process-wide (all agents share one store) and cleared on restart. `Memory` is per-agent and optionally persistent. Use `Cache` for rate-limiting tokens, deduplication keys, or short-lived shared results; use `Memory` for per-agent state that should survive across runs.
