<p align="center">
  <img src="assets/logos/keel-wordmark.svg" alt="Keel" width="280"/>
  <br/>
  <strong>Build AI agents in 40 lines, not 180.</strong>
</p>

Keel is a programming language where AI agents are first-class citizens. Classify emails, draft replies, and schedule workflows — all with built-in keywords, not library imports.

```keel
type Urgency = low | medium | high | critical

agent EmailBot {
  role "Professional email assistant"
  model "claude-sonnet"

  every 5.minutes {
    for email in fetch inbox where unread {
      urgency = classify email.body as Urgency fallback medium

      when urgency {
        low, medium => {
          reply = draft "response to {email}" { tone: "friendly" }
          confirm user reply then send reply to email
        }
        high, critical => {
          notify user "{urgency}: {email.subject}"
          guidance = ask user "How to respond?"
          reply = draft "response to {email}" { guidance: guidance }
          confirm user reply then send reply to email
        }
      }
    }
  }
}

run EmailBot
```

The same agent in Python + LangChain: [180+ lines](examples/comparison_python_vs_keel.md).

## Install

```bash
# One-liner (macOS / Linux)
curl -sSf https://keel-lang.dev/install.sh | sh

# Homebrew
brew install keel-lang/tap/keel

# From source
git clone https://github.com/keel-lang/keel.git && cd keel && cargo build --release
```

## Quick Start

```bash
# Set up a local LLM (no API key needed)
ollama pull gemma4
export KEEL_OLLAMA_MODEL=gemma4

# Run your first agent
keel run examples/minimal.keel
```

Or with the Anthropic API:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
./target/release/keel run examples/minimal.keel
```

## Why Keel?

| | Python + LangChain | Keel |
|---|---|---|
| **Lines for an email agent** | 180+ | 40 |
| **Imports needed** | 12 | 0 |
| **Classify an email** | chain + parser + prompt template | `classify email.body as Urgency` |
| **Ask a human** | `input()` + manual formatting | `ask user "How to respond?"` |
| **Schedule a check** | `schedule` library + while loop | `every 5.minutes { ... }` |
| **Type safety** | none | exhaustive match checking at compile time |
| **Time to understand** | 5+ minutes | 30 seconds |

## Features

**AI is built in** — not imported
```keel
classify text as Mood fallback neutral          # classification
summarize article in 2 sentences                # summarization
draft "reply to {email}" { tone: "formal" }     # text generation
extract { name: str, date: str } from document  # structured extraction
translate message to french                     # translation
```

**Type-safe with zero annotations**
```keel
type Priority = low | medium | high | critical

when priority {
  low, medium => auto_reply(email)
  high        => escalate(email)
  # Compile error: missing "critical"
}
```

**Humans in the loop**
```keel
answer = ask user "How should I respond?"
confirm user draft_reply then send draft_reply to email
notify user "3 new emails classified"
```

**Real connections**
```keel
connect inbox via imap {
  host: env.IMAP_HOST,
  user: env.EMAIL_USER,
  pass: env.EMAIL_PASS
}

emails = fetch inbox where unread    # real IMAP
send reply to email                  # real SMTP
response = fetch "https://api.example.com/data"  # real HTTP
```

**Lambdas and collection methods**
```keel
urgent = emails.filter(e => triage(e) == critical)
names = contacts.map(c => c.name).sort_by(n => n)
has_unread = emails.any(e => e.unread)
```

## CLI

```
keel run agent.keel       Execute a program
keel check agent.keel     Type-check without running
keel build agent.keel     Compile to bytecode
keel fmt agent.keel       Auto-format
keel init my-project      Scaffold a new project
keel repl                 Interactive REPL
keel lsp                  Language server (for editors)
```

## LLM Providers

Keel works with **Ollama** (local, free) or **Anthropic Claude** (API). No silent fallbacks — if a model isn't configured, you get a clear error with the exact `export` command to fix it.

```bash
# Local (Ollama)
export KEEL_OLLAMA_MODEL=gemma4

# Cloud (Anthropic)
export ANTHROPIC_API_KEY=sk-ant-...

# Per-model mapping (fast model for classify, capable for draft)
export KEEL_MODEL_CLAUDE_HAIKU=gemma4
export KEEL_MODEL_CLAUDE_SONNET=mistral:7b-instruct
```

## Examples

| Example | What it does |
|---------|-------------|
| [minimal.keel](examples/minimal.keel) | Hello world — tasks, state, notify |
| [email_agent.keel](examples/email_agent.keel) | Email triage + auto-reply |
| [customer_support.keel](examples/customer_support.keel) | Ticket classification + escalation |
| [code_reviewer.keel](examples/code_reviewer.keel) | PR risk assessment |
| [data_pipeline.keel](examples/data_pipeline.keel) | Validation, lambdas, collection ops |
| [daily_digest.keel](examples/daily_digest.keel) | Morning briefing from email |
| [meeting_prep.keel](examples/meeting_prep.keel) | Briefing notes for meetings |
| [multi_agent_inbox.keel](examples/multi_agent_inbox.keel) | Multi-agent collaboration (preview) |

## Editor Support

**VS Code** — syntax highlighting with the bundled extension:

```bash
cd editors/vscode
code --install-extension .
```

The LSP server provides real-time diagnostics, autocomplete, and hover info.

## Documentation

```bash
cd docs && mdbook serve
# Opens at http://localhost:3000
```

19 pages covering installation, language guide, CLI reference, and configuration.

## Status

**v0.9 (beta)** — the language works end-to-end. 130 tests, 8ms cold startup, 11 example programs.

What's next for v1.0: multi-agent orchestration, persistent memory, module system. See [ROADMAP.md](ROADMAP.md).

## License

MIT
