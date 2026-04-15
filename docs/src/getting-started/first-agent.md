# Your First Agent

Let's build a real agent — a task prioritizer that classifies items and presents them sorted by urgency.

## Step 1: Scaffold the project

```bash
keel init task-prioritizer
cd task-prioritizer
```

This creates `main.keel` with a starter agent.

## Step 2: Define your types

Replace `main.keel` with:

```keel
type Priority = low | medium | high | critical

type Task {
  title: str
  description: str
}
```

Types in Keel are either **enums** (a set of variants) or **structs** (named fields). The type checker ensures you handle all variants in `when` blocks.

## Step 3: Write the classification task

```keel
task prioritize(task: Task) -> Priority {
  classify task.description as Priority considering [
    "blocks other people"        => critical,
    "has a deadline this week"   => high,
    "nice to have"               => low
  ] fallback medium
}
```

This sends the task description to the LLM with classification hints. The `fallback medium` ensures you always get a valid `Priority` — never `none`.

## Step 4: Build the agent

```keel
agent Prioritizer {
  role "You help prioritize a task list"
  model "claude-sonnet"

  state {
    processed: int = 0
  }

  task run_batch(tasks: list[Task]) {
    for task in tasks {
      priority = prioritize(task)
      self.processed = self.processed + 1

      when priority {
        critical => notify user "🔴 CRITICAL: {task.title}"
        high     => notify user "🟠 HIGH: {task.title}"
        medium   => notify user "🟡 MEDIUM: {task.title}"
        low      => notify user "🟢 LOW: {task.title}"
      }
    }
    notify user "Processed {self.processed} tasks total"
  }

  every 1.hour {
    tasks = [
      {title: "Fix login bug", description: "Users can't log in, blocks the whole team"},
      {title: "Update README", description: "Nice to have, not urgent"},
      {title: "Deploy v2.0", description: "Release deadline is Friday"}
    ]
    run_batch(tasks)
  }
}

run Prioritizer
```

## Step 5: Run it

```bash
KEEL_OLLAMA_MODEL=gemma4 keel run main.keel
```

## Step 6: Type-check it

Before running, you can verify your code has no type errors:

```bash
keel check main.keel
```

If you forget a variant in a `when` block:

```
  × Type error
   ╭─[main.keel:25:5]
 24 │     when priority {
 25 │       critical => notify user "CRITICAL"
   ·       ─────────┬─────────
   ·                 ╰── Non-exhaustive match on Priority: missing high, medium, low
 26 │     }
   ╰────
```

## Key takeaways

| Concept | What it does |
|---------|-------------|
| `type Priority = low \| medium \| high \| critical` | Defines an enum — the type checker enforces exhaustive matching |
| `classify text as Type` | AI-powered classification into your enum |
| `considering [...]` | Gives the LLM hints for each variant |
| `fallback value` | Guarantees a non-nullable result |
| `when value { ... }` | Pattern matching — compiler checks all variants are covered |
| `state { field: Type = default }` | Mutable agent state, accessed via `self.field` |
| `every duration { ... }` | Scheduled recurring execution |

## Next: [Language Guide →](../guide/types.md)
