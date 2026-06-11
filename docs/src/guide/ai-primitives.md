# Stdlib: `Ai`

> **Alpha (v0.1).** Breaking changes expected.

The `Ai` namespace bundles LLM-backed operations. It's auto-imported — no `use` required. Under the hood, every call dispatches through the `LlmProvider` interface; the default provider is selected from `@model` (on the agent) or the global configuration.

## `ai.classify` — categorize into an enum

```keel
urgency = ai.classify(email.body, as: Urgency) ?? Urgency.medium

sentiment = ai.classify(review, as: Sentiment)   # returns Sentiment? (nullable)
```

With hints: <span class="badge badge-soon">Coming soon</span>

```keel
urgency = ai.classify(email.body,
  as: Urgency,
  considering: {
    "mentions a deadline within 24h": Urgency.high,
    "newsletter or automated":        Urgency.low
  }
) ?? Urgency.medium
```

`considering:` is a **map from hint string to enum variant**. The LLM gets the hints as classification nudges; typos or extra keys are caught by the type checker. *In v0.1 the argument is accepted but not forwarded to the LLM — tracked in [ROADMAP](../../ROADMAP.md).*

**Returns:** `T?` (where `T` is the enum). Use `?? T.variant` to supply a default inline.

## `ai.extract` — pull structured data from text

```keel
# Inline schema map
info = ai.extract(
  from: email,
  schema: { sender: str, subject: str, action_items: list[str] }
)

# Declared struct type (preferred — reusable, documentable)
type Invoice { vendor: str, amount: float, date: str }
result = ai.extract("Invoice from ACME $99.99 on 2026-01-10", as: Invoice)
```

**Returns:** a struct matching `schema:` or the fields of `as: T`, nullable.

Both forms are fully wired as of v0.1.3:
- `schema: { field: "type" }` — inline map of field names to type strings.
- `as: T` — derives the schema from a declared `type T { ... }` struct; raises a runtime error if `T` is not a known struct type. As of v0.1.19 the type checker resolves `T` from the `as:` argument, so field accesses on the result are statically checked.

## `ai.summarize` — condense content

```keel
brief = ai.summarize(article, in: 3, unit: sentences)
bullets = ai.summarize(report, format: bullets)
tldr = ai.summarize(thread, in: 1, unit: line)
capped = ai.summarize(article, format: bullets, max: 5, unit: sentences)
safe = ai.summarize(article, in: 3, unit: sentences) ?? "No summary"
```

**Returns:** `str?`. Use `?? "default"` to supply a fallback inline.

All four arguments (`in:`, `unit:`, `format:`, `max:`) are fully wired as of v0.1.3:
- `format: bullets` → appends "Format your response as a bulleted list." to the system prompt.
- `format: prose` → appends "Format your response as flowing prose."
- `max: N` → appends "Use at most N {unit}." (falls back to "items" if no unit is given).

## `ai.draft` — generate text

```keel
# Minimal
reply = ai.draft("response to {email.body}")

# With constraints
reply = ai.draft("response to {email.body}",
  tone: "professional",
  max_length: 150,
  guidance: user_guidance
)
```

The first positional argument is a prompt string; it supports interpolation like any other Keel string. Additional keyword arguments become hints for the model.

**Returns:** `str?`.

## `ai.translate` — language translation

```keel
french = ai.translate(message, to: french)
multi  = ai.translate(ui_strings, to: [spanish, german, japanese])
```

**Returns:** `str?` for a single target, `map[str, str]?` for multi-target.

## `ai.decide` — structured decision with reasoning

```keel
action = ai.decide(email,
  options: [reply, forward, archive, escalate],
  based_on: [urgency, sender, content]     # `based_on:` Coming soon
)
# action: Decision[Action]?
# action.choice — one of the enum options
# action.reason — LLM's explanation
# action.confidence — 0.0..1.0
```

> **Status:** v0.1 returns a plain map `{choice, reason, confidence: 1.0}` instead of a true `Decision[T]` type. The `based_on:` argument <span class="badge badge-soon">Coming soon</span> is parsed but not yet used. Full `Decision[T]` typing is tracked in [ROADMAP](../../ROADMAP.md).

## `ai.prompt` — raw LLM access (escape hatch)

When the higher-level functions don't give you enough control:

```keel
type SentimentScore { score: int, explanation: str }

score = ai.prompt(
  system: "Rate sentiment on a 1-10 scale.",
  user: "Text: {review}",
  response_format: json
) as SentimentScore
# score: SentimentScore?
```

`ai.prompt(...)` **must be followed by `as T`**. Use `as dynamic` if the response shape is truly unknown — this is a deliberate, visible opt-out.

> **Status:** fully wired as of v0.1.3. `response_format: json` injects "Respond with valid JSON only. No prose, no markdown fences." into the system prompt and validates the reply — a non-JSON reply is a runtime error.

## Per-call model override

```keel
urgency = ai.classify(email.body, as: Urgency, using: "fast")
reply   = ai.draft("response to {email}", using: "smart")
```

`using:` accepts a model alias that resolves via `KEEL_MODEL_<ALIAS>` environment variables, or a literal Ollama tag (`"ollama:gemma4"` or just `"gemma4"` if a single default is set). See [LLM Providers](../config/llm-providers.md).

## Swapping the provider <span class="badge badge-soon">Coming soon</span>

```keel
use std/ai

# Globally
ai.install(MyCustomProvider)

# Per-agent
agent Specialist {
  @provider MyFinetunedProvider
  @role "..."
}
```

Every `ai.*` call goes through `LlmProvider.complete`. Any type with a matching `complete` method structurally satisfies the interface.

> **Status:** v0.1 ships with Ollama only. `ai.install(...)` and `@provider` are reserved in the grammar but not registered in the runtime — tracked in [ROADMAP](../../ROADMAP.md).

## Why functions, not keywords

`ai.classify`, `ai.draft`, `ai.extract`, and friends are ordinary prelude functions rather than built-in grammar. That keeps the parser, type checker, and LSP free of LLM-specific special cases: you still write `ai.classify(...)` with the same ergonomics, but the implementation lives in a normal stdlib module. Swap the LLM, add a new `ai.*` operation in a library, or shadow `Ai` with your own namespace — the core language is unchanged. See [The Prelude & Interfaces](./stdlib.md).
