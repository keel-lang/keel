# Control Flow

> **Alpha (v0.1).** Breaking changes expected.

## if / else

`if/else` is an **expression** — it produces a value.

```keel
# Statement form (no value needed)
if urgency == high {
  escalate(email)
}

# With else
if urgency == high {
  escalate(email)
} else {
  auto_reply(email)
}

# Expression form — else is required
reply = if has_guidance {
  draft "response" { guidance: guidance }
} else {
  draft "response" { tone: "friendly" }
} ?? "(draft failed)"
```

## when (pattern matching)

`when` is an exhaustive pattern match. The compiler **requires all cases to be handled**.

`when` works as both a **statement** (branches execute side effects) and an **expression** (produces a value).

### Statement form

```keel
when urgency {
  low      => archive(email)
  medium   => auto_reply(email)
  high     => flag_and_draft(email)
  critical => escalate(email)
}
```

Missing a case is a compile error:

```
Non-exhaustive match on Urgency: missing critical
```

### Expression form

Use `when` anywhere a value is expected — assignment, return, argument. All arms must produce the same type.

```keel
label = when urgency {
  low      => "low priority"
  medium   => "medium priority"
  high     => "high priority"
  critical => "critical"
}
```

```keel
task describe(score: str) -> str {
  when score {
    "A" => "excellent"
    "B" => "good"
    _   => "needs work"
  }
}
```

### Wildcard and multiple patterns

Use `_` as a wildcard:

```keel
when urgency {
  critical => escalate(email)
  _        => auto_reply(email)     # covers low, medium, high
}
```

Multiple patterns per arm:

```keel
when urgency {
  low, medium    => auto_reply(email)
  high, critical => escalate(email)
}
```

Guards with `where` (block body required when using a guard):

```keel
when status {
  active where user.is_admin => { grant_access() }
  active                     => request_approval()
  _                          => deny()
}
```

## for loops

```keel
for email in emails {
  handle(email)
}

# With inline filter
for email in emails if email.unread {
  triage(email)
}

# Works with destructuring too
for { from, subject } in emails if subject != "" {
  Io.show("{from}: {subject}")
}

# Works with ranges
for x in 1..10 if x % 2 == 0 {
  Io.show(x)
}
```

## Augmented assignment

`+=`, `-=`, `*=`, and `/=` mutate an existing variable in its nearest enclosing scope. They do not create a new binding — a plain `=` in the same position would shadow; these update.

```keel
total = 0
for i in 1..5 {
    total += i      # updates outer `total`, not a loop-scoped shadow
}
# total is 15
```

Works on `self.field` inside an agent handler:

```keel
agent Counter {
    state { count: int = 0 }

    @on_start {
        self.count += 1
    }
}
```

Compound forms: `total -= cost`, `total *= factor`, `total /= divisor`.

## return

Explicit early return from a task:

```keel
task check(x: int) -> str {
  if x > 100 {
    return "too big"
  }
  if x < 0 {
    return "negative"
  }
  "ok"
}
```

## Retry, timeout, deadline

The `Control` namespace wraps a closure with resilience primitives. Each
takes a 0-arg lambda and returns whatever the lambda returned (or
raises an error if the budget is exhausted).

### `Control.retry(n, fn)`

Re-invoke `fn` up to `n` times until it returns without raising. The
last attempt's error is surfaced if every attempt fails.

```keel
result = Control.retry(5, () => {
  return Ai.prompt(system: "rate 1-10", user: review, response_format: json)
})
```

### `Control.with_timeout(duration, fn)`

Race the closure against a duration. Raises `TimeoutError` if the
closure runs past the deadline.

```keel
fast = Control.with_timeout(2.seconds, () => {
  return slow_external_call()
})
```

### `Control.with_deadline(datetime, fn)`

Same shape as `with_timeout`, but the limit is an absolute RFC 3339
timestamp instead of a duration. Raises `DeadlineError` on expiry.

```keel
done = Control.with_deadline("2026-12-31T23:59:00Z", () => {
  return long_task()
})
```

These three primitives compose: a retry of a `with_timeout` block bounds
each attempt's runtime, and the loop's overall budget is the retry count.
