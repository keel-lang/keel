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

## Diagnostic codes

Typed runtime errors expose a stable machine-readable code via `miette`'s diagnostic protocol. When an error propagates uncaught to the CLI, the code appears in the error output:

```
Error: keel::runtime::FileError
  × FileError: File.read `missing.txt`: No such file or directory
```

Codes follow the pattern `keel::runtime::<TypeName>`. Currently classified:

| Error type | Diagnostic code |
|---|---|
| `AiError` | `keel::runtime::AiError` |
| `AiSchemaError` | `keel::runtime::AiSchemaError` |
| `FileError` | `keel::runtime::FileError` |

More namespace errors will gain codes as they are migrated. Tooling and host integrations can inspect the code directly without parsing the error message string.

## `raise` — throw an error

Throw an error from any point in a task or agent handler:

```keel
task divide(a: int, b: int) -> int {
    if b == 0 {
        raise "division by zero"
    }
    return a / b
}
```

Any string becomes the error message. Caught by `try/catch err: Error`:

```keel
try {
    result = divide(10, 0)
} catch err: Error {
    Io.notify("Failed: {err.message}")
}
```

Non-string values are converted using their display representation. `raise` pairs symmetrically with `try/catch` — the two form a complete error-signalling and recovery model.

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
