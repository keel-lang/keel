# Human Interaction

Keel has four built-in keywords for human-in-the-loop workflows.

## ask

Prompts the user and **blocks** until they respond.

```keel
answer = ask user "How should I respond to this email?"
```

**Return type:** `str`

## confirm

Asks for yes/no approval. Returns `bool`.

```keel
approved = confirm user "Send this reply?\n\n{draft_reply}"
if approved { send draft_reply to email }
```

Shorthand with `then`:

```keel
confirm user reply then send reply to email
# Equivalent to: if confirm user reply { send reply to email }
```

**Return type:** `bool`

## notify

Non-blocking notification. Does not wait for a response.

```keel
notify user "Email classified as critical"
notify user "{emails.count} new messages"
```

## show

Presents structured data to the user. Automatically formats maps as key-value displays and lists of maps as tables.

```keel
# Key-value display
show user {
  from:    email.from,
  subject: email.subject,
  urgency: urgency
}
```

Output:

```
  ┌
  │ from     alice@example.com
  │ subject  Q3 Review
  │ urgency  Urgency.high
  └
```

List of maps renders as a table:

```keel
show user [
  {name: "Alice", status: "active"},
  {name: "Bob", status: "away"}
]
```

Output:

```
  name   status
  ─────  ──────
  Alice  active
  Bob    away
```
