# Hello World

Create a file called `hello.keel`:

```keel
use std/io
use std/schedule

agent Hello {
  @tools [io]
  @role "A friendly greeter"

  @on_start {
    schedule.every(5.seconds, () => {
      io.notify("Hello from Keel!")
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

  ⏱ schedule.every(5.seconds)
  ▸ Hello from Keel!

  ▸ Agent running. Press Ctrl+C to stop.
  ▸ Hello from Keel!
  ▸ Hello from Keel!
```

Press `Ctrl+C` to stop.

## What just happened?

1. **`use std/io`, `use std/schedule`** — imports the stdlib modules this file uses. Each import binds a lowercase module name (`io`, `schedule`).
2. **`agent Hello`** — declares an agent.
3. **`@tools [io]`** — grants the agent access to the effectful `io` module. Capabilities are deny-by-default inside agents.
4. **`@role "..."`** — an attribute describing what the agent does. Bound to the LLM provider for any `ai.*` calls.
5. **`@on_start { ... }`** — a lifecycle hook that runs when the agent starts.
6. **`schedule.every(5.seconds, () => { ... })`** — schedules a recurring block. `schedule` is a stdlib module, not a keyword.
7. **`io.notify(...)`** — prints to the terminal.
8. **`run(Hello)`** — starts the agent. `run` is a built-in free function — no import needed.

Two imports declare everything this program touches — see [The Standard Library](../guide/stdlib.md).

## Using AI

```keel
use std/ai
use std/io
use std/schedule

type Mood = happy | neutral | sad

task analyze(text: str) -> Mood {
  ai.classify(text, as: Mood) ?? Mood.neutral
}

agent MoodBot {
  @tools [io]
  @role "Analyzes the mood of text"

  @on_start {
    schedule.every(10.seconds, () => {
      mood = analyze("I love building programming languages!")
      io.notify("Mood: {mood}")
    })
  }
}

run(MoodBot)
```

`ai.classify` sends the text to the LLM and parses the response into one of the enum variants. `??` supplies a default when the LLM is unavailable or the response doesn't match.

## Next: [Your First Agent →](./first-agent.md)
