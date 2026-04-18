# Example: Multi-Agent Email System

> **Alpha (v0.1).** Breaking changes expected. Multi-agent support (`Agent.delegate`, `Agent.broadcast`, `@team`) is **not yet implemented** — it's slated for a future release. The program below is a design preview.

A preview showing how multiple agents collaborate via `Agent.delegate` and the `@team` attribute.

```keel
type Urgency  = low | medium | high | critical
type Category = question | request | info | complaint | spam

type TriageResult {
  urgency: Urgency
  category: Category
}

agent Classifier {
  @role "You classify emails by urgency and category"

  task triage(email: {body: str}) -> TriageResult {
    urgency  = Ai.classify(email.body, as: Urgency,  fallback: Urgency.medium)
    category = Ai.classify(email.body, as: Category, fallback: Category.question)
    {urgency: urgency, category: category}
  }
}

agent Responder {
  @role "You draft professional, helpful email replies"

  task reply_to(email: {body: str, from: str}, guidance: str? = none) -> str {
    Ai.draft("response to {email.body}",
      tone: "professional",
      guidance: guidance,
      max_length: 200
    ) ?? "(draft failed)"
  }
}

agent FollowupScheduler {
  @role "You manage follow-ups and reminders"

  task plan(email: {subject: str}, urgency: Urgency) {
    when urgency {
      critical => Schedule.after(2.hours, () => { Io.notify("Follow up on: {email.subject}") })
      high     => Schedule.after(24.hours, () => { Io.notify("Check status: {email.subject}") })
      medium   => Schedule.after(3.days, () => { Io.notify("Pending reply: {email.subject}") })
      low      => { }
    }
  }
}

agent InboxManager {
  @role "You coordinate the email handling team"
  @team [Classifier, Responder, FollowupScheduler]

  task handle(email: {body: str, from: str, subject: str}) {
    result = Classifier.triage(email) ?? {
      urgency: Urgency.medium,
      category: Category.question
    }

    when result.urgency {
      low => {
        when result.category {
          spam, info => Email.archive(email)
          _ => {
            reply = Responder.reply_to(email) ?? "(could not draft)"
            if Io.confirm(reply) { Email.send(reply, to: email.from) }
          }
        }
      }
      medium => {
        reply = Responder.reply_to(email) ?? "(could not draft)"
        if Io.confirm(reply) { Email.send(reply, to: email.from) }
        FollowupScheduler.plan(email, result.urgency)
      }
      high, critical => {
        summary = Ai.summarize(email.body, in: 2, unit: sentences, fallback: "(no summary)")
        Io.notify("{result.urgency} {result.category} from {email.from}")
        Io.show(summary)
        guidance = Io.ask("How should I respond?")
        reply = Responder.reply_to(email, guidance) ?? "(could not draft)"
        if Io.confirm(reply) { Email.send(reply, to: email.from) }
        FollowupScheduler.plan(email, result.urgency)
      }
    }
  }

  @on_start {
    Schedule.every(5.minutes, () => {
      for email in Email.fetch(unread: true) {
        handle(email)
      }
    })
  }
}

run(InboxManager)
```

## Architecture

```
InboxManager (orchestrator)
  ├── Classifier           — fast model, triages urgency + category
  ├── Responder            — capable model, drafts quality replies
  └── FollowupScheduler    — manages follow-up reminders
```

Each agent has its own role, model, and mailbox. Calling an agent's task from another agent goes through the target agent's mailbox — that's what "delegation" means at runtime. `@team [...]` registers the set of peer agents this agent will delegate to.

## Status

Multi-agent collaboration is **not in v0.1**. The `@team` attribute, cross-agent task calls, and `Agent.broadcast` will land in a later release. Use this page as a design reference.
