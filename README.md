<p align="center">
  <img src="brand/svg/keel-primary.svg" alt="Keel" width="120"/>
</p>

<h1 align="center">Keel</h1>

<p align="center">
  <strong>A programming language where AI agents are first-class citizens.</strong>
</p>

<p align="center">
  <em>v0.2 — alpha. Not production-ready. Breaking changes expected between 0.x releases.</em>
</p>

---

## Status: Alpha (v0.2)

Keel is in **early design and implementation**. There are **no production users** and no stable API. The language and standard library will change — including in ways that break existing `.keel` files — across upcoming 0.x releases.

If you're here, you're either curious or contributing. Both are welcome. Neither is shipping to prod.

- **Roadmap:** [ROADMAP.md](ROADMAP.md)
- **Spec:** [SPEC.md](SPEC.md)
- **Changelog:** [CHANGELOG.md](CHANGELOG.md)

---

## The Idea

Building an AI agent today means stitching together frameworks on top of languages that were never designed for autonomous systems. Keel is a small language where the actor model is the only concurrency primitive, and everything else — AI, scheduling, HTTP, email, memory, human I/O — lives in a **standard library**. Import what you need with `use std/ai`, `use std/email`, … and call it directly — `ai.classify(...)`, `ai.draft(...)`.

```keel
use std/ai
use std/email
use std/io
use std/schedule

type Urgency = low | medium | high | critical

task triage(email: { body: str, from: str, subject: str }) -> Urgency {
  # `??` covers a `none` result (model unavailable). A real model can return
  # text that matches no variant, which raises AiSchemaError — caught here so
  # one bad email can't abort the batch. `??` alone does not catch it.
  try {
    ai.classify(email.body, as: Urgency) ?? Urgency.medium
  } catch err: AiSchemaError {
    Urgency.medium
  }
}

agent EmailBot {
  @tools [ai, email, io]
  @role "Professional email triage"

  task handle(email: { body: str, from: str, subject: str }) {
    urgency = triage(email)
    when urgency {
      low, medium => {
        reply = ai.draft("response to {email.body}", tone: "friendly") ?? "(draft failed)"
        if io.confirm(reply) { email.send(reply, to: email.from) }
      }
      high, critical => {
        io.notify("{urgency}: {email.subject}")
        guidance = io.ask("How should I respond?")
        reply = ai.draft("response to {email.body}", guidance: guidance) ?? "(draft failed)"
        if io.confirm(reply) { email.send(reply, to: email.from) }
      }
    }
  }

  @on_start {
    schedule.every(5.minutes, () => {
      emails = email.fetch(unread: true)
      for email in emails {
        self.handle(email)
      }
    })
  }
}

run(EmailBot)
```

Each file imports the stdlib modules it uses — `use std/ai`, `use std/email`, … — and every agent declares which of those it may call through deny-by-default `@tools`. Namespaces are lowercase: `ai.*`, `io.*`, `email.*`, `schedule.*`.

---

## Design in One Paragraph

The core language has a small reserved keyword set and the actor model. Everything else is a stdlib function call behind an **interface** boundary. `ai.*` dispatches through that boundary to one of three built-in backends — Ollama (local default), OpenAI, and Anthropic — or a provider you write in Keel with `impl LlmProvider`. The compiler knows a namespace's shape; the runtime installs the backend. Reserved keyword inflation is the enemy. See [SPEC.md §0–§3](SPEC.md) for the design, [ROADMAP.md](ROADMAP.md) for shipped status, and [SPEC.md §10](SPEC.md) for the full keyword list.

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
| Classify an email | Parser + prompt template + chain | `ai.classify(body, as: Urgency)` |
| Ask a human | `input()` + manual formatting | `io.ask("How to respond?")` |
| Schedule a check | `schedule` library + while loop | `schedule.every(5.minutes, () => { ... })` |
| Send email | SMTP config + lettre-style setup | `email.send(reply, to: addr)` |
| Swap LLM backend | rewrite SDK calls | `@provider anthropic` or `KEEL_PROVIDER` |
| Tool access | implicit, ungoverned | deny-by-default `@tools [ai, email]` |
| Type safety | none at compile time | exhaustive match checking, nullable (`T?`) safety, return-type checks |

`ai.*` dispatches through a swappable provider interface — the compiler knows the namespace's shape, the runtime installs the backend (Ollama, OpenAI, Anthropic, or one you write). Per-call model selection goes through `using:` and `KEEL_MODEL_*`; provider selection through `@provider`, a `provider:` model-tag prefix, or `KEEL_PROVIDER`.

---

## CLI

```
keel run agent.keel       Execute a program
keel test agent.keel      Run the program's test blocks
keel check agent.keel     Type-check without running (--strict rejects unresolved types)
keel fmt agent.keel       Auto-format
keel lint agent.keel      Style and best-practice checks (--fix applies safe fixes)
keel init my-project      Scaffold a new project
keel repl                 Interactive REPL
keel lsp                  Language server (stdin/stdout)
keel build agent.keel     Deferred: bytecode compiler post-v0.1
```

Global flags: `--trace` narrates LLM calls; `--log-level debug|info|warn|error` sets the `Log.*` threshold.

---

## LLM Provider

`ai.*` dispatches through swappable backends. Three ship built in — **Ollama**
(default, local), **OpenAI**, and **Anthropic (Claude)** — selected with no extra
code: a `provider:` prefix on the model tag (per call), `@provider` (per agent),
or `KEEL_PROVIDER` (per program). For proprietary or self-hosted models, write a
provider in Keel (`impl LlmProvider` + `ai.install`/`@provider`). No silent
fallbacks — a missing key or unmapped model throws a clear `AiError`.

```bash
# Ollama (default): a local daemon with a pulled model
export KEEL_OLLAMA_MODEL=gemma4
export KEEL_MODEL_FAST=gemma4              # optional per-alias mapping

# OpenAI / Anthropic: select the backend and supply a key
export KEEL_PROVIDER=anthropic
export ANTHROPIC_API_KEY=sk-...
```

See [docs: LLM Providers](docs/src/config/llm-providers.md) for user-authored
providers and the full precedence rules.

---

## Editor Support

VS Code extension with syntax highlighting and LSP — maintained in its own repository:

**[github.com/keel-lang/vscode-keel](https://github.com/keel-lang/vscode-keel)**

```bash
git clone https://github.com/keel-lang/vscode-keel
cd vscode-keel
code --install-extension keel-lang-*.vsix
```

The LSP provides diagnostics, completion, hover, and go-to-definition. Refactoring and inlay hints are on the roadmap.

---

## Documentation

```bash
cd docs && mdbook serve
# opens at http://localhost:3000
```

---

## Versioning and Breaking Changes

Keel is in alpha. Semver is **not** respected between 0.x minor versions.

- **0.2.x** — current alpha. One module system (`use std/<name>` and local file imports), deny-by-default `@tools`, built-in `test` blocks, swappable LLM providers. API is unstable; breaking changes in patch releases are allowed.
- **0.x** — further pre-1.0 releases will keep breaking things where the design demands it. The [changelog](CHANGELOG.md) flags every break. See [ROADMAP.md](ROADMAP.md).
- **1.0.x** — first API-stable release. Semver begins.

Do not write anything you're not willing to rewrite.

---

## Contributing

Issues and PRs welcome. Document roles are explicit: [SPEC.md](SPEC.md) describes the language design target, [ROADMAP.md](ROADMAP.md) tracks shipped/partial/planned status, and the README plus mdBook describe current user-facing behavior.

---

## License

MIT
