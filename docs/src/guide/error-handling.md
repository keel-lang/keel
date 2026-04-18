# Error Handling

> **Alpha (v0.1).** Breaking changes expected.

## try / catch

```keel
try {
  Email.send(reply, to: email.from)
} catch err: NetworkError {
  Control.retry(3, backoff: exponential, () => { Email.send(reply, to: email.from) })
} catch err: Error {
  Io.notify("Failed: {err.message}")
}
```

`catch` matches by variant — the same mechanism as `when` on any enum. `Error` is the catch-all.

## Control.retry

Retry a failing operation with optional exponential backoff:

```keel
# Fixed delay (1s between attempts)
Control.retry(3, () => { Email.send(reply, to: addr) })

# Exponential backoff: 1s, 2s, 4s
Control.retry(3, backoff: exponential, () => { Email.send(reply, to: addr) })

# Fixed delay between attempts
Control.retry(5, delay: 10.seconds, () => { Http.get(url) })
```

`Control.retry` is a stdlib function, not a keyword — so you can wrap it, compose it, or write your own variant.

## fallback on AI operations

AI operations that can fail accept `fallback:` to guarantee a value:

```keel
# Without fallback: returns Urgency?
result = Ai.classify(text, as: Urgency)

# With fallback: returns Urgency
result = Ai.classify(text, as: Urgency, fallback: Urgency.medium)
```

## Null coalescing — `??`

Provide a default for nullable values:

```keel
name  = user_input ?? "anonymous"
reply = Ai.draft("response") ?? "(draft failed)"
port  = Env.get("PORT")?.to_int() ?? 3000
```

## Null-safe access — `?.`

Returns `none` instead of crashing if the left side is `none`:

```keel
subject = email?.subject           # str? — none if email is none
length  = email?.body?.length      # chained
```

## Null assertion — `!.`

Asserts a value is not `none`; throws `NullError` if it is:

```keel
subject = email!.subject
```

Use sparingly — prefer `??`, `fallback:`, or `when`.

## Pattern-matching on a Result

`Result[T, E]` is the standard way stdlib surfaces rich errors:

```keel
when Http.request(url: "https://api.example.com") {
  ok { value }  => process(value)
  err { error } => Io.notify("Request failed: {error.message}")
}
```
