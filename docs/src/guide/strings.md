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
| `.length` | `int` | `"hello".length` → `5` |
| `.is_empty` | `bool` | `"".is_empty` → `true` |
| `.contains(s)` | `bool` | `"hello".contains("ell")` → `true` |
| `.starts_with(s)` | `bool` | `"hello".starts_with("hel")` → `true` |
| `.ends_with(s)` | `bool` | `"hello".ends_with("lo")` → `true` |
| `.trim()` | `str` | `" hi ".trim()` → `"hi"` |
| `.upper()` | `str` | `"hello".upper()` → `"HELLO"` |
| `.lower()` | `str` | `"HELLO".lower()` → `"hello"` |
| `.split(sep)` | `list[str]` | `"a,b,c".split(",")` → `["a","b","c"]` |
| `.replace(old, new)` | `str` | `"hello".replace("l","r")` → `"herro"` |
| `.to_int()` | `int?` | `"42".to_int()` → `42` |
| `.to_float()` | `float?` | `"3.14".to_float()` → `3.14` |

## `Str` namespace — regex & processing

The `Str` namespace provides regex matching and string manipulation:

```keel
# Test if a pattern matches
if Str.match(text, "\\d{4}-\\d{2}-\\d{2}") {
  Io.show("looks like a date")
}

# Extract first capture group (returns str?)
phone = Str.extract(text, "(\\+?\\d[\\d\\s-]{7,})")

# Truncate with ellipsis
short = Str.truncate("Hello, World!", 7)   # "Hello, …"

# Left-pad with spaces (or custom char)
padded  = Str.pad("42", 6)              # "    42"
zeroed  = Str.pad("42", 6, char: "0")  # "000042"
```

| Method | Returns | Notes |
|--------|---------|-------|
| `Str.match(text, pattern)` | `bool` | True if regex matches anywhere in `text` |
| `Str.extract(text, pattern)` | `str?` | First capture group; `none` if no match |
| `Str.truncate(text, max)` | `str` | Truncates to `max` chars; appends `"…"` if cut. `max` must be ≥ 0 |
| `Str.pad(text, width, char?)` | `str` | Left-pads to `width` with `char` (default `" "`). `width` must be ≥ 0 |

Patterns use standard regex syntax (Rust `regex` crate — no look-behind).

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

> **Scope:** `Cache` fills the gap between `self.` (per-agent state) and `Memory` (persistent vector store, planned for v0.2). Use it for rate-limiting tokens, deduplication keys, or short-lived computed results.
