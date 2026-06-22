<section class="keel-hero">
  <img class="keel-hero-logo" src="keel-logo.svg" alt="Keel" />
  <a class="keel-version-pill" href="release-notes.html">v0.2.4 — latest release</a>
  <h1 class="keel-hero-title">The Keel Language</h1>
  <p class="keel-hero-tagline">A small, statically-typed language where AI agents are first-class citizens.</p>
  <div class="keel-hero-actions">
    <a class="keel-btn keel-btn-primary" href="getting-started/installation.html">Install Keel &rarr;</a>
    <a class="keel-btn" href="https://github.com/keel-lang/keel">View on GitHub</a>
  </div>
</section>

<div class="keel-features">
  <div class="keel-feature">
    <svg class="keel-feature-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="6" cy="6" r="2.4"/><circle cx="18" cy="6" r="2.4"/><circle cx="12" cy="18" r="2.4"/><path d="M7.6 7.7 10.6 16M16.4 7.7 13.4 16M8 6h8"/></svg>
    <h3>Agents are primitives</h3>
    <p>The actor model is the one concurrency primitive. Per-agent serial mailboxes, isolated mutable state via <code>self</code> — no shared-memory races to reason about.</p>
  </div>
  <div class="keel-feature">
    <svg class="keel-feature-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M12 3 21 8 12 13 3 8z"/><path d="M3 12l9 5 9-5M3 16l9 5 9-5"/></svg>
    <h3>Small core, deep stdlib</h3>
    <p>AI calls, scheduling, HTTP, email, memory — all live in a standard library you import one line at a time with <code>use std/&lt;name&gt;</code>. The core language stays tiny.</p>
  </div>
  <div class="keel-feature">
    <svg class="keel-feature-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M12 3 5 6v5c0 4 3 7 7 9 4-2 7-5 7-9V6z"/><path d="m9.5 12 1.8 1.8 3.4-3.6"/></svg>
    <h3>Capabilities, deny-by-default</h3>
    <p>An agent can only touch what its <code>@tools</code> list declares. Undeclared effects are compile-time errors — every program is auditable at a glance.</p>
  </div>
  <div class="keel-feature">
    <svg class="keel-feature-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M12 2 4 5v6c0 5 3.5 8 8 11 4.5-3 8-6 8-11V5z" opacity=".35"/><path d="M8 12h8M8 9h8M8 15h5"/></svg>
    <h3>Statically typed</h3>
    <p>Full inference, exhaustive pattern matching, nullable safety, no implicit <code>any</code>. Misconfiguration fails loud at startup — never with plausible nonsense at runtime.</p>
  </div>
</div>

## A first agent

Everything you need to triage and reply to email — model calls, human-in-the-loop, scheduling — in one file:

```keel
use std/ai
use std/email
use std/io
use std/schedule

type Urgency = low | medium | high | critical

agent EmailAssistant {
  @role "Professional email assistant"
  @tools [ai, io, email]

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

Four imports, one capability list, and the whole program is auditable at a glance: this agent can call models, talk to a human, and send email — and nothing else.

## Design Principles

1. **Small core, deep stdlib.** If a feature can be a library, it is one. The core language has a small reserved keyword set; everything else is a `std/` module imported with the same `use` syntax as your own files.
2. **Agents are primitives.** `agent` is the only concurrency model. Per-agent serial mailboxes with isolated mutable state via `self`.
3. **Capabilities are deny-by-default.** Effectful modules must be declared in `@tools` before an agent can use them. Undeclared calls are compile-time errors, not surprises in production.
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

> **Alpha.** Keel is pre-1.0. Semver is **not** respected between 0.x minor versions, and breaking changes can land in patch releases.

- **0.2.x** — current alpha. One module system (`use std/<name>` and local file imports), deny-by-default `@tools`, built-in `test` blocks.
- **0.x** — further pre-1.0 releases will keep breaking things where the design demands it. The [changelog](./changelog.md) flags every break.
- **1.0.x** — first API-stable release. Semver begins.

See [SPEC.md](https://github.com/keel-lang/keel/blob/main/SPEC.md) for the authoritative design and the [GitHub project board](https://github.com/orgs/keel-lang/projects/1) for planned work.
