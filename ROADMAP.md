# Keel Roadmap

> From proof-of-concept to production-grade agent language — built in Rust from day one.

---

## Why Rust From the Start

Building a throwaway prototype in Python to "validate fast" means rewriting everything later. Instead, we invest upfront in the right foundation:

- **Single binary** — `keel run agent.keel` works out of the box, no Python/Node/JVM install
- **True concurrency** — agents need parallelism (fetch email while classifying another); Rust's async model (Tokio) handles this natively
- **Performance** — sub-second startup, minimal memory footprint, no GC pauses
- **Credibility** — a language built on Python will always feel like "just another framework"
- **No throwaway code** — every line written from v0.1 onward contributes to the real product

The Rust ecosystem already has excellent crates for everything we need: `logos` (lexer), `chumsky` (parser), `reqwest` (HTTP/LLM APIs), `tokio` (async runtime), `lettre` (email), `serde` (serialization).

---

## Overview

| Version | Codename | Focus | Status |
|---------|----------|-------|--------|
| **v0.9** | *Beta* | Interpreter, type checker, bytecode VM, full CLI + tooling, docs | **Current** |
| **v1.0** | *Stable* | Multi-agent, persistent memory, module system, ecosystem | Next |
| **v2.0** | *Scale* | LLVM native compilation, cloud deployment, visual IDE | Future |

---

## v0.9 — "Beta" (Current Release)

**Goal:** A complete, usable language for building single-agent AI workflows. Full interpreter, type checker, bytecode VM, CLI tooling, documentation, and editor support.

### Architecture

```
.keel file
    ↓
┌─────────┐    ┌──────────┐    ┌───────┐    ┌─────────────┐    ┌──────────────────┐
│  Lexer  │ →  │  Parser  │ →  │  AST  │ →  │    Type     │ →  │  Tree-Walking    │
│ (logos) │    │(chumsky) │    │       │    │   Checker   │    │  Interpreter     │
└─────────┘    └──────────┘    └───────┘    └─────────────┘    └──────────────────┘
                                                                        ↓
                                                       ┌────────────────────────────┐
                                                       │  Runtime Services          │
                                                       │  ├── LLM Client (reqwest)  │
                                                       │  ├── Email (lettre/imap)   │
                                                       │  ├── Scheduler (tokio)     │
                                                       │  └── Terminal I/O          │
                                                       └────────────────────────────┘
```

### Rust Project Structure

```
keel/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lexer/
│   │   └── mod.rs           # Token definitions + logos lexer
│   ├── parser/
│   │   └── mod.rs           # Grammar + AST generation
│   ├── ast/
│   │   └── mod.rs           # AST node types
│   ├── types/
│   │   ├── mod.rs           # Type definitions (structural, enum, tuple, nullable)
│   │   ├── checker.rs       # Type inference + checking pass
│   │   └── builtins.rs      # Built-in type signatures (AI primitives, collections)
│   ├── interpreter/
│   │   ├── mod.rs           # Tree-walking evaluator
│   │   ├── environment.rs   # Variable scoping
│   │   └── builtins.rs      # Built-in functions (classify, draft, etc.)
│   └── runtime/
│       ├── mod.rs           # Runtime orchestration
│       ├── llm.rs           # LLM API client (OpenAI, Anthropic)
│       ├── email.rs         # IMAP/SMTP integration
│       ├── scheduler.rs     # every/after/at implementation
│       └── human.rs         # ask/confirm/notify (terminal I/O)
├── examples/
│   └── *.keel               # Example Keel programs
└── tests/
    ├── lexer_tests.rs
    ├── parser_tests.rs
    ├── type_checker_tests.rs
    └── integration_tests.rs
```

### Core Syntax Supported

- `agent` declaration (role, model, tools)
- `task` definition and invocation
- `classify`, `draft`, `summarize` (routed to LLM APIs via `reqwest`)
- `ask user`, `confirm user`, `notify user` (terminal-based I/O)
- `fetch` (HTTP GET + IMAP email fetch)
- `send` (SMTP email send)
- `every` (async polling loop via Tokio)
- `if/else`, `when`, `for`
- String interpolation with `{}`, variables, basic types
- `connect` for email credentials (from env vars)
- **Type system:** full inference + checking (structural types, enums with associated data, nullable safety, tuples)
- **Error handling:** `try`/`catch` with ADT error variants, `retry`, `fallback`

### Key Rust Crates

| Crate | Purpose |
|-------|---------|
| `logos` | Fast lexer generation from token definitions |
| `chumsky` | Parser combinator library for readable grammars |
| `tokio` | Async runtime for concurrency + scheduling |
| `reqwest` | HTTP client for LLM API calls |
| `lettre` | SMTP email sending |
| `imap` | IMAP email fetching |
| `serde` + `serde_json` | JSON serialization for LLM responses |
| `clap` | CLI argument parsing |
| `miette` | Beautiful, user-friendly error diagnostics |

**PoC demo:**
- Email responder agent that classifies, auto-replies, and escalates
- Runs in terminal with human-in-the-loop prompts

**Deferred to v1.0:**
- Multi-agent orchestration (delegate works but runs locally)
- Persistent memory (remember/recall log to terminal, not persisted)
- Package/module system (`use`)
### Success Criteria
- [x] `cargo build` produces a single `keel` binary (5.2MB)
- [x] `keel run examples/email_agent.keel` executes the full PoC
- [x] Email fetch → classify → auto-reply or escalate workflow works end-to-end
- [x] Cold startup under 100ms (8ms actual)
- [x] Type errors caught at compile time: nullable access, type mismatch, non-exhaustive match
- [x] `keel check examples/email_agent.keel` reports clean types
- [x] Bytecode compiler and register-based VM (`keel build`)
- [x] REPL with expression evaluation (`keel repl`)
- [x] Auto-formatter (`keel fmt`)
- [x] Project scaffold (`keel init`)
- [x] LSP server with diagnostics, completion, hover (`keel lsp`)
- [x] VS Code extension with syntax highlighting
- [x] 19-page mdBook documentation
- [x] 11 example programs, 130 tests
- [ ] 10+ real-world workflows tested by users
- [ ] Community feedback from early adopters

---

## v1.0 — "Stable" (Multi-Agent & Memory)

**Goal:** Agents that collaborate, remember, and scale. Keel becomes a platform for building agent *systems*, not just individual agents.

### Language Features

**Multi-agent collaboration**
- `delegate task to Agent` — inter-agent task passing
- `team [Agent1, Agent2, Agent3]` — agent groups
- `broadcast message to team` — fan-out communication
- Agent discovery and dynamic routing
- Shared context between agents
- Message-passing concurrency (actor model on Tokio)

**Persistent memory**
- `remember` / `recall` / `forget` — semantic memory operations
- Automatic embedding and indexing (pluggable backends)
- Memory scoping: agent-level, team-level, global
- Memory decay and relevance scoring
- `recall similar to X` — semantic similarity search

**Guardrails & safety**
- `guardrails { ... }` block in agent definitions
- Cost limits per request/agent/day
- Content filtering rules
- Mandatory human approval for high-risk actions
- Audit logging for all AI decisions

**Advanced control flow**
- Parallel execution: `parallel { task1(), task2(), task3() }`
- Race conditions: `first_of { search(a), search(b) }`
- Conditional pipelines: `email -> classify -> when { ... }`
- Stream processing: `stream events |> filter |> process`

**Module system**
- `use keel/slack` — official connectors
- `use community/crm` — community packages
- Private modules and namespacing
- Version pinning

### Runtime Enhancements

**Agent orchestration**
- Actor-model message passing between agents (Tokio channels)
- Agent supervision trees (auto-restart on failure)
- Backpressure and flow control

**Memory backends (pluggable)**
- SQLite + embedded vectors (default, zero-config)
- Built-in embedding engine (local, via `candle` or `ort`)
- PostgreSQL + pgvector
- Pinecone, Weaviate, Qdrant

**Observability**
- Agent dashboard (web UI via embedded HTTP server)
- Token usage and cost tracking per agent
- Execution traces and decision logs
- OpenTelemetry integration for performance profiling

### Ecosystem

**Package registry: `keel.pkg`**
- Publish and install community packages
- Official connectors for major services
- Template agents (email assistant, support bot, researcher, etc.)

**Standard library**
- `keel/email` — IMAP/SMTP with smart parsing
- `keel/slack` — full Slack API
- `keel/notion` — Notion pages and databases
- `keel/http` — advanced HTTP client
- `keel/db` — database queries
- `keel/fs` — file system operations
- `keel/csv`, `keel/json`, `keel/pdf` — format handlers

### Success Criteria
- [ ] Multi-agent PoC: 3+ agents collaborating on a complex workflow
- [ ] Memory system handles 100K+ entries with sub-second recall
- [ ] Local embeddings run without external API dependency
- [ ] 20+ community packages published
- [ ] Agent dashboard used by 100+ teams

---

## v2.0 — "Scale" (Production & Cloud)

**Goal:** Keel is production-ready. Deploy agent swarms that handle real business workloads at scale, with visual tooling for non-programmers.

### Language Features

**Agent lifecycle management**
- Hot-reload agents without downtime
- Version pinning for agents in production
- Canary deployments: `deploy Agent v2 to 10% of traffic`
- Health checks and auto-restart

**Advanced patterns**
- Trait composition (preferred over inheritance): `agent Bot with [Logging, RateLimiting]`
- Agent templates: `agent Support from SupportTemplate { ... }` (override defaults, not class inheritance)
- Event sourcing: full replay of agent decisions
- Sagas: multi-step transactions with compensation

> **Design note:** Classical inheritance (`extends`) is deliberately avoided. Inheritance is the feature most often added to languages and most often regretted — it creates tight coupling, fragile base class problems, and diamond ambiguity. Keel agents compose behavior through `team` (delegation), `delegate` (task passing), shared tasks, and trait-style `with` mixins. These provide reuse without the inheritance tax.

**Domain-specific extensions**
- `keel/customer-support` — ticket handling, SLA tracking
- `keel/sales` — CRM, pipeline, outreach
- `keel/devops` — monitoring, incident response
- `keel/data` — ETL pipelines, data quality

### Platform

**Keel Cloud (optional hosted runtime)**
- Deploy agents with `keel deploy`
- Auto-scaling based on workload
- Global edge execution
- Managed memory and connections
- Pay-per-agent-minute pricing

**Keel Studio (Visual IDE)**
- Visual agent builder (drag-and-drop)
- Flow editor for pipelines
- Live agent monitoring
- Prompt playground for AI primitives
- Collaboration features for teams

**Keel Hub (Marketplace)**
- Pre-built agent templates
- Connector marketplace
- Enterprise agent sharing
- Certified agents with security audits

### Runtime

**Keel Runtime v2**
- Distributed execution across nodes
- Agent-level isolation (sandboxing)
- Zero-downtime deployments
- Built-in rate limiting and circuit breakers
- Multi-model support: route different AI primitives to different providers
- Local model support (Ollama, llama.cpp)

**Enterprise features**
- SSO and RBAC for agent management
- Compliance logging and data residency
- SLA monitoring for agent performance
- Encrypted memory and credential management
- On-premise deployment option

### Compiler: LLVM Backend

Add ahead-of-time compilation alongside the bytecode VM:

```
.keel → Lexer → Parser → AST → Keel IR → LLVM IR → Native Binary
```

- Compile Keel programs to native binaries via LLVM (using `inkwell` crate)
- Sub-millisecond cold start
- Minimal memory footprint
- Cross-compilation: Linux, macOS, Windows, ARM
- WASM target for browser-based agents

**CLI additions:**
```bash
keel compile agent.keel              # produce native binary
keel compile agent.keel --target wasm  # compile to WebAssembly
keel deploy agent.keel               # deploy to Keel Cloud
```

### Success Criteria
- [ ] 1000+ agents in production across paying customers
- [ ] Keel Studio used by non-technical users to build agents
- [ ] 99.9% uptime for Keel Cloud
- [ ] Native binary startup < 10ms
- [ ] WASM agents running in the browser
- [ ] Enterprise deployment at 3+ large companies
- [ ] Active open-source community with 500+ contributors

---

## Long-Term Vision

The ultimate goal is for Keel to become the **standard way** to define and deploy intelligent systems:

- **Self-improving agents** — agents that modify their own Keel source code based on performance feedback
- **Agent-to-agent protocols** — standard communication protocols between agents built by different teams
- **Federated agent networks** — cross-organization agent collaboration with privacy guarantees
- **Keel as the "SQL of AI"** — just as SQL became the universal language for data, Keel becomes the universal language for agents
- **Natural language → Keel** — describe what you want in plain English, get executable Keel code

---

## How to Get Involved

The Keel language is open-source from day one.

- **Star the repo** — show interest and track progress
- **Try the PoC** — build from source and run the email agent example
- **Write an agent** — push the boundaries of what Keel can express
- **Build a connector** — add support for your favorite service
- **Join the Discord** — discuss language design decisions

---

*The future of programming is declarative intelligence. Let's build it together.*
