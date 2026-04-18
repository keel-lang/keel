# Stdlib: `Io`

> **Alpha (v0.1).** Breaking changes expected.

The `Io` namespace provides human-in-the-loop interaction. All four functions route through a channel (terminal by default; Slack, email, or custom via interface).

## `Io.ask` — blocking input

Blocks the current handler until the user responds.

```keel
answer = Io.ask("How should I respond to this?")
# answer: str

choice = Io.ask("Pick a priority", options: [low, medium, high])
# choice: the enum

pick = Io.ask("Approve deployment?", options: [yes, no], via: slack)
```

**Returns:** `str` for free text, or the enum type when `options:` is provided.

## `Io.confirm` — yes/no approval

```keel
approved = Io.confirm("Send this reply?\n\n{draft_reply}")
if approved { Email.send(draft_reply, to: email.from) }

# Via a specific channel
approved = Io.confirm("Delete 50 files?", via: slack)
```

**Returns:** `bool` — `true` if approved, `false` otherwise.

## `Io.notify` — non-blocking message

```keel
Io.notify("Email classified as critical")
Io.notify("Weekly report ready", via: slack)
```

Does not wait for a response.

## `Io.show` — display structured data

```keel
Io.show(email_summary)
Io.show(table(results))
Io.show(chart(metrics))
```

Formats structured data for the terminal or UI.

## Channels

`via:` selects the channel. Default is terminal; the stdlib ships with slack and email channels, and additional channels can be installed by implementing the `Channel` interface.

```keel
Io.notify("Deploy started", via: slack)
```

## Why a library, not keywords

`Io.ask`, `Io.confirm`, `Io.notify`, and `Io.show` are prelude functions rather than reserved words. Treating them as regular functions keeps the grammar small and lets them compose with the rest of the language: they work inside pipelines, accept lambdas for formatting, and can be wrapped by user code without fighting the parser. See [The Prelude & Interfaces](./prelude.md).
