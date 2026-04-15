# Error Handling

## try / catch

```keel
try {
  send email
} catch err: NetworkError {
  retry 3 times with backoff { send email }
} catch err: Error {
  notify user "Failed: {err.message}"
}
```

## retry

Retry a failing operation with optional exponential backoff:

```keel
# Fixed delay (1s between attempts)
retry 3 times { send email }

# Exponential backoff: 1s, 2s, 4s
retry 3 times with backoff { send email }
```

Output on failure:

```
  ↻ Retry 1/3 in 1s: Network error
  ↻ Retry 2/3 in 2s: Network error
  ✗ Failed after 3 retries: Network error
```

## fallback

AI operations that can fail use `fallback` to guarantee a value:

```keel
# Without fallback: returns Urgency? (nullable)
result = classify text as Urgency

# With fallback: returns Urgency (guaranteed)
result = classify text as Urgency fallback medium
```

## Null coalescing

`??` provides a default for nullable values:

```keel
name = user_input ?? "anonymous"
reply = draft "response" ?? "(draft failed)"
port = env.PORT.to_int() ?? 3000
```

## Null-safe access

`?.` returns `none` instead of crashing if the left side is `none`:

```keel
subject = email?.subject           # none if email is none
length = email?.body?.length       # chained null-safe access
```

## Null assertion

`!` asserts a value is not `none` — crashes if it is:

```keel
subject = email!.subject           # throws NullError if email is none
```

Use sparingly — prefer `??` or `fallback`.
