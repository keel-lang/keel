# Example: Multi-Agent Email System

A v2 preview showing how multiple agents collaborate with `delegate` and `team`.

```keel
type Urgency  = low | medium | high | critical
type Category = question | request | info | complaint | spam

type TriageResult {
  urgency: Urgency
  category: Category
}

connect email via imap {
  host: env.IMAP_HOST,
  user: env.EMAIL_USER,
  pass: env.EMAIL_PASS
}

agent Classifier {
  role "You classify emails by urgency and category"
  model "claude-haiku"

  task triage(email: {body: str}) -> TriageResult {
    urgency  = classify email.body as Urgency fallback medium
    category = classify email.body as Category fallback question
    {urgency: urgency, category: category}
  }
}

agent Responder {
  role "You draft professional, helpful email replies"
  model "claude-sonnet"

  task reply_to(email: {body: str, from: str}, guidance: str? = none) -> str {
    draft "response to {email}" {
      tone: "professional",
      guidance: guidance,
      max_length: 200
    } ?? "(draft failed)"
  }
}

agent Scheduler {
  role "You manage follow-ups and reminders"
  model "claude-haiku"

  task plan_followup(email: {subject: str}, urgency: Urgency) {
    when urgency {
      critical => after 2.hours  { notify user "Follow up on: {email.subject}" }
      high     => after 24.hours { notify user "Check status: {email.subject}" }
      medium   => after 3.days   { notify user "Pending reply: {email.subject}" }
      low      => { }
    }
  }
}

agent InboxManager {
  role "You coordinate the email handling team"
  model "claude-sonnet"
  team [Classifier, Responder, Scheduler]

  task handle(email: {body: str, from: str, subject: str}) {
    result = delegate triage(email) to Classifier ?? {
      urgency: medium,
      category: question
    }

    when result.urgency {
      low => {
        when result.category {
          spam, info => archive email
          _ => {
            reply = delegate reply_to(email) to Responder ?? "(could not draft)"
            confirm user reply then send reply to email
          }
        }
      }
      medium => {
        reply = delegate reply_to(email) to Responder ?? "(could not draft)"
        confirm user reply then send reply to email
        delegate plan_followup(email, result.urgency) to Scheduler
      }
      high, critical => {
        summary = summarize email.body in 2 sentences fallback "(no summary)"
        notify user "{result.urgency} {result.category} from {email.from}"
        show user summary
        guidance = ask user "How should I respond?"
        reply = delegate reply_to(email, guidance) to Responder ?? "(could not draft)"
        confirm user reply then send reply to email
        delegate plan_followup(email, result.urgency) to Scheduler
      }
    }
  }

  every 5.minutes {
    emails = fetch email where unread
    for email in emails {
      handle(email)
    }
  }
}

run InboxManager
```

## Architecture

```
InboxManager (orchestrator)
  ├── Classifier  — fast model, triages urgency + category
  ├── Responder   — capable model, drafts quality replies
  └── Scheduler   — manages follow-up reminders
```

Each agent has its own role and model. The orchestrator delegates tasks to specialists.
