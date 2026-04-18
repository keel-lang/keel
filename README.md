<p align="center">
  <img src="assets/logos/keel-wordmark.svg" alt="Keel" width="280"/>
  <br/>
  <strong>A programming language where AI agents are first-class citizens.</strong>
</p>

<p align="center">
  <em>v0.1 — alpha. Not production-ready. Breaking changes expected between 0.x releases.</em>
</p>

---

## Status: Alpha (v0.1.0)

Keel is in **early design and implementation**. There are **no production users** and no stable API. The language and standard library will change — including in ways that break existing `.keel` files — across upcoming 0.x releases.

If you're here, you're either curious or contributing. Both are welcome. Neither is shipping to prod.

- **Roadmap:** [ROADMAP.md](ROADMAP.md)
- **Spec:** [SPEC.md](SPEC.md)
- **Changelog:** [CHANGELOG.md](CHANGELOG.md)

---

## The Idea

Building an AI agent today means stitching together frameworks on top of languages that were never designed for autonomous systems. Keel is a small language where the actor model is the only concurrency primitive, and everything else — AI, scheduling, HTTP, email, memory, human I/O — lives in a **standard library that is auto-imported**. You never write `use keel/ai`. You just write `Ai.classify(...)` and it works.

```keel
type Urgency = low | medium | high | critical

agent EmailBot {
  @role "Professional email triage"

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

run(EmailBot)
```

Zero imports. The `Ai`, `Io`, `Email`, `Schedule` namespaces are in scope from the start.

---

## Design in One Paragraph

The core language ships **27 keywords** and the actor model. Everything else is a stdlib function call routed through an **interface** that users can swap. LLM providers, memory stores, HTTP clients, schedulers, and loggers are all replaceable without leaving the language. Reserved keyword inflation is the enemy. See [SPEC.md §0–§3](SPEC.md) for the design and [SPEC.md §10](SPEC.md) for the full keyword list.

---

## Install

Three options, from easiest to most manual:

```bash
# 1. One-liner installer (macOS / Linux, any arch)
curl -sSf https://keel-lang.dev/install.sh | sh

# 2. Homebrew (macOS / Linux)
brew install --formula https://keel-lang.dev/keel.rb

# 3. From source
git clone https://github.com/keel-lang/keel.git
cd keel && cargo build --release
./target/release/keel --version
```

The installer and Homebrew formula point at the latest GitHub release. If no release exists yet for your tag, they fall back to build-from-source instructions.

---

## Quick Start

```bash
# Install Ollama, pull a model, and point Keel at it
ollama pull gemma4
export KEEL_OLLAMA_MODEL=gemma4

# Run an example
./target/release/keel run examples/minimal.keel
```

---

## What's Different

| | Typical Python + LangChain | Keel |
|---|---|---|
| Classify an email | Parser + prompt template + chain | `Ai.classify(body, as: Urgency)` |
| Ask a human | `input()` + manual formatting | `Io.ask("How to respond?")` |
| Schedule a check | `schedule` library + while loop | `Schedule.every(5.minutes, () => { ... })` |
| Send email | SMTP config + lettre-style setup | `Email.send(reply, to: addr)` |
| Type safety | none at compile time | exhaustive match checking, null safety |
| Imports needed | 10+ | 0 |

The zero-import story comes from the **prelude**: stdlib namespaces are auto-imported into every file. The compiler doesn't know what `Ai` is — the runtime installs it. Users can swap any namespace's implementation by installing a different interface implementation at startup.

---

## CLI

```
keel run agent.keel       Execute a program
keel check agent.keel     Type-check without running
keel build agent.keel     Compile to bytecode (.keelc)
keel fmt agent.keel       Auto-format
keel init my-project      Scaffold a new project
keel repl                 Interactive REPL
keel lsp                  Language server (stdin/stdout)
```

---

## LLM Provider

Keel v0.1 ships with a single backend: **Ollama** (local, offline). It implements the `LlmProvider` interface; users can install custom implementations. No silent fallbacks — if a model isn't configured, you get a clear error.

```bash
# Required: Ollama running locally with a pulled model
export KEEL_OLLAMA_MODEL=gemma4

# Optional: per-alias mapping
export KEEL_MODEL_FAST=gemma4
export KEEL_MODEL_SMART=mistral:7b-instruct
```

---

## Editor Support

VS Code extension with syntax highlighting and LSP:

```bash
cd editors/vscode
code --install-extension .
```

The LSP provides diagnostics, autocomplete, and hover. Refactoring and inlay hints are on the roadmap.

---

## Documentation

```bash
cd docs && mdbook serve
# opens at http://localhost:3000
```

---

## Versioning and Breaking Changes

Keel is in alpha. Semver is **not** respected between 0.x minor versions.

- **0.1.x** — current alpha. API is unstable; breaking changes in patch releases are allowed.
- **0.2.x and later** — scoped after v0.1 ships. See [ROADMAP.md](ROADMAP.md).
- **1.0.x** — first API-stable release. Semver begins.

Do not write anything you're not willing to rewrite.

---

## Contributing

Issues and PRs welcome. The spec ([SPEC.md](SPEC.md)) is the source of truth — if the implementation diverges, the spec wins unless the divergence is better, in which case the spec updates.

---

## License

MIT
