# Example: Multi-Agent Email System

> **Alpha (v0.1).** Breaking changes expected. `Agent.delegate` is wired as of v0.1.4. `Agent.broadcast` and `@team` routing are wired as of v0.1.6.

This first runnable workflow keeps synchronous return-value helpers as top-level
tasks. For mailbox-specific coordination between live agents, use
`Agent.delegate`, `Agent.send`, and `Agent.broadcast` as described in
[Agent Communication](../guide/agent-communication.md).

```keel
use std/ai
use std/email
use std/io
use std/schedule

type Urgency  = low | medium | high | critical
type Category = question | request | info | complaint | spam

type TriageResult {
  urgency: Urgency
  category: Category
}

task triage_email(email: {body: str}) -> TriageResult {
  urgency  = ai.classify(email.body, as: Urgency)  ?? Urgency.medium
  category = ai.classify(email.body, as: Category) ?? Category.question
  {urgency: urgency, category: category}
}

task draft_reply(email: {body: str, from: str}, guidance: str? = none) -> str {
  ai.draft("response to {email.body}",
    tone: "professional",
    guidance: guidance,
    max_length: 200
  ) ?? "(draft failed)"
}

task plan_followup(email: {subject: str}, urgency: Urgency) {
  when urgency {
    critical => schedule.after(2.hours, () => { io.notify("Follow up on: {email.subject}") })
    high     => schedule.after(24.hours, () => { io.notify("Check status: {email.subject}") })
    medium   => schedule.after(3.days, () => { io.notify("Pending reply: {email.subject}") })
    low      => { }
  }
}

agent InboxManager {
  @tools [ai, email, io]
  @role "You coordinate the email handling team"

  task handle(email: {body: str, from: str, subject: str}) {
    result = triage_email(email) ?? {
      urgency: Urgency.medium,
      category: Category.question
    }

    when result.urgency {
      low => {
        when result.category {
          spam, info => email.archive(email)
          _ => {
            reply = draft_reply(email) ?? "(could not draft)"
            if io.confirm(reply) { email.send(reply, to: email.from) }
          }
        }
      }
      medium => {
        reply = draft_reply(email) ?? "(could not draft)"
        if io.confirm(reply) { email.send(reply, to: email.from) }
        plan_followup(email, result.urgency)
      }
      high, critical => {
        summary = ai.summarize(email.body, in: 2, unit: sentences) ?? "(no summary)"
        io.notify("{result.urgency} {result.category} from {email.from}")
        io.show(summary)
        guidance = io.ask("How should I respond?")
        reply = draft_reply(email, guidance) ?? "(could not draft)"
        if io.confirm(reply) { email.send(reply, to: email.from) }
        plan_followup(email, result.urgency)
      }
    }
  }

  @on_start {
    schedule.every(5.minutes, () => {
      for email in email.fetch(unread: true) {
        self.handle(email)
      }
    })
  }
}

run(InboxManager)
```

## Architecture

```
InboxManager (orchestrator agent)
  ├── triage_email(...)    — synchronous classifier helper
  ├── draft_reply(...)     — synchronous response helper
  └── plan_followup(...)   — synchronous scheduler helper
```

Use top-level tasks when the caller needs a return value immediately. Use
agents with mailboxes when work should cross a live actor boundary.
`delegate(target, task, args)` posts a named task event to the target
agent's mailbox. `@team [...]` tags a running agent with one or more team names
for `broadcast(team, data, event:)`.

```keel
use std/io

agent Classifier {
  @tools [io]
  @team ["email"]
  on refresh(msg: str) { io.show("Classifier refresh: {msg}") }
}

agent Coordinator {
  @on_start {
    run(Classifier)
    broadcast("email", "new batch", event: "refresh")
  }
}
```

## Status

Multi-agent collaboration is available in v0.1 with in-process mailboxes. `Agent.delegate`, `Agent.send`, `Agent.broadcast`, and `@team` routing are wired. Current limits: delivery is in-process only, broadcast is non-blocking, and agents without a matching handler silently ignore the event.
