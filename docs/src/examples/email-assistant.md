# Example: Email Assistant

> **Alpha (v0.1).** Breaking changes expected.

A complete email agent that triages, auto-replies, and escalates.

```keel
use std/ai
use std/email
use std/io
use std/memory
use std/schedule

type Urgency = low | medium | high | critical

task triage(email: {body: str, from: str, subject: str}) -> Urgency {
  ai.classify(email.body,
    as: Urgency,
    considering: {
      "from a known VIP or executive":   Urgency.critical,
      "mentions a deadline within 24h":  Urgency.high,
      "asks a direct question":          Urgency.medium,
      "newsletter or automated message": Urgency.low
    },
    using: "fast"
  ) ?? Urgency.medium
}

task brief(email: {body: str}) -> str {
  ai.summarize(email.body,
    in: 1, unit: sentence,
    using: "fast"
  ) ?? "(no summary)"
}

task compose(email: {body: str, from: str}, guidance: str? = none) -> str {
  if guidance != none {
    ai.draft("response to {email.body}", tone: "professional", guidance: guidance)
  } else {
    ai.draft("response to {email.body}", tone: "friendly", max_length: 150)
  } ?? "(draft failed)"
}

agent EmailAssistant {
  @role "You are a professional email assistant"
  @tools [email]
  @memory persistent

  state {
    handled_count: int = 0
  }

  task handle(email: {body: str, from: str, subject: str}) {
    urgency = triage(email)
    summary = brief(email)

    when urgency {
      low => {
        io.notify("Archived: {email.subject} [{urgency}]")
        email.archive(email)
      }
      medium => {
        reply = compose(email)
        if io.confirm("Auto-reply to '{email.subject}':\n\n{reply}") {
          email.send(reply, to: email.from)
        }
      }
      high, critical => {
        io.notify("{urgency} email from {email.from}")
        io.show({
          from:    email.from,
          subject: email.subject,
          summary: summary,
          urgency: urgency
        })
        guidance = io.ask("How should I respond?")
        reply = compose(email, guidance)
        if io.confirm(reply) {
          email.send(reply, to: email.from)
        }
      }
    }

    memory.remember({
      contact:    email.from,
      subject:    email.subject,
      urgency:    urgency,
      handled_at: now
    })

    self.handled_count = self.handled_count + 1
  }

  @on_start {
    schedule.every(5.minutes, () => {
      emails = email.fetch(unread: true)
      io.notify("{emails.count} new emails")
      for email in emails {
        self.handle(email)
      }
    })
  }
}

run(EmailAssistant)
```

## Setup

```bash
export IMAP_HOST=imap.gmail.com
export EMAIL_USER=you@gmail.com
export EMAIL_PASS=your-app-password
export KEEL_OLLAMA_MODEL=gemma4

keel run email_agent.keel
```

## How it works

1. Every 5 minutes, `email.fetch(unread: true)` pulls new messages.
2. `triage` classifies each by urgency using a fast model.
3. `low` → auto-archive.
4. `medium` → draft a reply, confirm before sending.
5. `high`/`critical` → show a summary, ask for guidance, draft with it, confirm.
6. Each interaction is remembered for future context.

Zero imports. `Ai`, `Io`, `Email`, `Schedule`, and `Memory` are all auto-imported via the [prelude](../guide/stdlib.md).
