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
reply = if has_guidance {
  draft "response" { guidance: guidance }
} else {
  draft "response" { tone: "friendly" }
} ?? "(draft failed)"
```

## when (pattern matching)

`when` is an exhaustive pattern match. The compiler **requires all cases to be handled**.

```keel
when urgency {
  low      => archive email
  medium   => auto_reply(email)
  high     => flag_and_draft(email)
  critical => escalate(email)
}
```

Missing a case is a compile error:

```
Non-exhaustive match on Urgency: missing critical
```

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

Guards with `where`:

```keel
when status {
  active where user.is_admin => grant_access()
  active                     => request_approval()
  _                          => deny()
}
```

## for loops

```keel
for email in emails {
  handle(email)
}

# With filter
for email in emails where email.unread {
  triage(email)
}
```

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
