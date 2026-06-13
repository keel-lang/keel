# Example: Multi-Agent Email System

This first runnable workflow keeps synchronous return-value helpers as top-level
tasks. For mailbox-specific coordination between live agents, use the built-in
`delegate`, `send`, and `broadcast` verbs as described in
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

task triage_email(msg: {body: str}) -> TriageResult {
  urgency  = ai.classify(msg.body, as: Urgency)  ?? Urgency.medium
  category = ai.classify(msg.body, as: Category) ?? Category.question
  {urgency: urgency, category: category}
}

task draft_reply(msg: {body: str, from: str}, guidance: str? = none) -> str {
  ai.draft("response to {msg.body}",
    tone: "professional",
    guidance: guidance,
    max_length: 200
  ) ?? "(draft failed)"
}

task plan_followup(msg: {subject: str}, urgency: Urgency) {
  when urgency {
    critical => schedule.after(2.hours, () => { io.notify("Follow up on: {msg.subject}") })
    high     => schedule.after(24.hours, () => { io.notify("Check status: {msg.subject}") })
    medium   => schedule.after(3.days, () => { io.notify("Pending reply: {msg.subject}") })
    low      => { }
  }
}

agent InboxManager {
  @tools [ai, email, io]
  @role "You coordinate the email handling team"

  task handle(msg: {body: str, from: str, subject: str}) {
    result = triage_email(msg)

    when result.urgency {
      low => {
        when result.category {
          spam, info => email.archive(msg)
          _ => {
            reply = draft_reply(msg)
            if io.confirm(reply) { email.send(reply, to: msg.from) }
          }
        }
      }
      medium => {
        reply = draft_reply(msg)
        if io.confirm(reply) { email.send(reply, to: msg.from) }
        plan_followup(msg, result.urgency)
      }
      high, critical => {
        summary = ai.summarize(msg.body, in: 2, unit: sentences) ?? "(no summary)"
        io.notify("{result.urgency} {result.category} from {msg.from}")
        io.show(summary)
        guidance = io.ask("How should I respond?")
        reply = draft_reply(msg, guidance)
        if io.confirm(reply) { email.send(reply, to: msg.from) }
        plan_followup(msg, result.urgency)
      }
    }
  }

  @on_start {
    schedule.every(5.minutes, () => {
      for msg in email.fetch(unread: true) {
        self.handle(msg)
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

Multi-agent collaboration is available with in-process mailboxes. `Agent.delegate`, `Agent.send`, `Agent.broadcast`, and `@team` routing are wired. Current limits: delivery is in-process only, broadcast is non-blocking, and agents without a matching handler silently ignore the event.
