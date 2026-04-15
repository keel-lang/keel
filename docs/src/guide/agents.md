# Agents

An agent is the core building block — an autonomous entity with a role, capabilities, and behavior.

## Minimal agent

```keel
agent Greeter {
  role "You greet people warmly"
}

run Greeter
```

## Full anatomy

```keel
agent EmailAssistant {
  # Identity
  role "Professional email assistant for the team"
  model "claude-sonnet"

  # Capabilities
  tools [email, calendar]
  memory persistent

  # Mutable state
  state {
    processed: int = 0
    last_run: str = "never"
  }

  # Agent-scoped tasks
  task handle(email: {body: str, from: str}) {
    urgency = triage(email)
    self.processed = self.processed + 1
    notify user "Handled email #{self.processed}"
  }

  # Scheduled behavior
  every 5.minutes {
    emails = fetch email where unread
    for email in emails {
      handle(email)
    }
    self.last_run = "just now"
  }
}

run EmailAssistant
```

## Agent fields

| Field | Required | Description |
|-------|----------|-------------|
| `role` | Yes | Natural language description — used as LLM system prompt |
| `model` | No | LLM model name (default: `claude-sonnet`) |
| `tools` | No | List of connections this agent can use |
| `memory` | No | `none`, `session`, or `persistent` |
| `state` | No | Mutable fields with types and defaults |
| `config` | No | Key-value configuration (temperature, timeout, etc.) |
| `team` | No | List of other agents (for multi-agent systems) |

## State

Agent state is the only place where mutation is allowed. Access via `self.`:

```keel
state {
  count: int = 0
  last_seen: str = ""
}

task increment() {
  self.count = self.count + 1
  self.last_seen = now
}
```

State is isolated per agent — different agents don't share state.

## Scheduling

Agents come alive with `every`, `after`, and `on` blocks:

```keel
# Recurring
every 5.minutes { check_inbox() }

# One-time delayed
after 30.minutes { follow_up(ticket) }

# Event handler
on message(msg: Message) {
  response = draft "reply to {msg}" { tone: "warm" }
  send response to msg
}
```

## Running agents

```keel
run MyAgent                  # foreground — blocks until Ctrl+C
run MyAgent in background    # non-blocking
stop MyAgent                 # graceful shutdown
```

## Composition

Keep agents small and focused. Use top-level tasks for shared logic:

```keel
# Shared logic
task triage(email: {body: str}) -> Urgency {
  classify email.body as Urgency fallback medium
}

# Focused agent
agent Classifier {
  role "Classifies incoming email"
  every 5.minutes {
    for email in fetch email where unread {
      urgency = triage(email)
      notify user "{urgency}: {email.subject}"
    }
  }
}
```
