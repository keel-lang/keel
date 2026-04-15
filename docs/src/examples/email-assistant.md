# Example: Email Assistant

A complete email agent that triages, auto-replies, and escalates.

```keel
type Urgency = low | medium | high | critical

connect email via imap {
  host: env.IMAP_HOST,
  user: env.EMAIL_USER,
  pass: env.EMAIL_PASS
}

task triage(email: {body: str, from: str, subject: str}) -> Urgency {
  classify email.body as Urgency considering [
    "from a known VIP or executive"     => critical,
    "mentions a deadline within 24h"    => high,
    "asks a direct question"            => medium,
    "newsletter or automated message"   => low
  ] fallback medium using "claude-haiku"
}

task brief(email: {body: str}) -> str {
  summarize email.body in 1 sentence fallback "(no summary)" using "claude-haiku"
}

task compose(email: {body: str, from: str}, guidance: str? = none) -> str {
  if guidance != none {
    draft "response to {email}" {
      tone: "professional",
      guidance: guidance
    }
  } else {
    draft "response to {email}" {
      tone: "friendly",
      max_length: 150
    }
  } ?? "(draft failed)"
}

agent EmailAssistant {
  role "You are a professional email assistant"
  model "claude-sonnet"
  tools [email]
  memory persistent

  state {
    handled_count: int = 0
  }

  task handle(email: {body: str, from: str, subject: str}) {
    urgency = triage(email)
    summary = brief(email)

    when urgency {
      low => {
        notify user "Archived: {email.subject} [{urgency}]"
        archive email
      }
      medium => {
        reply = compose(email)
        confirm user "Auto-reply to '{email.subject}':\n\n{reply}" then send reply to email
      }
      high, critical => {
        notify user "{urgency} email from {email.from}"
        show user {
          from:    email.from,
          subject: email.subject,
          summary: summary,
          urgency: urgency
        }
        guidance = ask user "How should I respond?"
        reply = compose(email, guidance)
        confirm user reply then send reply to email
      }
    }

    remember {
      contact:    email.from,
      subject:    email.subject,
      urgency:    urgency,
      handled_at: now
    }

    self.handled_count = self.handled_count + 1
  }

  every 5.minutes {
    emails = fetch email where unread
    notify user "{emails.count} new emails"
    for email in emails {
      handle(email)
    }
  }
}

run EmailAssistant
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

1. Every 5 minutes, fetches unread emails via IMAP
2. Classifies each by urgency using a fast model (claude-haiku → Ollama)
3. Low urgency → auto-archive
4. Medium → drafts a reply, asks for confirmation before sending
5. High/critical → shows summary, asks for guidance, drafts with that guidance
6. Remembers each interaction for future context
