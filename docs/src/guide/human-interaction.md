# Stdlib: `Io`

> **Alpha (v0.1).** Breaking changes expected.

The `Io` namespace provides human-in-the-loop interaction. All four functions route through a channel (terminal by default; Slack, email, or custom via interface).

## `io.ask` — blocking input

Blocks the current handler until the user responds.

```keel
answer = io.ask("How should I respond to this?")
# answer: str

choice = io.ask("Pick a priority", options: [low, medium, high])
# choice: the enum

pick = io.ask("Approve deployment?", options: [yes, no], via: slack)
```

**Returns:** `str` for free text, or the enum type when `options:` is provided.

## `io.confirm` — yes/no approval

```keel
approved = io.confirm("Send this reply?\n\n{draft_reply}")
if approved { email.send(draft_reply, to: email.from) }

# Via a specific channel
approved = io.confirm("Delete 50 files?", via: slack)
```

**Returns:** `bool` — `true` if approved, `false` otherwise.

## `io.notify` — non-blocking message

```keel
io.notify("Email classified as critical")
io.notify("Weekly report ready", via: slack)
```

Does not wait for a response.

## `io.show` — display structured data

```keel
io.show(email_summary)
io.show(table(results))
io.show(chart(metrics))
```

Formats structured data for the terminal or UI.

## Channels

`via:` selects the channel. Default is terminal; the stdlib ships with slack and email channels, and additional channels can be installed by implementing the `Channel` interface.

```keel
io.notify("Deploy started", via: slack)
```

## Why a library, not keywords

`io.ask`, `io.confirm`, `io.notify`, and `io.show` are prelude functions rather than reserved words. Treating them as regular functions keeps the grammar small and lets them compose with the rest of the language: they work inside pipelines, accept lambdas for formatting, and can be wrapped by user code without fighting the parser. See [The Prelude & Interfaces](./stdlib.md).
