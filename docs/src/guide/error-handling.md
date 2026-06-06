# Error Handling

> **Alpha (v0.1).** Breaking changes expected.

## The two-tier failure model

Keel separates *absence* from *failure*:

| Situation | Mechanism | Handle with |
|---|---|---|
| LLM unavailable / mock / network failure | Returns `T?` (`none`) | `??` or `when` |
| LLM gave output that didn't match schema | Throws `AiSchemaError` | `try/catch` |
| Namespace operation failed (I/O, network, parse) | Throws a typed error | `try/catch` |
| Fatal config error | Hard error | fix the config |
| Programmer fault (`none!`, bad cast) | Throws `Error` | `try/catch` or fix the code |

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

Use `try/catch` when you need to distinguish failure causes. Every stdlib namespace that can fail raises a named error type; `Error` is the catch-all:

```keel
try {
  data = File.read("config.json")
} catch e: FileError {
  data = "{}"                      # handle missing/unreadable file
} catch e: Error {
  Io.show("unexpected: {e.message}")
}

try {
  rows = Csv.parse_records(raw)
} catch e: CsvError {
  Io.show("bad CSV: {e.message}")
}

try {
  resp = Http.get("https://api.example.com/data")
} catch e: HttpError {
  Io.show("network error: {e.message}")
}

try {
  result = Shell.run("build.sh")
} catch e: ShellError {
  Io.show("shell error: {e.message}")
}

try {
  urgency = Ai.classify(email.body, as: Urgency)
} catch err: AiSchemaError {
  Io.notify("Unexpected LLM output: {err.got}")
  urgency = Urgency.medium
} catch err: Error {
  Io.notify("Failed: {err.message}")
}
```

`catch` matches by type name — the first matching clause runs. `Error` is the catch-all. The bound name carries at least `message: str`.

## Error type registry

All stdlib error types and the namespaces that raise them:

| Error type | Raised by | Notes |
|---|---|---|
| `FileError` | `File.*` | I/O failures: not found, permission denied, etc. |
| `CsvError` | `Csv.*` | Parse failures, bad row structure, non-string cells |
| `DbError` | `Db.*` | Query/exec failures, connection errors |
| `CacheError` | `Cache.*` | Serialization errors |
| `MathError` | `Math.*` | Domain errors: `sqrt(-1)`, `log(0)`, `asin(2)` |
| `MemoryError` | `Memory.*` | Persistence errors, `@memory none` restriction |
| `EmailError` | `Email.*` | IMAP/SMTP failures |
| `HttpError` | `Http.*` | Network failures (connection refused, timeout) |
| `ShellError` | `Shell.*` | Failed to spawn shell or wait for process |
| `JsonError` | `Json.*` | JSON parse errors, serialization failures |
| `EnvError` | `Env.require` | Required env variable not set |
| `AiError` | `Ai.*` | LLM config errors |
| `AiSchemaError` | `Ai.extract`, `Ai.classify` | LLM output didn't match expected schema (`got` field contains raw output) |
| `CapabilityError` | any `@tools`-restricted method | Method not allowed by the agent's `@tools` list |
| `TimeoutError` | `Control.with_timeout` | Closure exceeded the given duration |
| `DeadlineError` | `Control.with_deadline` | Closure ran past the deadline |
| `UserRaised` | `raise` statement | User-raised error |
| `RuntimeBusy` | any async event | Interpreter event queue full |

Catch any of these specifically, or use `catch e: Error` as a fallback for all.

**`AiSchemaError` extra field:**

| Field | Type | Value |
|---|---|---|
| `message` | `str` | Human-readable description |
| `got` | `str` | The raw LLM output that didn't match |

## Diagnostic codes

When an uncaught error reaches the CLI, the stable diagnostic code appears:

```
Error: keel::runtime::FileError
  × FileError: File.read `missing.txt`: No such file or directory
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
    Io.notify("Raised: {err.message}")
} catch err: Error {
    Io.notify("Other failure: {err.message}")
}
```

Non-string values are converted using their display representation.

## `Control.retry`

Retry a failing operation:

```keel
# Retry up to 3 times; last error surfaces if all fail
Control.retry(3, () => { Email.send(reply, to: addr) })

# Combine with try/catch to handle specific errors after all retries fail
try {
    Control.retry(3, () => { Http.get(url) })
} catch e: HttpError {
    Io.show("still failing after 3 tries: {e.message}")
}
```

## `Control.with_timeout` / `Control.with_deadline`

```keel
try {
    result = Control.with_timeout(5.seconds, () => {
        Http.get("https://slow.example.com/api")
    })
} catch e: TimeoutError {
    Io.show("timed out: {e.message}")
}

try {
    result = Control.with_deadline("2025-12-31T23:59:59Z", () => {
        process_batch()
    })
} catch e: DeadlineError {
    Io.show("deadline passed: {e.message}")
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
