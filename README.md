<p align="center">
  <img src="brand/svg/keel-primary.svg" alt="Keel" width="120"/>
</p>

<h1 align="center">Keel</h1>

<p align="center">
  <strong>A programming language where AI agents are first-class citizens.</strong>
</p>

<p align="center">
  <em>v0.1 — alpha. Not production-ready. Breaking changes expected between 0.x releases.</em>
</p>

---

## Status: Alpha (v0.1)

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

The core language has a small reserved keyword set and the actor model. Everything else is a stdlib function call behind a planned **interface** boundary. v0.1 ships that boundary in the design and uses Ollama as the only LLM backend; runtime provider swapping is still planned. Reserved keyword inflation is the enemy. See [SPEC.md §0–§3](SPEC.md) for the design, [ROADMAP.md](ROADMAP.md) for shipped status, and [SPEC.md §10](SPEC.md) for the full keyword list.

---

## Install

Three options:

```bash
# 1. Homebrew (macOS / Linux)
brew install keel-lang/tap/keel

# 2. One-liner installer (macOS / Linux, any arch)
curl -sSf https://keel-lang.dev/install.sh | sh

# 3. From source
git clone https://github.com/keel-lang/keel.git
cd keel && cargo build --release
./target/release/keel --version
```

The tap and the installer both pull the latest GitHub release. The source path is the most direct fallback if a packaged install fails.

---

## Quick Start

```bash
# Install Ollama, pull a model, and point Keel at it
ollama pull gemma4
export KEEL_OLLAMA_MODEL=gemma4

# Run an example
./target/release/keel run examples/hello_world.keel
```

---

## What's Different

| | Typical Python + LangChain | Keel |
|---|---|---|
| Classify an email | Parser + prompt template + chain | `Ai.classify(body, as: Urgency)` |
| Ask a human | `input()` + manual formatting | `Io.ask("How to respond?")` |
| Schedule a check | `schedule` library + while loop | `Schedule.every(5.minutes, () => { ... })` |
| Send email | SMTP config + lettre-style setup | `Email.send(reply, to: addr)` |
| Type safety | none at compile time | exhaustive match checking, `T?` nullable types |
| Imports needed | 10+ | 0 |

The zero-import story comes from the **prelude**: stdlib namespaces are auto-imported into every file. The compiler doesn't know what `Ai` is — the runtime installs it. The interface model is the intended extension point; v0.1 exposes model alias selection through `using:` and `KEEL_MODEL_*`, while custom provider installation is still planned.

---

## CLI

```
keel run agent.keel       Execute a program
keel check agent.keel     Type-check without running
keel fmt agent.keel       Auto-format
keel init my-project      Scaffold a new project
keel repl                 Interactive REPL
keel lint agent.keel      Static analysis; --fix flag
keel lsp                  Language server (stdin/stdout)
keel build agent.keel     Deferred: bytecode compiler post-v0.1
```

---

## LLM Provider

Keel v0.1 ships with a single backend: **Ollama** (local, offline). It follows the planned `LlmProvider` interface shape, but custom provider installation is not wired yet. No silent fallbacks — if a model isn't configured, you get a clear error.

```bash
# Required: Ollama running locally with a pulled model
export KEEL_OLLAMA_MODEL=gemma4

# Optional: per-alias mapping
export KEEL_MODEL_FAST=gemma4
export KEEL_MODEL_SMART=mistral:7b-instruct
```

---

## Editor Support

VS Code extension with syntax highlighting and LSP — maintained in its own repository:

**[github.com/keel-lang/vscode-keel](https://github.com/keel-lang/vscode-keel)**

```bash
git clone https://github.com/keel-lang/vscode-keel
cd vscode-keel
code --install-extension keel-lang-*.vsix
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

Issues and PRs welcome. Document roles are explicit: [SPEC.md](SPEC.md) describes the language design target, [ROADMAP.md](ROADMAP.md) tracks shipped/partial/planned status, and the README plus mdBook describe current user-facing behavior.

---

## License

MIT
