# Tasks

Tasks are Keel's functions. They're named, reusable, and the last expression in the body is the return value.

## Basic tasks

```keel
task greet(name: str) -> str {
  "Hello, {name}!"
}
```

Call it:

```keel
msg = greet("World")   # "Hello, World!"
```

## Parameters

```keel
use std/ai

# Typed parameters
task add(a: int, b: int) -> int {
  a + b
}

# Default values
task compose(body: str, tone: str = "friendly") -> str {
  ai.draft("response to {body}", tone: tone) ?? "(draft failed)"
}

# Struct parameters (inline type)
task triage(msg: {body: str, from: str}) -> Urgency {
  ai.classify(msg.body, as: Urgency) ?? Urgency.medium
}
```

## Implicit return

The last expression in a task body is the return value:

```keel
task double(x: int) -> int {
  x * 2              # this is the return value
}
```

Use `return` for early exits:

```keel
use std/ai

task handle(msg: {body: str, from: str}) -> str {
  if msg.from.contains("noreply") {
    return "Skipped automated email"
  }
  ai.draft("response to {msg.body}", tone: "professional") ?? "(draft failed)"
}
```

`return expr` is checked against the declared `-> T`. A mismatch is a
compile-time error; bare `return` (no value) is always accepted.

```keel
task greet() -> str {
  return 42        # error: return value: expected str, got int
}
```

## Task composition

Tasks can call other tasks:

```keel
task add(a: int, b: int) -> int { a + b }
task double(x: int) -> int { add(x, x) }

result = double(5)   # 10
```

## Top-level vs agent tasks

Tasks defined **outside** agents are reusable and testable. Tasks defined **inside** agents can access `self`:

```keel
use std/ai
use std/email

# Top-level: shared, testable
task triage(email: {body: str}) -> Urgency {
  ai.classify(email.body, as: Urgency) ?? Urgency.medium
}

# Agent-scoped: can access self.state
agent Bot {
  state { count: int = 0 }

  task increment() {
    self.count = self.count + 1
  }
}
```

Agent-owned tasks are explicit methods of the current agent:

```keel
agent Bot {
  state { count: int = 0 }

  task increment() {
    self.count = self.count + 1
  }

  task run() {
    self.increment()
  }
}
```

Inside an agent, bare `increment()` does **not** search agent-owned tasks; it
resolves lexical and top-level scope only. Use `self.increment()` for
agent-local work. Prefer top-level tasks for any logic that doesn't need agent
state.

## Variadic parameters

A task can collect any number of positional arguments into a list by prefixing the last parameter with `...`:

```keel
task greet(...names: str) -> str {
  result = ""
  for n in names { result += n + " " }
  result
}

greet("Alice", "Bob")     # names = ["Alice", "Bob"]
greet()                   # names = []  (empty list)
```

**Spread at call sites:** prefix any `list[T]` or `set[T]` with `...` to expand it into individual variadic slots:

```keel
more = ["Dave", "Eve"]
greet("Alice", ...more)       # names = ["Alice", "Dave", "Eve"]
greet(...more, ...more)       # merge two lists
```

**Fixed params before the variadic** are supported:

```keel
task labeled(prefix: str, ...items: str) -> str {
  result = prefix + ":"
  for item in items { result += " " + item }
  result
}

labeled("tags", "rust", "keel", "lang")  # "tags: rust keel lang"
```

**Important:** passing `list[T]` without `...` is a **type error** — the variadic expects `T`, not `list[T]`:

```keel
task sum(...nums: int) -> int {
  total = 0
  for n in nums { total += n }
  total
}

scores = [1, 2, 3]
sum(scores)     # ERROR: variadic arg `nums`: expected int, got list[int]
sum(...scores)  # OK: expands list into slots
```

Inside the body the variadic parameter binds as `list[T]`, so all list methods work on it.

## Generic tasks

Tasks can declare type parameters in brackets after their name. Type arguments
are **inferred at every call site** — no explicit instantiation syntax is needed.

```keel
task identity[T](x: T) -> T { x }

task first[A, B](a: A, b: B) -> A { a }

task main() {
  s: str = identity("hello")   # T = str
  n: int = identity(42)        # T = int
  f: str = first("hi", 99)     # A = str, B = int
}
```

Generic tasks work equally well inside agents:

```keel
agent Bot {
  task wrap[T](x: T) -> list[T] { [x] }
}
```

## Pipeline composition

```keel
email |> triage |> respond |> log

# With arguments
email |> triage |> respond(tone: "formal")
```

The `|>` operator passes the left-hand value as the first argument to the right-hand function.
