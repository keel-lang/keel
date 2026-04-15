# Hello World

Create a file called `hello.keel`:

```keel
agent Hello {
  role "A friendly greeter"
  model "claude-haiku"

  every 5.seconds {
    notify user "Hello from Keel!"
  }
}

run Hello
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

  ⏱ polling every 5 seconds
  ▸ Hello from Keel!

  ▸ Agent running. Press Ctrl+C to stop.
  ▸ Hello from Keel!
  ▸ Hello from Keel!
```

The agent prints "Hello from Keel!" every 5 seconds until you press `Ctrl+C`.

## What just happened?

1. **`agent Hello`** — declares an agent named `Hello`
2. **`role "..."`** — describes what the agent does (used as system prompt context)
3. **`model "claude-haiku"`** — which LLM model to use (mapped to your local Ollama model)
4. **`every 5.seconds { ... }`** — runs the block every 5 seconds
5. **`notify user "..."`** — prints a message to the terminal
6. **`run Hello`** — starts the agent

## Using AI

Let's make the agent actually use AI:

```keel
type Mood = happy | neutral | sad

task analyze(text: str) -> Mood {
  classify text as Mood fallback neutral
}

agent MoodBot {
  role "Analyzes the mood of text"
  model "claude-haiku"

  every 10.seconds {
    mood = analyze("I love building programming languages!")
    notify user "Mood: {mood}"
  }
}

run MoodBot
```

The `classify` keyword sends the text to the LLM and maps the response to one of the enum variants. The `fallback neutral` ensures you always get a valid result.

## Next: [Your First Agent →](./first-agent.md)
