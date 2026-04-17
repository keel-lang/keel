<div style="text-align: center; margin-bottom: 2em;">
  <img src="wordmark.svg" alt="Keel" style="max-width: 280px; margin-bottom: 0.5em;" class="light-only"/>
  <img src="wordmark-dark.svg" alt="Keel" style="max-width: 280px; margin-bottom: 0.5em;" class="dark-only"/>
  <p style="color: #64748b; font-size: 1.1em;">A programming language where AI agents are first-class citizens</p>
</div>

# The Keel Language

**Keel** is a programming language where AI agents are first-class citizens.

Building an AI agent that monitors your email, classifies messages, and drafts replies takes ~180 lines of Python with LangChain. In Keel, it takes 40:

```keel
type Urgency = low | medium | high | critical

agent EmailAssistant {
  role "Professional email assistant"
  model "claude-sonnet"
  tools [email]

  task triage(email: {body: str}) -> Urgency {
    classify email.body as Urgency fallback medium
  }

  task handle(email: {body: str, from: str, subject: str}) {
    urgency = triage(email)

    when urgency {
      low, medium => {
        reply = draft "response to {email}" { tone: "friendly" }
        confirm user reply then send reply to email
      }
      high, critical => {
        notify user "{urgency}: {email.subject}"
        guidance = ask user "How to respond?"
        reply = draft "response to {email}" { guidance: guidance }
        confirm user reply then send reply to email
      }
    }
  }

  every 5.minutes {
    for email in fetch email where unread {
      handle(email)
    }
  }
}

run EmailAssistant
```

## Design Principles

1. **Agents are primitives** — `agent` is a keyword, not a class pattern
2. **Intent over implementation** — `classify`, `draft`, `summarize` are built-in keywords, not library calls
3. **Statically typed** — full inference, every expression has a known type, mismatches are compile errors
4. **Humans in the loop** — `ask`, `confirm`, `notify` are first-class
5. **Web is native** — `fetch`, `search`, `send` need no imports
6. **Time is built in** — `every`, `after`, `at` for scheduling

## What Keel Looks Like

### Classify with AI
```keel
urgency = classify email.body as Urgency considering [
  "mentions a deadline" => high,
  "newsletter"          => low
] fallback medium using "claude-haiku"
```

### Pattern matching
```keel
when urgency {
  low      => archive email
  medium   => auto_reply(email)
  high     => escalate(email)
  critical => page_oncall(email)
}
```

### Collections with lambdas
```keel
urgent = emails.filter(e => triage(e) == critical)
names = contacts.map(c => c.name).sort_by(n => n)
```

### Retry with backoff
```keel
retry 3 times with backoff { send email }
```

## Getting Started

Install Keel and build your first agent in 5 minutes: [Installation →](./getting-started/installation.md)
