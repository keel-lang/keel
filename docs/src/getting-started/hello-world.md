# Hello World

> **Alpha (v0.1).** Breaking changes expected.

Create a file called `hello.keel`:

```keel
agent Hello {
  @role "A friendly greeter"
  @model "claude-haiku"

  @on_start {
    Schedule.every(5.seconds, () => {
      Io.notify("Hello from Keel!")
    })
  }
}

run(Hello)
```

Run it:

```bash
KEEL_OLLAMA_MODEL=gemma4 keel run hello.keel
```

Output:

```
⚡ LLM provider: Ollama (http://localhost:11434)
▸ Starting agent Hello
  role: A friendly greeter
  model: gemma4 (ollama @ http://localhost:11434)

  ⏱ Schedule.every(5.seconds)
  ▸ Hello from Keel!

  ▸ Agent running. Press Ctrl+C to stop.
  ▸ Hello from Keel!
  ▸ Hello from Keel!
```

Press `Ctrl+C` to stop.

## What just happened?

1. **`agent Hello`** — declares an agent.
2. **`@role "..."`** — an attribute describing what the agent does. Bound to the LLM provider for any `Ai.*` calls.
3. **`@model "..."`** — which model to use.
4. **`@on_start { ... }`** — a lifecycle hook that runs when the agent starts.
5. **`Schedule.every(5.seconds, () => { ... })`** — schedules a recurring block. `Schedule` is a stdlib namespace, always in scope.
6. **`Io.notify(...)`** — prints to the terminal. `Io` is also stdlib.
7. **`run(Hello)`** — starts the agent.

No imports. The `Schedule`, `Io`, `Ai` namespaces are in scope from the start — that's the [prelude](../guide/prelude.md).

## Using AI

```keel
type Mood = happy | neutral | sad

task analyze(text: str) -> Mood {
  Ai.classify(text, as: Mood, fallback: Mood.neutral)
}

agent MoodBot {
  @role "Analyzes the mood of text"
  @model "claude-haiku"

  @on_start {
    Schedule.every(10.seconds, () => {
      mood = analyze("I love building programming languages!")
      Io.notify("Mood: {mood}")
    })
  }
}

run(MoodBot)
```

`Ai.classify` sends the text to the LLM and parses the response into one of the enum variants. `fallback:` guarantees a non-nullable result.

## Next: [Your First Agent →](./first-agent.md)
