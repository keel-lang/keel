# Your First Agent

> **Alpha (v0.1).** Breaking changes expected.

Let's build a task prioritizer that classifies items and reports them by urgency.

## 1. Scaffold

```bash
keel init task-prioritizer
cd task-prioritizer
```

This creates `main.keel`.

## 2. Define types

```keel
type Priority = low | medium | high | critical

type Task {
  title: str
  description: str
}
```

Types are either **enums** (a set of variants, optionally with data) or **structs** (named fields). The type checker enforces exhaustive matching on enums.

## 3. A classification task

```keel
use std/ai

task prioritize(t: Task) -> Priority {
  ai.classify(t.description,
    as: Priority,
    considering: {
      "blocks other people":       Priority.critical,
      "has a deadline this week":  Priority.high,
      "nice to have":              Priority.low
    }
  ) ?? Priority.medium
}
```

`ai.classify` sends `t.description` to the LLM with the hints and parses the response into the enum. `?? Priority.medium` supplies a default when the LLM is unavailable or returns nothing parseable.

## 4. Build the agent

```keel
use std/io
use std/schedule

agent Prioritizer {
  @role "You help prioritize a task list"

  state {
    processed: int = 0
  }

  task run_batch(tasks: list[Task]) {
    for t in tasks {
      priority = prioritize(t)
      self.processed = self.processed + 1

      when priority {
        critical => io.notify("CRITICAL: {t.title}")
        high     => io.notify("HIGH: {t.title}")
        medium   => io.notify("MEDIUM: {t.title}")
        low      => io.notify("LOW: {t.title}")
      }
    }
    io.notify("Processed {self.processed} tasks total")
  }

  @on_start {
    schedule.every(1.hour, () => {
      tasks = [
        {title: "Fix login bug", description: "Users can't log in, blocks the team"},
        {title: "Update README",  description: "Nice to have, not urgent"},
        {title: "Deploy v2.0",    description: "Release deadline is Friday"}
      ]
      self.run_batch(tasks)
    })
  }
}

run(Prioritizer)
```

## 5. Run it

```bash
KEEL_OLLAMA_MODEL=gemma4 keel run main.keel
```

## 6. Type-check

```bash
keel check main.keel
```

Forget a `when` variant and the compiler stops you:

```
  × Type error
   ╭─[main.keel:18:7]
 18 │       when priority {
   ·       ─────┬─────
   ·            ╰── Non-exhaustive match on Priority: missing high, medium, low
 19 │         critical => io.notify("CRITICAL")
   ╰────
```

## Key takeaways

| Concept | What it does |
|---|---|
| `type Priority = low \| medium \| high \| critical` | Enum — the type checker enforces exhaustive matching |
| `ai.classify(x, as: T) ?? V` | LLM-powered classification into an enum; `??` supplies a default when the result is absent |
| `considering: [...]` | Hints to the LLM per variant |
| `when value { ... }` | Exhaustive pattern matching, checked at compile time |
| `state { field: T = default }` | Mutable agent state, accessed via `self.field` |
| `@on_start { ... }` | Runs once when the agent starts |
| `schedule.every(duration, () => { ... })` | Recurring execution, from the stdlib |
| `io.notify(...)` | Terminal notification from the stdlib |
| `run(MyAgent)` | Starts the agent |

No imports. Every namespace (`Ai`, `Io`, `Schedule`) is in scope via the [prelude](../guide/stdlib.md).

## Next: [Language Guide →](../guide/types.md)
