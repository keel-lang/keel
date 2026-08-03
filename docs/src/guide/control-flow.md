# Control Flow

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
reply = if guidance != none {
  ai.draft("response", guidance: guidance)
} else {
  ai.draft("response", tone: "friendly")
} ?? "(draft failed)"
```

Use it anywhere a value is expected — assignment, `return`, a call argument, an interpolation slot — and chain it with `else if`. Both branches must produce the same type.

```keel
task band(score: int) -> str {
  return if score > 90 { "A" } else if score > 80 { "B" } else { "C" }
}

task pick(n: int) -> str {
  return shout(if n == 0 { "zero" } else { "other" })
}
```

`else` really is required in expression position: an `if` with no `else` has no value to produce when the condition is false. [`keel build`](../cli/build.md) enforces that — today it is the only engine that does, since `keel check` still accepts the form and `keel run` evaluates it to `none`. It likewise rejects an *unannotated* `if`-expression whose `then` branch exits via `return`, which leaves no tail value to infer the result type from; annotate it (`x: int = …`) or use it where the surrounding syntax already pins a type.

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

### Struct patterns

Bind named fields from a struct value directly in `when` arms using `{ field1, field2 }` syntax:

```keel
type Signal { price: float, volume: float, rsi: float }

task classify(s: Signal) -> str {
  when s {
    { price, volume } where price > 1000.0 and volume > 0.0 => "active"
    { price }         where price > 1000.0                  => "thin"
    _                                                        => "quiet"
  }
}
```

The bound fields are available in both the `where` guard and the arm body.

An **unguarded** struct arm matches any value of that struct type and satisfies exhaustiveness — no `_` is needed:

```keel
when order {
  { quantity, price } => {
    total = quantity * price
    io.show("total: {total}")
  }
}
```

A **guarded** struct arm is not total; add a `_` fallback if no other arm covers the remaining cases.

The subject must be a struct, and every field you name must exist on it. Matching a struct pattern against an enum or other non-struct value, or naming a field the struct does not declare, is a compile-time error — a mistyped field never silently binds `none`. An unguarded arm is only total against a **non-nullable** struct; for a nullable subject like `Signal?`, the `none` case still needs its own arm (or a `_`).

Struct patterns work in both statement and expression `when` forms.

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
  io.show("{from}: {subject}")
}

# Works with ranges
for x in 1..10 if x % 2 == 0 {
  io.show(x)
}
```

## while loops

Repeat a body until the condition becomes false:

```keel
n = 5
while n > 0 {
    io.show("tick: {n}")
    n -= 1
}
```

Use `while true { ... break }` for indefinite loops that exit via `break`:

```keel
total = 0
i = 1
while true {
    total += i
    i += 1
    if total > 10 { break }
}
```

The condition must be `bool`. `break` and `continue` work the same as in `for` loops.

## break and continue

`break` exits the nearest enclosing `for` or `while` loop immediately. `continue` skips the rest of the current iteration and advances to the next.

```keel
# break — stop as soon as the target is found
for item in items {
    if item == target {
        break
    }
    process(item)
}

# continue — skip even numbers in a while loop
x = 0
while x < 10 {
    x += 1
    if x % 2 == 0 { continue }
    process_odd(x)
}
```

Both keywords affect only the **innermost** loop. There are no labeled jumps yet.

```keel
# break inside a nested loop only exits the inner one
for outer in 1..5 {
    for inner in 1..10 {
        if inner > 2 {
            break          # exits inner loop only
        }
    }
    io.show("outer={outer}")   # still runs 5 times
}
```

> `break` and `continue` are reserved keywords. Using them outside a loop is a runtime error.

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

### `control.retry(n, fn)`

Re-invoke `fn` up to `n` times until it returns without raising. The
last attempt's error is surfaced if every attempt fails.

```keel
result = control.retry(5, () => {
  return ai.prompt(system: "rate 1-10", user: review, response_format: json)
})
```

### `control.with_timeout(duration, fn)`

Race the closure against a duration. Raises `TimeoutError` if the
closure runs past the deadline.

```keel
fast = control.with_timeout(2.seconds, () => {
  return slow_external_call()
})
```

### `control.with_deadline(datetime, fn)`

Same shape as `with_timeout`, but the limit is an absolute RFC 3339
timestamp instead of a duration. Raises `DeadlineError` on expiry.

```keel
done = control.with_deadline("2026-12-31T23:59:00Z", () => {
  return long_task()
})
```

These three primitives compose: a retry of a `with_timeout` block bounds
each attempt's runtime, and the loop's overall budget is the retry count.
