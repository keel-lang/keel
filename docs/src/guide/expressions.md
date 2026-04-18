# Variables & Expressions

> **Alpha (v0.1).** Breaking changes expected.

## Variables

Variables are **immutable by default**. Once bound, they can't be reassigned — but you can shadow them:

```keel
name = "Keel"
name = "Keel v2"    # shadows the previous binding — original value is gone
```

Agent state fields are the exception — they're mutable via `self.`:

```keel
agent Counter {
  state { count: int = 0 }

  task increment() {
    self.count = self.count + 1   # mutable state
  }
}
```

## Type annotations

Type annotations are optional — the compiler infers types. But you can add them:

```keel
x = 42              # inferred as int
y: int = 42         # explicit annotation
z: float = 3.14     # explicit
```

## Arithmetic

```keel
x = 2 + 3 * 4       # 14 (standard precedence)
y = 10 / 3           # 3 (integer division)
z = 10.0 / 3.0       # 3.333... (float division)
r = 17 % 5           # 2 (modulo)
```

## Comparison

```keel
5 > 3                # true
"abc" == "abc"       # true
x != none            # true if x is not none
```

## Boolean logic

```keel
true and false       # false
true or false        # true
not true             # false

if x > 0 and x < 100 {
  notify user "in range"
}
```

## Null coalescing

The `??` operator provides a default when the left side is `none`:

```keel
name = user_input ?? "anonymous"
port = env.PORT.to_int() ?? 3000
result = classify text as Mood ?? neutral
```

## Pipeline operator

Chain operations with `|>`:

```keel
email |> triage |> respond |> log

# Equivalent to:
log(respond(triage(email)))

# With extra arguments:
email |> triage |> respond(tone: "friendly") |> log("email_responses")
```

## Field access

```keel
email.subject                  # field access
email?.subject                 # null-safe — returns none if email is none
email!.subject                 # null assertion — throws if email is none

env.API_KEY                    # environment variable
self.count                     # agent state field
```

## Struct and list literals

```keel
# Struct (map)
person = {name: "Alice", age: 30, active: true}

# List
items = [1, 2, 3, 4, 5]

# Nested
records = [
  {name: "Alice", score: 95},
  {name: "Bob", score: 87}
]
```
