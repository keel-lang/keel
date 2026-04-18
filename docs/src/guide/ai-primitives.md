# Stdlib: `Ai`

> **Alpha (v0.1).** Breaking changes expected.

The `Ai` namespace bundles LLM-backed operations. It's auto-imported — no `use` required. Under the hood, every call dispatches through the `LlmProvider` interface; the default provider is selected from `@model` (on the agent) or the global configuration.

## `Ai.classify` — categorize into an enum

```keel
urgency = Ai.classify(email.body, as: Urgency, fallback: Urgency.medium)

sentiment = Ai.classify(review, as: Sentiment)   # returns Sentiment? (nullable)
```

With hints:

```keel
urgency = Ai.classify(email.body,
  as: Urgency,
  considering: {
    "mentions a deadline within 24h": Urgency.high,
    "newsletter or automated":        Urgency.low
  },
  fallback: Urgency.medium
)
```

`considering:` is a **map from hint string to enum variant**. The LLM gets the hints as classification nudges; typos or extra keys are caught by the type checker.

**Returns:** `T?` without `fallback:`, `T` with `fallback:` (where `T` is the enum).

## `Ai.extract` — pull structured data from text

```keel
info = Ai.extract(
  from: email,
  schema: { sender: str, subject: str, action_items: list[str] }
)
# info: { sender: str, subject: str, action_items: list[str] }?

dates = Ai.extract(from: contract, schema: { start: str, end: str })
```

**Returns:** a struct matching `schema:`, nullable.

## `Ai.summarize` — condense content

```keel
brief = Ai.summarize(article, in: 3, unit: sentences)
bullets = Ai.summarize(report, format: bullets)
tldr = Ai.summarize(thread, in: 1, unit: line)
safe = Ai.summarize(article, in: 3, unit: sentences, fallback: "No summary")
```

**Returns:** `str?` without `fallback:`, `str` with.

## `Ai.draft` — generate text

```keel
# Minimal
reply = Ai.draft("response to {email.body}")

# With constraints
reply = Ai.draft("response to {email.body}",
  tone: "professional",
  max_length: 150,
  guidance: user_guidance
)
```

The first positional argument is a prompt string; it supports interpolation like any other Keel string. Additional keyword arguments become hints for the model.

**Returns:** `str?`.

## `Ai.translate` — language translation

```keel
french = Ai.translate(message, to: french)
multi  = Ai.translate(ui_strings, to: [spanish, german, japanese])
```

**Returns:** `str?` for a single target, `map[str, str]?` for multi-target.

## `Ai.decide` — structured decision with reasoning

```keel
action = Ai.decide(email,
  options: [reply, forward, archive, escalate],
  based_on: [urgency, sender, content]
)
# action: Decision[Action]?
# action.choice — one of the enum options
# action.reason — LLM's explanation
# action.confidence — 0.0..1.0
```

## `Ai.prompt` — raw LLM access (escape hatch)

When the higher-level functions don't give you enough control:

```keel
type SentimentScore { score: int, explanation: str }

score = Ai.prompt(
  system: "Rate sentiment on a 1-10 scale.",
  user: "Text: {review}",
  response_format: json
) as SentimentScore
# score: SentimentScore?
```

`Ai.prompt(...)` **must be followed by `as T`**. Use `as dynamic` if the response shape is truly unknown — this is a deliberate, visible opt-out.

## Per-call model override

```keel
urgency = Ai.classify(email.body, as: Urgency, using: "claude-haiku")
reply   = Ai.draft("response to {email}", using: "claude-sonnet")
```

`using:` accepts a model string, or a concrete `LlmProvider` implementation if you've installed one.

## Swapping the provider

```keel
# Globally
Ai.install(MyCustomProvider)

# Per-agent
agent Specialist {
  @provider MyOllamaProvider
  @role "..."
}
```

Every `Ai.*` call goes through `LlmProvider.complete`. Any type with a matching `complete` method structurally satisfies the interface.

## Why functions, not keywords

`Ai.classify`, `Ai.draft`, `Ai.extract`, and friends are ordinary prelude functions rather than built-in grammar. That keeps the parser, type checker, and LSP free of LLM-specific special cases: you still write `Ai.classify(...)` with the same ergonomics, but the implementation lives in a normal stdlib module. Swap the LLM, add a new `Ai.*` operation in a library, or shadow `Ai` with your own namespace — the core language is unchanged. See [The Prelude & Interfaces](./prelude.md).
