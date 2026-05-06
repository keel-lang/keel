# Examples

> **Alpha (v0.1).** These examples use the v0.1 prelude-based surface described in [SPEC.md](../SPEC.md).

## Files

| File | What it demonstrates |
|---|---|
| [`hello_world.keel`](hello_world.keel) | Message handler + `Ai.draft` |
| [`email_agent.keel`](email_agent.keel) | Full email triage + auto-reply agent |
| [`customer_support.keel`](customer_support.keel) | Ticket classification + escalation |
| [`code_reviewer.keel`](code_reviewer.keel) | PR risk assessment |
| [`data_pipeline.keel`](data_pipeline.keel) | Collection operations, lambdas, validation |
| [`daily_digest.keel`](daily_digest.keel) | Morning briefing from email |
| [`meeting_prep.keel`](meeting_prep.keel) | Meeting context + briefing notes |
| [`agent_delegation.keel`](agent_delegation.keel) | Multi-agent collaboration with `Agent.delegate` |
| [`broadcast_team.keel`](broadcast_team.keel) | Team routing with `@team` and `Agent.broadcast` |
| [`multi_agent_inbox.keel`](multi_agent_inbox.keel) | Multi-agent inbox orchestration sketch |
| [`http_demo.keel`](http_demo.keel) | `Http.get` against a public endpoint |
| [`at_demo.keel`](at_demo.keel) | `Schedule.at` with an ISO 8601 datetime |
| [`cron_schedule.keel`](cron_schedule.keel) | `Schedule.cron` with 5-field cron expressions |
| [`rich_enum.keel`](rich_enum.keel) | Rich enum variants: construction + destructuring |

## Common conventions

- Every file uses the **prelude**: `Ai`, `Io`, `Email`, `Http`, `Schedule`, `Memory` are in scope without imports.
- Agent metadata uses `@attributes`: `@role`, `@model`, `@tools`, `@memory`, etc.
- Scheduled work lives inside `@on_start` with a `Schedule.every` / `Schedule.after` call.
- AI primitives are function calls: `Ai.classify(x, as: T, fallback: V)`.
- Email, HTTP, and memory are library modules: `Email.fetch`, `Email.send`, `Memory.remember`.
