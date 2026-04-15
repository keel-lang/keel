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
# Typed parameters
task add(a: int, b: int) -> int {
  a + b
}

# Default values
task compose(email: str, tone: str = "friendly") -> str {
  draft "response to {email}" { tone: tone }
}

# Struct parameters (inline type)
task triage(email: {body: str, from: str}) -> Urgency {
  classify email.body as Urgency fallback medium
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
task handle(email: {body: str, from: str}) -> str {
  if email.from.contains("noreply") {
    return "Skipped automated email"
  }
  draft "response to {email}" { tone: "professional" } ?? "(draft failed)"
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
# Top-level: shared, testable
task triage(email: {body: str}) -> Urgency {
  classify email.body as Urgency fallback medium
}

# Agent-scoped: can access self.state
agent Bot {
  state { count: int = 0 }

  task increment() {
    self.count = self.count + 1
  }
}
```

Prefer top-level tasks for any logic that doesn't need agent state.

## Pipeline composition

```keel
email |> triage |> respond |> log

# With arguments
email |> triage |> respond(tone: "formal")
```

The `|>` operator passes the left-hand value as the first argument to the right-hand function.
