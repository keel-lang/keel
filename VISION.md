# Keel — Vision

> *What if languages were designed today, knowing that AI is the runtime?*

> **Alpha notice.** Keel is v0.1 — alpha. No production users. Breaking changes expected between 0.x releases. This document describes the destination; [SPEC.md](SPEC.md) and [ROADMAP.md](ROADMAP.md) describe where we are on the way there.

---

## The Problem

Building AI agents today means writing hundreds of lines of Python to express what should take ten. The hard parts — LLM calls, scheduling, human-in-the-loop, memory — are framework concerns, which means they're imported, configured, wrapped, and stitched together. The easy parts — types, pattern matching, exhaustiveness — are rare in the ecosystems where agents live.

Keel collapses the stack: the actor model is the one concurrency primitive, and everything else lives in a **standard library that is auto-imported**. You never write `use keel/ai`. You write `Ai.classify(...)` and the prelude makes it work.

The win isn't novel syntax. It's that the language does less, the stdlib does more, and the interface between them is small enough to reason about.

---

## Two Design Decisions That Define Keel

### 1. The actor model is the core. Everything else is a library.

`agent` is the primitive. It owns a mailbox, serial handler execution, and isolated mutable state accessible only via `self`. That's the single concurrency model. AI calls, scheduling, I/O, HTTP, memory, search — none of those are language features. They're functions in namespaces the runtime installs into scope.

This is what earns keeping the core small: there is almost nothing that can't be expressed as a library given the actor model + structured concurrency + interfaces.

### 2. The standard library is the prelude.

Every namespace in the stdlib is auto-imported. Users get keyword-feel ergonomics (`Ai.classify(...)`) without the compiler having to know about `classify`. That means parser, lexer, type checker, and LSP stay free of feature-specific special cases, and stdlib implementations are swappable without leaving the language.

If `Ai.classify` were a keyword, a user who wanted a different LLM would need a fork, a compile flag, or a monkey patch. With `Ai.classify` as a function behind an `LlmProvider` interface, they install their own implementation at startup. The language doesn't care.

---

## What Keel Is (v0.1)

A small, statically-typed language for writing AI agents:

```keel
type Urgency = low | medium | high | critical

agent EmailBot {
  @role "Professional email triage"

  on message(msg: Message) {
    urgency = Ai.classify(msg.body, as: Urgency, fallback: Urgency.medium)
    when urgency {
      low, medium    => auto_reply(msg)
      high, critical => escalate(msg)
    }
  }
}

run(EmailBot)
```

- **22 keywords.** If a word isn't reserved, it's an identifier. Namespaces, duration units, attribute names — all identifiers.
- **Full type inference.** Every expression has a type. Mismatches are compile errors. Annotations are rarely needed.
- **Exhaustive pattern matching.** Missing an enum variant is a compile error, not a runtime surprise.
- **Nullable safety.** Operations that can fail return nullable types. The caller handles failure explicitly.
- **Structural concurrency.** `Async.spawn`, `Async.join_all`, `Async.select`. Parent cancels children. Simple and sufficient.

---

## What Keel Is Not

- **Not a framework.** No DSL inside another language. No ORM-style wrappers. Keel is a compiler and a runtime.
- **Not a replacement for general-purpose languages.** Use Keel when you're building agents. Use Go, Rust, Python for the rest.
- **Not magical.** The prelude is just functions in scope. The interface system is structural types. The runtime is Tokio. Nothing here is secret sauce.
- **Not stable.** v0.1 is alpha. We will break things. Building a startup on current Keel would be a mistake.

---

## Who It's For

1. **AI engineers** who are tired of LangChain boilerplate and want type safety at compile time.
2. **Rust engineers** who want to write high-level agent code without leaving the Rust ecosystem.
3. **Tool builders** who want a clean target for their own LLM providers, memory stores, or scheduler backends — the interface system is an invitation.
4. **Language designers** who want to watch a small language grow in public with the core/stdlib line drawn deliberately.

---

## What Success Looks Like

At v1.0:

- The keyword list is still 22 words.
- A new LLM provider is a 200-line PR against a stdlib crate, not a language change.
- A new memory backend is the same.
- The `.keel` file for a three-agent workflow fits on one screen.
- The IDE provides working autocomplete, go-to-def, rename, and diagnostics.
- The compiler catches the bug before the LLM call burns a dollar.
- Breaking changes go to zero per release.

At v2.0+:

- Agent systems built in Keel run in production at companies that care about reliability.
- Keel is embeddable in existing Rust, Python, and Node services via the plugin ABI.
- A visible community publishes interfaces, providers, and ready-to-use agents.

---

## What We Reject

- **Silent fallbacks.** If the model isn't configured, we don't substitute a mock and return plausible nonsense. We fail loud with an actionable error.
- **Keyword inflation.** Every keyword is a permanent parser special case. We resist them aggressively. Sugar goes to the library.
- **Inheritance.** Agents compose via delegation, shared tasks, and trait-style mixins. Classical inheritance, with its fragile base class problem and diamond ambiguity, stays out.
- **"Dynamic by default."** Untyped escape hatches exist (`dynamic`, `extern`, `Ai.prompt as dynamic`) but must always be opted into explicitly. You cannot accidentally end up in `dynamic` land.
- **Framework creep in the core.** If it can be a library, it is.

---

## The Pitch, Short Version

Keel is a small language where AI agents are the concurrency primitive and the standard library is the prelude. Write less code, catch more errors at compile time, swap implementations without forking, and build systems that read like what they do.

Status: alpha. Come break it.
