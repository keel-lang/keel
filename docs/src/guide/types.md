# Types

Keel is **statically typed with full inference**. You rarely need to write type annotations — the compiler figures them out — but every expression has a known type, and mismatches are caught before your code runs.

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

AI operations return nullable types when they can fail:

```keel
result = classify text as Urgency     # Urgency? — might be none
safe = result ?? medium               # Urgency — guaranteed

# Or use fallback for non-nullable directly:
result = classify text as Urgency fallback medium   # Urgency
```

## Collections

```keel
nums = [1, 2, 3]                         # list[int]
names = ["alice", "bob"]                  # list[str]
info = {name: "Zied", role: "builder"}   # map[str, str]
```

**List properties:**

| Property | Returns | Description |
|----------|---------|-------------|
| `.count` | `int` | Number of elements |
| `.first` | `T?` | First element or none |
| `.last` | `T?` | Last element or none |
| `.is_empty` | `bool` | True if count == 0 |

## Type conversions

```keel
port_str = "8080"
port = port_str.to_int() ?? 3000     # int — 8080 or default
ratio = 3.to_float() / 4.to_float()  # float — 0.75
label = Urgency.high.to_str()        # str — "high"
```

Conversions that can fail return nullable types (`str.to_int()` → `int?`). Conversions that always succeed return non-nullable (`int.to_str()` → `str`).

## Duration literals

```keel
5.seconds    30.minutes    2.hours    1.day    7.days

# Short forms
30.sec       1.min         2.hr       1.d
```
