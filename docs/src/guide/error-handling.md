# Error Handling

> **Alpha (v0.1).** Breaking changes expected.

## The two-tier failure model

Keel separates *absence* from *failure*:

| Situation | Mechanism | Handle with |
|---|---|---|
| LLM unavailable / mock / network failure | Returns `T?` (`none`) | `??` or `when` |
| LLM gave output that didn't match schema | Throws `AiSchemaError` | `try/catch` |
| Fatal config error | Hard error | fix the config |
| Programmer fault (`none!`, bad cast) | Throws `NullError` / `Error` | `try/catch` or fix the code |

## `??` — null coalescing (simple default)

Provide a default when the result is `none`:

```keel
urgency = Ai.classify(email.body, as: Urgency) ?? Urgency.medium
summary = Ai.summarize(article, in: 3, unit: sentences) ?? "No summary"
name    = user_input ?? "anonymous"
port    = Env.get("PORT")?.to_int() ?? 3000
```

This is the right tool when you don't care *why* the result is absent — just provide a sensible default.

## `try / catch` — typed error handling

Use `try/catch` when you need to distinguish failure causes:

```keel
try {
  urgency = Ai.classify(email.body, as: Urgency) ?? Urgency.medium
} catch err: AiSchemaError {
  # LLM returned something that didn't match the Urgency enum
  Io.notify("Unexpected LLM output: {err.got}")
  urgency = Urgency.medium
} catch err: Error {
  # Any other error (network, NullError, etc.)
  Io.notify("Failed: {err.message}")
}
```

`catch` matches by type name — the first matching clause runs. `Error` is the catch-all. The bound name (`err`) carries at least `message: str`.

**`AiSchemaError` fields:**

| Field | Type | Value |
|---|---|---|
| `message` | `str` | Human-readable description |
| `got` | `str` | The raw LLM output that didn't match |

## `Control.retry`

Retry a failing operation with optional exponential backoff:

```keel
# Fixed delay (1s between attempts)
Control.retry(3, () => { Email.send(reply, to: addr) })

# Exponential backoff: 1s, 2s, 4s
Control.retry(3, backoff: exponential, () => { Email.send(reply, to: addr) })

# Fixed delay between attempts
Control.retry(5, delay: 10.seconds, () => { Http.get(url) })
```

`Control.retry` is a stdlib function, not a keyword — wrap it, compose it, or write your own.

## Null-safe access — `?.`

Returns `none` instead of crashing if the left side is `none`:

```keel
subject = email?.subject           # str? — none if email is none
length  = email?.body?.length      # chained
```

## Null assertion — `!`

Asserts a value is not `none`; throws `NullError` at runtime if it is:

```keel
subject = email!.subject
```

Use sparingly — prefer `??` or `when` for safe handling.
