# Variables & Expressions

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
  io.notify("in range")
}
```

## Type rules for operators

The type checker validates operand types at `keel check` time.

| Operator | Valid operand types |
|---|---|
| `+` | `int`, `float` (mixed ok), `str + str`, `list + list`, `datetime + duration`, `duration + datetime`, `duration + duration` |
| `-` | `int`, `float` (mixed ok), `datetime - duration`, `datetime - datetime`, `duration - duration` |
| `*` `/` `%` | `int`, `float` (mixed ok) |
| `<` `>` `<=` `>=` | `int`, `float` (mixed ok), `str + str`, `datetime`, `duration` |
| `==` `!=` | any |
| `and` `or` | any |
| `+=` `-=` `*=` `/=` `%=` | same rules as the base operator |

`unknown` / `dynamic` values skip the check (gradual typing escape hatch).

Type mismatches are caught early:

```keel
x = "hello" + 5     # error: cannot apply `+` to str and int
x = "hello" < 42    # error: cannot apply `<` to str and int
x = 0
x += "oops"         # error: cannot apply `+` to int and str
```

## Null coalescing

The `??` operator provides a default when the left side is `none`:

```keel
name = user_input ?? "anonymous"
port = env.get("PORT")?.to_int() ?? 3000
mood = ai.classify(text, as: Mood) ?? Mood.neutral
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
msg.subject                    # field access
msg?.subject                   # null-safe — returns none if msg is none
msg!.subject                   # null assertion — throws if msg is none

env.get("API_KEY")             # environment variable (use std/env)
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

## Destructuring

Unpack struct fields or tuple elements directly into named bindings.

**Struct shorthand** — field names become variable names:

```keel
{name, age} = person
io.show("{name} is {age}")
```

**Struct rename** — bind a field under a different local name:

```keel
{urgency: u, category: c} = result
```

**Tuple positional** — bind list elements by position:

```keel
(label, count) = ("alpha", 42)
```

**In a `for` loop** — destructure each element as it's iterated:

```keel
for {from, subject} in emails {
  io.show("{from}: {subject}")
}
```

**In a task parameter** — destructure a struct argument at the call boundary:

```keel
use std/io

task handle({body, from}: Email) {
  io.show("From {from}: {body}")
}
```

The type checker enforces that struct fields exist and that tuple arity matches. Missing fields and mismatches are compile-time errors.
