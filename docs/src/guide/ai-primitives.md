# AI Primitives

Keel has **six built-in AI keywords**. They're not library functions — they're language-level constructs with custom grammar, type inference, and LLM routing.

## classify

Categorize input into a predefined enum type.

```keel
urgency = classify email.body as Urgency
```

With classification hints:

```keel
urgency = classify email.body as Urgency considering [
  "mentions a deadline within 24h"  => high,
  "uses urgent/angry language"      => critical,
  "newsletter or automated message" => low
] fallback medium using "claude-haiku"
```

**Return type:** `T?` without fallback, `T` with fallback.

## summarize

Condense content.

```keel
brief = summarize article in 3 sentences
bullets = summarize report format bullets
tldr = summarize thread in 1 line fallback "(no summary)"
```

**Return type:** `str?` without fallback, `str` with fallback.

## draft

Generate text content.

```keel
reply = draft "response to {email}" {
  tone: "friendly",
  max_length: 150,
  guidance: "include action items"
}
```

The description is a string with interpolation — variables are included as context in the LLM prompt.

**Return type:** `str?`

## extract

Pull structured data from unstructured text.

```keel
info = extract {
  sender: str,
  subject: str,
  action_items: list[str]
} from email_body
```

The LLM extracts the specified fields and returns them as a struct/map.

**Return type:** `{fields}?`

## translate

Language translation.

```keel
french = translate message to french

# Multi-target
localized = translate ui_text to [spanish, german, japanese]
# Returns: {spanish: "...", german: "...", japanese: "..."}
```

**Return type:** `str?` for single target, `map[str, str]?` for multi-target.

## decide

Structured decision with reasoning.

```keel
action = decide email {
  options: [reply, forward, archive, escalate],
  based_on: [urgency, sender, content]
}
# action.choice  — the selected option
# action.reason  — LLM's explanation
```

**Return type:** `{choice: str, reason: str}?`

## Model override with `using`

All AI primitives inherit the agent's model. Override per-operation:

```keel
# Fast model for classification
urgency = classify email.body as Urgency using "claude-haiku"

# Capable model for drafting
reply = draft "response" { tone: "formal" } using "claude-sonnet"
```

## prompt — raw LLM access

When the built-in primitives don't fit, use `prompt` for full control:

```keel
result = prompt {
  system: "You are a legal document analyzer.",
  user: "Extract all liability clauses from: {document}"
} as LiabilityClauses
```

`prompt` always requires `as Type` — there's no untyped path.

## Nullable safety

| Expression | Type | Why |
|------------|------|-----|
| `classify X as T` | `T?` | LLM might fail to classify |
| `classify X as T fallback V` | `T` | Fallback guarantees a value |
| `draft "..." { opts }` | `str?` | LLM might fail |
| `draft "..." ?? "default"` | `str` | Null coalescing guarantees |
| `ask user "prompt"` | `str` | User always responds |
| `confirm user msg` | `bool` | Always true or false |
