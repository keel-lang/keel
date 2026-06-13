# Example: Email Assistant

A complete email agent that triages, auto-replies, and escalates.

```keel
use std/ai
use std/email
use std/io
use std/memory
use std/schedule
use std/time

type Urgency = low | medium | high | critical

task triage(msg: {body: str, from: str, subject: str}) -> Urgency {
  ai.classify(msg.body,
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

task brief(msg: {body: str}) -> str {
  ai.summarize(msg.body,
    in: 1, unit: sentences,
    using: "fast"
  ) ?? "(no summary)"
}

task compose(msg: {body: str, from: str}, guidance: str? = none) -> str {
  if guidance != none {
    ai.draft("response to {msg.body}", tone: "professional", guidance: guidance)
  } else {
    ai.draft("response to {msg.body}", tone: "friendly", max_length: 150)
  } ?? "(draft failed)"
}

agent EmailAssistant {
  @role "You are a professional email assistant"
  @tools [ai, io, email]
  @memory persistent

  state {
    handled_count: int = 0
  }

  task handle(msg: {body: str, from: str, subject: str}) {
    urgency = triage(msg)
    summary = brief(msg)

    when urgency {
      low => {
        io.notify("Archived: {msg.subject} [{urgency}]")
        email.archive(msg)
      }
      medium => {
        reply = compose(msg)
        if io.confirm("Auto-reply to '{msg.subject}':\n\n{reply}") {
          email.send(reply, to: msg.from)
        }
      }
      high, critical => {
        io.notify("{urgency} email from {msg.from}")
        io.show({
          from:    msg.from,
          subject: msg.subject,
          summary: summary,
          urgency: urgency
        })
        guidance = io.ask("How should I respond?")
        reply = compose(msg, guidance)
        if io.confirm(reply) {
          email.send(reply, to: msg.from)
        }
      }
    }

    memory.remember(msg.from, {
      subject:    msg.subject,
      urgency:    urgency,
      handled_at: time.now()
    })

    self.handled_count = self.handled_count + 1
  }

  @on_start {
    schedule.every(5.minutes, () => {
      inbox = email.fetch(unread: true)
      io.notify("{inbox.count()} new emails")
      for msg in inbox {
        self.handle(msg)
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

Six imports declare everything this program touches, and `@tools [ai, io, email]` grants the agent its effectful capabilities — including the `ai.*` calls reached through the helper tasks. See [The Standard Library](../guide/stdlib.md).
