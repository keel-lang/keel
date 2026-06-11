<div style="text-align: center; margin: 2em 0 1em;">
  <img src="keel-logo.svg" alt="Keel" style="width: 140px; height: 140px;"/>
</div>

# The Keel Language

> **Latest: [%%VERSION%%](./release-notes.md)** — All stdlib errors are now typed — distinguish causes in try/catch blocks.

> **Alpha.** Breaking changes expected between 0.x releases. See [versioning](#versioning-and-breaking-changes).


**Keel** is a small, statically-typed language for building AI agents. The actor model is its one concurrency primitive. Everything else — AI calls, scheduling, human-in-the-loop, HTTP, email, memory — lives in a **standard library that is auto-imported** into every program.

You never write `use keel/ai`. You write `ai.classify(...)` and the prelude makes it work.

```keel
use std/ai
use std/email
use std/io
use std/schedule

type Urgency = low | medium | high | critical

agent EmailAssistant {
  @role "Professional email assistant"
  @tools [email]

  on message(msg: Message) {
    urgency = ai.classify(msg.body, as: Urgency) ?? Urgency.medium

    when urgency {
      low, medium => {
        reply = ai.draft("response to {msg.body}", tone: "friendly")
        if io.confirm(reply) { email.send(reply, to: msg.from) }
      }
      high, critical => {
        io.notify("{urgency}: {msg.subject}")
        guidance = io.ask("How should I respond?")
        reply = ai.draft("response to {msg.body}", guidance: guidance)
        if io.confirm(reply) { email.send(reply, to: msg.from) }
      }
    }
  }

  @on_start {
    schedule.every(5.minutes, () => {
      for email in email.fetch(unread: true) {
        send(self, email.as_message())
      }
    })
  }
}

run(EmailAssistant)
```

Zero imports. Namespaces like `Ai`, `Io`, `Email`, and `Schedule` are in scope from the start.

## Design Principles

1. **Small core, deep stdlib.** If a feature can be a library, it is one. The core language has a small reserved keyword set.
2. **Agents are primitives.** `agent` is the only concurrency model. Per-agent serial mailboxes with isolated mutable state via `self`.
3. **Prelude-as-stdlib.** The standard library is auto-imported. Users get keyword-feel ergonomics without the compiler having to know about every feature.
4. **Interfaces everywhere.** LLM providers, memory stores, HTTP clients, loggers — all behind interfaces. Users swap implementations without leaving the language.
5. **Statically typed.** Full inference. Exhaustive pattern matching. Nullable safety. No implicit `any`.
6. **No silent fallbacks.** Misconfiguration fails loud at startup, not with plausible-looking nonsense at runtime.

## Try It

Install and run your first agent:

```bash
curl https://keel-lang.dev/install.sh | sh
keel run examples/showcase.keel
```

→ [Installation](./getting-started/installation.md)

## Versioning and Breaking Changes

Keel is in alpha. Semver is **not** respected between 0.x minor versions.

- **0.1.x** — current alpha. Complete reset: new design + migrated implementation. Breaking changes in patch releases are allowed.
- **0.2.x / 0.3.x** — deliberately unplanned until v0.1 ships.
- **1.0.x** — first API-stable release. Semver begins.

See [SPEC.md](https://github.com/keel-lang/keel/blob/main/SPEC.md) for the authoritative design and [ROADMAP.md](https://github.com/keel-lang/keel/blob/main/ROADMAP.md) for the plan.
