# Inbox Assistant

Reference v0.1 example. A two-agent program that reads your mailbox,
triages urgency, drafts replies, escalates critical issues, and
posts a daily digest to a webhook.

Use this directory to exercise every wired language feature and
stdlib namespace in Keel v0.1. If a change to the lexer, parser,
type checker, interpreter, or runtime doesn't break this example,
it probably doesn't break anything user-visible.

## What it covers

**Language**

- Simple enums (`Urgency`, `Tone`) and rich enums with per-variant fields (`Action`)
- Type aliases (`EmailId`) and nominal struct types (`Sentiment`)
- Exhaustive `when` with simple-variant and rich-variant destructuring
- Nullable types with `?.`, `??`, and `fallback:`
- `as T` cast on `Ai.prompt`
- Triple-quoted strings with interpolation
- Escaped braces (`\{` / `\}`) in string literals
- Duration literals (`5.minutes`, `3.seconds`)
- Named arguments on every stdlib call
- Top-level tasks + agent-scoped tasks
- `state { ... }` with `self.` read/write
- `@role`, `@model`, `@on_start`
- Cross-agent messaging via `Agent.send` + `on message(...)` handler
- Lambdas in `Schedule.every` / `after` / `at`
- `for … in …`, `if/else`, `return`
- Collection literals: list, map, set

**Stdlib**

- `Ai.classify`, `Ai.summarize`, `Ai.draft`, `Ai.extract`, `Ai.translate`, `Ai.decide`, `Ai.prompt … as T`
- `Io.notify`, `Io.show`, `Io.ask`, `Io.confirm`
- `Schedule.every`, `Schedule.after`, `Schedule.at`
- `Email.fetch`, `Email.send`
- `Http.post`
- `Env.get`, `Env.require`
- `Log.info`, `Log.warn`, `Log.debug`
- `Agent.run`, `Agent.send`

## Running it

Copy the config template and fill in what you have:

```bash
cp .env.example .env
$EDITOR .env
set -a && source .env && set +a
```

Then run it. Two common modes:

```bash
# Offline smoke test — mocks every LLM call, returns empty on Email.
KEEL_LLM=mock KEEL_ONESHOT=1 keel run inbox_assistant.keel

# Full run — requires Ollama on localhost and optional IMAP/SMTP.
keel run inbox_assistant.keel
```

`KEEL_ONESHOT=1` exits after the first idle window (useful for CI
and smoke tests). Omit it to let the assistant run continuously.
`Ctrl-C` always exits cleanly.

Two useful flags while iterating:

- `keel --log-level debug run inbox_assistant.keel` — surfaces the
  `Log.debug` lines (`boot state: ...`, `no new mail`, etc.) that
  are hidden under the default `info` threshold.
- `keel --trace run inbox_assistant.keel` — narrates every `Ai.*`
  call as it fires, including the input preview and which model
  was used.

## What to watch for

When validating a language change, a clean run should:

- Print `InboxAssistant booting` and the boot-state debug line
- Print `Assistant ready — watching inbox every 5 minutes` after 3s
- Print the `Email.fetch` warning (or triage real emails if IMAP is set)
- `Schedule.at` logs `"registered"` silently; triggering it requires a
  near-future `DIGEST_AT`
- All four `when action { … }` arms must be reachable — to exercise
  them, send yourself an email with strong language and vary the
  `Urgency` output

If any of the above regresses, the triggering change probably needs
investigation. For a feature-by-feature status map, see
[ROADMAP.md](../../ROADMAP.md).
