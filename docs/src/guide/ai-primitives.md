# Stdlib: `Ai`

> **Alpha (v0.1).** Breaking changes expected.

The `Ai` namespace bundles LLM-backed operations. It's auto-imported — no `use` required. Under the hood, every call dispatches through the `LlmProvider` interface; the default provider is selected from `@model` (on the agent) or the global configuration.

## `Ai.classify` — categorize into an enum

```keel
urgency = Ai.classify(email.body, as: Urgency) ?? Urgency.medium

sentiment = Ai.classify(review, as: Sentiment)   # returns Sentiment? (nullable)
```

With hints: <span class="badge badge-soon">Coming soon</span>

```keel
urgency = Ai.classify(email.body,
  as: Urgency,
  considering: {
    "mentions a deadline within 24h": Urgency.high,
    "newsletter or automated":        Urgency.low
  }
) ?? Urgency.medium
```

`considering:` is a **map from hint string to enum variant**. The LLM gets the hints as classification nudges; typos or extra keys are caught by the type checker. *In v0.1 the argument is accepted but not forwarded to the LLM — tracked in [ROADMAP](../../ROADMAP.md).*

**Returns:** `T?` (where `T` is the enum). Use `?? T.variant` to supply a default inline.

## `Ai.extract` — pull structured data from text

```keel
# Inline schema map
info = Ai.extract(
  from: email,
  schema: { sender: str, subject: str, action_items: list[str] }
)

# Declared struct type (preferred — reusable, documentable)
type Invoice { vendor: str, amount: float, date: str }
result = Ai.extract("Invoice from ACME $99.99 on 2026-01-10", as: Invoice)
```

**Returns:** a struct matching `schema:` or the fields of `as: T`, nullable.

Both forms are fully wired as of v0.1.3:
- `schema: { field: "type" }` — inline map of field names to type strings.
- `as: T` — derives the schema from a declared `type T { ... }` struct; raises a runtime error if `T` is not a known struct type.

## `Ai.summarize` — condense content

```keel
brief = Ai.summarize(article, in: 3, unit: sentences)
bullets = Ai.summarize(report, format: bullets)
tldr = Ai.summarize(thread, in: 1, unit: line)
capped = Ai.summarize(article, format: bullets, max: 5, unit: sentences)
safe = Ai.summarize(article, in: 3, unit: sentences) ?? "No summary"
```

**Returns:** `str?`. Use `?? "default"` to supply a fallback inline.

All four arguments (`in:`, `unit:`, `format:`, `max:`) are fully wired as of v0.1.3:
- `format: bullets` → appends "Format your response as a bulleted list." to the system prompt.
- `format: prose` → appends "Format your response as flowing prose."
- `max: N` → appends "Use at most N {unit}." (falls back to "items" if no unit is given).

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
  based_on: [urgency, sender, content]     # `based_on:` Coming soon
)
# action: Decision[Action]?
# action.choice — one of the enum options
# action.reason — LLM's explanation
# action.confidence — 0.0..1.0
```

> **Status:** v0.1 returns a plain map `{choice, reason, confidence: 1.0}` instead of a true `Decision[T]` type. The `based_on:` argument <span class="badge badge-soon">Coming soon</span> is parsed but not yet used. Full `Decision[T]` typing is tracked in [ROADMAP](../../ROADMAP.md).

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

> **Status:** fully wired as of v0.1.3. `response_format: json` injects "Respond with valid JSON only. No prose, no markdown fences." into the system prompt and validates the reply — a non-JSON reply is a runtime error.

## Per-call model override

```keel
urgency = Ai.classify(email.body, as: Urgency, using: "fast")
reply   = Ai.draft("response to {email}", using: "smart")
```

`using:` accepts a model alias that resolves via `KEEL_MODEL_<ALIAS>` environment variables, or a literal Ollama tag (`"ollama:gemma4"` or just `"gemma4"` if a single default is set). See [LLM Providers](../config/llm-providers.md).

## Swapping the provider <span class="badge badge-soon">Coming soon</span>

```keel
# Globally
Ai.install(MyCustomProvider)

# Per-agent
agent Specialist {
  @provider MyFinetunedProvider
  @role "..."
}
```

Every `Ai.*` call goes through `LlmProvider.complete`. Any type with a matching `complete` method structurally satisfies the interface.

> **Status:** v0.1 ships with Ollama only. `Ai.install(...)` and `@provider` are reserved in the grammar but not registered in the runtime — tracked in [ROADMAP](../../ROADMAP.md).

## Why functions, not keywords

`Ai.classify`, `Ai.draft`, `Ai.extract`, and friends are ordinary prelude functions rather than built-in grammar. That keeps the parser, type checker, and LSP free of LLM-specific special cases: you still write `Ai.classify(...)` with the same ergonomics, but the implementation lives in a normal stdlib module. Swap the LLM, add a new `Ai.*` operation in a library, or shadow `Ai` with your own namespace — the core language is unchanged. See [The Prelude & Interfaces](./prelude.md).
