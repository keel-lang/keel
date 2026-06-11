# Examples

> **Alpha (v0.1).** These examples use the module-based surface described in [SPEC.md](../SPEC.md): the stdlib is imported with `use std/<name>`.

## Files

| File | What it demonstrates |
|---|---|
| [`inbox_modules/`](inbox_modules/) | Multi-file program: `use std/<name>` + local imports, implicit main, per-file tests (`validation_test.keel`) |
| [`hello_world.keel`](hello_world.keel) | Message handler + `ai.draft` |
| [`email_agent.keel`](email_agent.keel) | Full email triage + auto-reply agent |
| [`customer_support.keel`](customer_support.keel) | Ticket classification + escalation |
| [`code_reviewer.keel`](code_reviewer.keel) | PR risk assessment |
| [`data_pipeline.keel`](data_pipeline.keel) | Collection operations, lambdas, validation |
| [`daily_digest.keel`](daily_digest.keel) | Morning briefing from email |
| [`meeting_prep.keel`](meeting_prep.keel) | Meeting context + briefing notes |
| [`agent_delegation.keel`](agent_delegation.keel) | Multi-agent collaboration with `delegate` |
| [`broadcast_team.keel`](broadcast_team.keel) | Team routing with `@team` and `broadcast` |
| [`multi_agent_inbox.keel`](multi_agent_inbox.keel) | Multi-agent inbox orchestration sketch |
| [`http_demo.keel`](http_demo.keel) | `http.get` against a public endpoint |
| [`at_demo.keel`](at_demo.keel) | `schedule.at` with an ISO 8601 datetime |
| [`cron_schedule.keel`](cron_schedule.keel) | `schedule.cron` with 5-field cron expressions |
| [`rich_enum.keel`](rich_enum.keel) | Rich enum variants: construction + destructuring |
| [`interfaces.keel`](interfaces.keel) | User-defined interfaces + all four built-in interfaces (`Comparable`, `Equatable`, `Serializable`, `Iterable`) |
| [`stringable.keel`](stringable.keel) | `impl Stringable for T` — custom string interpolation for user-defined types |
| [`while_loop.keel`](while_loop.keel) | `while` loops with `break` and `continue` |
| [`subscript_access.keel`](subscript_access.keel) | List and string subscript syntax (`list[i]`, `str[i]`) |

## Common conventions

- Every file imports the **stdlib modules** it uses: `use std/ai`, `use std/io`, `use std/email`, … Local files import each other with `use "./file.keel"` — see [`inbox_modules/`](inbox_modules/) for a multi-file program.
- Agent metadata uses `@attributes`: `@role`, `@model`, `@tools`, `@memory`, etc.
- Scheduled work lives inside `@on_start` with a `schedule.every` / `schedule.after` call.
- AI primitives are function calls: `ai.classify(x, as: T) ?? default`.
- Email, HTTP, and memory are library modules: `email.fetch`, `email.send`, `memory.remember`.
