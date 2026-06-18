# Stdlib: `ai`

`use std/ai` to access LLM-backed operations. Under the hood, every call dispatches through the `LlmProvider` interface; the default provider is selected from `@model` (on the agent) or `KEEL_OLLAMA_MODEL`.

## `ai.classify` — categorize into an enum

```keel
urgency = ai.classify(email.body, as: Urgency) ?? Urgency.medium

sentiment = ai.classify(review, as: Sentiment)   # returns Sentiment? (nullable)
```

With hints:

```keel
urgency = ai.classify(email.body,
  as: Urgency,
  considering: {
    "mentions a deadline within 24h": Urgency.high,
    "newsletter or automated":        Urgency.low
  }
) ?? Urgency.medium
```

`considering:` is a **map from hint string to enum variant**. The LLM gets the hints as classification nudges.

**Returns:** `T?` (where `T` is the enum). Use `?? T.variant` to supply a default inline.

> **`??` only catches absence, not failure.** `??` fires on a `none` result —
> the model returned no answer, no model is configured, or mock mode is active.
> It does **not** rescue thrown failures: if the provider is unreachable
> `ai.classify` raises `AiError` (`reason: "unavailable"`), and if the model
> returns text matching **no** variant of `T` it raises `AiSchemaError`. HTML-heavy
> or chatty inputs trigger the schema case against real models. In production,
> wrap the call in `try/catch` so one bad input — or a brief outage — can't abort
> a batch:
>
> ```keel
> try {
>   urgency = ai.classify(email.body, as: Urgency) ?? Urgency.medium
> } catch err: AiSchemaError {
>   urgency = Urgency.medium   # err.got holds the raw output
> }
> ```
>
> See [Error Handling](./error-handling.md).

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

> Like `ai.classify`, `ai.extract` **raises `AiSchemaError`** when the model's
> output can't be coerced to the requested shape — `??` only covers the `none`
> (no-answer / mock) case. Wrap it in `try/catch AiSchemaError` when the input
> isn't trusted to produce clean structured data.

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
  options: [reply, forward, archive, escalate]
)
# action: map with keys choice, reason, confidence
# action.choice — one of the enum options
# action.reason — LLM's explanation
# action.confidence — 0.0..1.0
```

**Returns:** a map `{choice, reason, confidence: 1.0}`. The `choice` value is the selected option; `reason` is the LLM's explanation.

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

> **Status:** fully wired as of v0.1.3. `response_format: json` injects "Respond with valid JSON only. No prose, no markdown fences." into the system prompt and validates the reply — a non-JSON reply raises `AiSchemaError` (with the raw output in `got`), the same error `ai.classify` and `ai.extract` raise on unparseable output.

## Per-call model override

```keel
urgency = ai.classify(email.body, as: Urgency, using: "fast")
reply   = ai.draft("response to {email}", using: "smart")
```

`using:` accepts a model alias that resolves via `KEEL_MODEL_<ALIAS>` environment variables, or a literal Ollama tag (`"ollama:gemma4"` or just `"gemma4"` if a single default is set). See [LLM Providers](../config/llm-providers.md).

Every `ai.*` call goes through the `LlmProvider` interface. Ollama is the only wired backend; `@provider` and `ai.install(...)` are reserved for a future release.

## Why functions, not keywords

`ai.classify`, `ai.draft`, `ai.extract`, and friends are ordinary stdlib functions (imported with `use std/ai`) rather than built-in grammar. That keeps the parser, type checker, and LSP free of LLM-specific special cases: you still write `ai.classify(...)` with the same ergonomics, but the implementation lives in a normal stdlib module. Swap the LLM, add a new `ai.*` operation in a library, or shadow the `ai` binding with your own module — the core language is unchanged. See [The Standard Library](./stdlib.md).
