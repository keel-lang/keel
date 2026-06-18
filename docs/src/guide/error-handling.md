# Error Handling

## The two-tier failure model

Keel separates *absence* from *failure*:

| Situation | Mechanism | Handle with |
|---|---|---|
| Model had no answer / no model configured / mock | Returns `T?` (`none`) | `??` or `when` |
| LLM provider unreachable or misconfigured | Throws `AiError` (`reason` field) | `try/catch` |
| LLM gave output that didn't match schema | Throws `AiSchemaError` | `try/catch` |
| Namespace operation failed (I/O, network, parse) | Throws a typed error | `try/catch` |
| Fatal config error | Hard error | fix the config |
| Programmer fault (`none!`, bad cast) | Throws `Error` | `try/catch` or fix the code |

## `??` — null coalescing (simple default)

Provide a default when the result is `none`:

```keel
urgency = ai.classify(email.body, as: Urgency) ?? Urgency.medium
summary = ai.summarize(article, in: 3, unit: sentences) ?? "No summary"
name    = user_input ?? "anonymous"
port    = env.get("PORT")?.to_int() ?? 3000
```

This is the right tool when you don't care *why* the result is absent — just provide a sensible default.

## `try / catch` — typed error handling

Use `try/catch` when you need to distinguish failure causes. Every stdlib namespace that can fail raises a named error type; `Error` is the catch-all:

```keel
use std/ai
use std/csv
use std/email
use std/file
use std/http
use std/io
use std/shell

try {
  data = file.read("config.json")
} catch e: FileError {
  data = "{}"                      # handle missing/unreadable file
} catch e: Error {
  io.show("unexpected: {e.message}")
}

try {
  rows = csv.parse_records(raw)
} catch e: CsvError {
  io.show("bad CSV: {e.message}")
}

try {
  resp = http.get("https://api.example.com/data")
} catch e: HttpError {
  io.show("network error: {e.message}")
}

try {
  result = shell.run("build.sh")
} catch e: ShellError {
  io.show("shell error: {e.message}")
}

try {
  urgency = ai.classify(email.body, as: Urgency)
} catch err: AiSchemaError {
  io.notify("Unexpected LLM output: {err.got}")
  urgency = Urgency.medium
} catch err: Error {
  io.notify("Failed: {err.message}")
}
```

`catch` matches by type name — the first matching clause runs. `Error` is the catch-all. The bound name carries at least `message: str`.

## Error type registry

All stdlib error types and the namespaces that raise them:

| Error type | Raised by | Notes |
|---|---|---|
| `FileError` | `file.*` | I/O failures: not found, permission denied, etc. |
| `CsvError` | `csv.*` | Parse failures, bad row structure, non-string cells |
| `DbError` | `db.*` | Query/exec failures, connection errors |
| `CacheError` | `cache.*` | Serialization errors |
| `MathError` | `math.*` | Domain errors: `sqrt(-1)`, `log(0)`, `asin(2)` |
| `MemoryError` | `memory.*` | Persistence errors, `@memory none` restriction |
| `EmailError` | `email.*` | IMAP/SMTP failures |
| `HttpError` | `http.*` | Network failures (connection refused, timeout) |
| `ShellError` | `shell.*` | Failed to spawn shell or wait for process |
| `JsonError` | `json.*` | JSON parse errors, serialization failures |
| `EnvError` | `env.require` | Required env variable not set |
| `AiError` | `ai.*` | Provider unreachable, network failure, or misconfiguration (`reason` field: `"unavailable"` or `"provider"`) |
| `AiSchemaError` | `ai.extract`, `ai.classify`, `ai.prompt` (JSON mode) | LLM output didn't match expected schema (`got` field contains raw output) |
| `CapabilityError` | any `@tools`-restricted method | Method not allowed by the agent's `@tools` list |
| `TimeoutError` | `control.with_timeout` | Closure exceeded the given duration |
| `DeadlineError` | `control.with_deadline` | Closure ran past the deadline |
| `UserRaised` | `raise` statement | User-raised error |
| `RuntimeBusy` | any async event | Interpreter event queue full |

Catch any of these specifically, or use `catch e: Error` as a fallback for all.

**`AiError` extra field:**

| Field | Type | Value |
|---|---|---|
| `message` | `str` | Human-readable description |
| `reason` | `str` | `"unavailable"` (network / provider unreachable) or `"provider"` (model not mapped / config fault) |

A provider failure throws `AiError` rather than returning `none`, so `ai.classify(...) ?? default` never silently hides an outage — the `??` default applies only to genuine absence (no answer, no model configured, mock mode). Catch `AiError` to retry or degrade explicitly:

```keel
try {
  urgency = ai.classify(email.body, as: Urgency) ?? Urgency.medium
} catch err: AiError {
  io.notify("classifier {err.reason}: {err.message}")
  urgency = Urgency.medium
}
```

**`AiSchemaError` extra field:**

| Field | Type | Value |
|---|---|---|
| `message` | `str` | Human-readable description |
| `got` | `str` | The raw LLM output that didn't match |

## Diagnostic codes

When an uncaught error reaches the CLI, the stable diagnostic code appears:

```
Error: keel::runtime::FileError
  × FileError: file.read `missing.txt`: No such file or directory
```

Codes follow the pattern `keel::runtime::<TypeName>`. Tooling and host integrations can inspect the code directly without parsing the message string.

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

`raise` produces a `UserRaised` error; caught by `catch err: UserRaised` or the generic `catch err: Error`:

```keel
try {
    result = divide(10, 0)
} catch err: UserRaised {
    io.notify("Raised: {err.message}")
} catch err: Error {
    io.notify("Other failure: {err.message}")
}
```

Non-string values are converted using their display representation.

## `control.retry`

Retry a failing operation:

```keel
# Retry up to 3 times; last error surfaces if all fail
control.retry(3, () => { email.send(reply, to: addr) })

# Combine with try/catch to handle specific errors after all retries fail
try {
    control.retry(3, () => { http.get(url) })
} catch e: HttpError {
    io.show("still failing after 3 tries: {e.message}")
}
```

## `control.with_timeout` / `control.with_deadline`

```keel
try {
    result = control.with_timeout(5.seconds, () => {
        http.get("https://slow.example.com/api")
    })
} catch e: TimeoutError {
    io.show("timed out: {e.message}")
}

try {
    result = control.with_deadline("2025-12-31T23:59:59Z", () => {
        process_batch()
    })
} catch e: DeadlineError {
    io.show("deadline passed: {e.message}")
}
```

## Null-safe access — `?.`

Returns `none` instead of crashing if the left side is `none`:

```keel
subject = email?.subject           # str? — none if email is none
length  = email?.body?.length      # chained
```

## Null assertion — `!`

Asserts a value is not `none`; throws a plain `Error` at runtime if it is:

```keel
subject = email!.subject
```

Use sparingly — prefer `??` or `when` for safe handling.
