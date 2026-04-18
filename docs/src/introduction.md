<div style="text-align: center; margin-bottom: 2em;">
  <img src="wordmark.svg" alt="Keel" style="max-width: 280px; margin-bottom: 0.5em;" class="light-only"/>
  <img src="wordmark-dark.svg" alt="Keel" style="max-width: 280px; margin-bottom: 0.5em;" class="dark-only"/>
  <p style="color: #64748b; font-size: 1.1em;">A programming language where AI agents are first-class citizens</p>
</div>

> **Alpha (v0.1).** Keel is in early design. Breaking changes expected between 0.x releases. No production users, no binary release yet — build from source. See [versioning](#versioning-and-breaking-changes).

# The Keel Language

**Keel** is a small, statically-typed language for building AI agents. The actor model is its one concurrency primitive. Everything else — AI calls, scheduling, human-in-the-loop, HTTP, email, memory — lives in a **standard library that is auto-imported** into every program.

You never write `use keel/ai`. You write `Ai.classify(...)` and the prelude makes it work.

```keel
type Urgency = low | medium | high | critical

agent EmailAssistant {
  @role "Professional email assistant"
  @tools [Email]

  on message(msg: Message) {
    urgency = Ai.classify(msg.body, as: Urgency, fallback: Urgency.medium)

    when urgency {
      low, medium => {
        reply = Ai.draft("response to {msg.body}", tone: "friendly")
        if Io.confirm(reply) { Email.send(reply, to: msg.from) }
      }
      high, critical => {
        Io.notify("{urgency}: {msg.subject}")
        guidance = Io.ask("How should I respond?")
        reply = Ai.draft("response to {msg.body}", guidance: guidance)
        if Io.confirm(reply) { Email.send(reply, to: msg.from) }
      }
    }
  }

  @on_start {
    Schedule.every(5.minutes, () => {
      for email in Email.fetch(unread: true) {
        self.dispatch(message: email.as_message())
      }
    })
  }
}

run(EmailAssistant)
```

Zero imports. Namespaces like `Ai`, `Io`, `Email`, and `Schedule` are in scope from the start.

## Design Principles

1. **Small core, deep stdlib.** If a feature can be a library, it is one. The core language has **22 keywords**.
2. **Agents are primitives.** `agent` is the only concurrency model. Per-agent serial mailboxes with isolated mutable state via `self`.
3. **Prelude-as-stdlib.** The standard library is auto-imported. Users get keyword-feel ergonomics without the compiler having to know about every feature.
4. **Interfaces everywhere.** LLM providers, memory stores, HTTP clients, loggers — all behind interfaces. Users swap implementations without leaving the language.
5. **Statically typed.** Full inference. Exhaustive pattern matching. Nullable safety. No implicit `any`.
6. **No silent fallbacks.** Misconfiguration fails loud at startup, not with plausible-looking nonsense at runtime.

## Try It

Build from source and run your first agent:

```bash
git clone https://github.com/keel-lang/keel.git
cd keel && cargo build --release
./target/release/keel run examples/minimal.keel
```

→ [Installation](./getting-started/installation.md)

## Versioning and Breaking Changes

Keel is in alpha. Semver is **not** respected between 0.x minor versions.

- **0.1.x** — current alpha. Complete reset: new design + migrated implementation. Breaking changes in patch releases are allowed.
- **0.2.x / 0.3.x** — deliberately unplanned until v0.1 ships.
- **1.0.x** — first API-stable release. Semver begins.

See [SPEC.md](https://github.com/keel-lang/keel/blob/main/SPEC.md) for the authoritative design and [ROADMAP.md](https://github.com/keel-lang/keel/blob/main/ROADMAP.md) for the plan.
