# Changelog

All notable changes to Keel are documented here.

Format: features include a short `.keel` example, bug fixes include what was broken and how it was fixed.

---

## [Unreleased] — v1.0 "Pulse" (in progress)

### Fixed

#### Multi-line method chaining
A newline before `.` or `?.` no longer ends the statement — chains now read naturally across lines:

```keel
result = nums
  .filter(n => n > 2)
  .map(n => n * 10)
```

Before: the newline after `nums` terminated the statement, so the next line's `.filter(...)` was an unexpected token.

#### REPL persists bindings across inputs
The REPL now keeps one live interpreter for the whole session. Variables defined at the prompt carry over between expressions.

```
keel> nums = [1, 2, 3, 4, 5]
keel> nums.filter(n => n > 2).map(n => n * 10)
  [30, 40, 50]
```

Before: each input was wrapped in a throw-away task, so `nums` vanished as soon as the first prompt finished evaluating.

#### Runtime errors show the offending source line
Runtime errors now include a miette-rendered snippet of the source, underlining the statement that failed:

```
× Runtime error: Undefined variable: 'foo'
 ╭─[err.keel:5:5]
 4 │     nums = [1, 2, 3]
 5 │     result = nums.map(n => n + foo)
   ·     ─────────────┬─────────────
   ·                  ╰── Undefined variable: 'foo'
 6 │     notify user "result: {result}"
 ╰────
```

Before: runtime errors surfaced as a bare `Runtime error: ...` message with no source context.

### Added

#### Type Checker
Static type inference and checking pass that runs before execution. Catches errors at compile time instead of runtime.

```keel
type Mood = happy | sad | neutral

# Type error: non-exhaustive match — missing "neutral"
when mood {
  happy => notify user "great"
  sad   => notify user "cheer up"
}

# Type error: if condition must be bool, got str
if "hello" { notify user "bad" }

# Type error: expected str, got int
task greet(name: str) -> str { "hi" }
greet(42)

# Type error: Cannot assign str to state field 'count' of type int
self.count = "not a number"
```

Checks performed:
- Non-exhaustive `when` matches (missing enum variants)
- `if` condition must be `bool`
- `for` loop requires a `list`
- Undefined variables
- `self` used outside agent body
- Task argument count and type validation
- Incompatible arithmetic (`"hello" + 42`)
- Negating non-numeric types
- Mixed list element types (`[1, "two", 3]`)
- State field type mismatches
- `after` requires a `duration`
- `retry` count must be `int`
- Nullable safety: `T?` vs `T` tracking through `??` and `fallback`

#### Real Scheduling
`every` blocks now run on actual intervals using tokio. Agents stay alive and poll on schedule. Press `Ctrl+C` to stop.

```keel
agent Monitor {
  role "Checks status every 30 seconds"

  state { checks: int = 0 }

  every 30.seconds {
    self.checks = self.checks + 1
    notify user "Check #{self.checks}"
  }
}

run Monitor
# Output:
#   Check #1
#   Agent running. Press Ctrl+C to stop.
#   Check #2      (30s later)
#   Check #3      (60s later)
#   ...
```

`after` now uses real delays:

```keel
after 5.minutes {
  notify user "Reminder: follow up on the ticket"
}
```

#### AI Primitives: extract, translate, decide
All six AI primitives are now implemented (classify, summarize, draft were in v0.1).

**extract** — pull structured data from text:
```keel
info = extract {
  sender: str,
  subject: str,
  action_items: list[str]
} from email_body
# Returns a map: {sender: "Alice", subject: "Q3 Review", action_items: [...]}
```

**translate** — language translation:
```keel
french = translate message to french
# Returns: "Bonjour, comment allez-vous?"

localized = translate ui_text to [spanish, german, japanese]
# Returns: {spanish: "...", german: "...", japanese: "..."}
```

**decide** — structured decision with reasoning:
```keel
action = decide email {
  options: [reply, forward, archive, escalate],
  based_on: [urgency, sender, content]
}
# Returns: {choice: "reply", reason: "Direct question from a known contact"}
```

#### Retry with Backoff
Retry failed operations with configurable attempts and exponential backoff.

```keel
# Retry 3 times with 1s delay between each
retry 3 times { send email }

# Retry with exponential backoff: 1s, 2s, 4s
retry 3 times with backoff { send email }
```

Output on failure:
```
  ↻ Retry 1/3 in 1s: Network error
  ↻ Retry 2/3 in 2s: Network error
  ✗ Failed after 3 retries: Network error
```

#### Ollama Integration (Local LLM)
Run Keel agents with local models via Ollama. No API key needed.

```bash
# Catch-all: all model names → gemma4
KEEL_OLLAMA_MODEL=gemma4 keel run agent.keel

# Per-model mapping (fast model for classify, capable for draft)
KEEL_MODEL_CLAUDE_HAIKU=gemma4 \
KEEL_MODEL_CLAUDE_SONNET=mistral:7b-instruct \
  keel run agent.keel

# Direct Ollama model in .keel files
classify text as Mood using "ollama:gemma4"
```

Strict model validation — no silent fallbacks:
```
  ✗ Model 'claude-haiku' is not available locally.
  Set one of:
    export KEEL_MODEL_CLAUDE_HAIKU=<ollama_model>
    export KEEL_OLLAMA_MODEL=<ollama_model>
```

#### Lambda/Closure Execution
Lambdas work as expressions and as arguments to collection methods.

```keel
# Single-param lambda
doubled = [1, 2, 3].map(n => n * 2)        # [2, 4, 6]

# Filter with lambda
big = [1, 5, 10].filter(n => n > 3)        # [5, 10]

# Multi-param lambda
pairs = list.zip_with(other, (a, b) => a + b)

# All collection methods work: .map, .filter, .find, .any, .all, .sort_by, .flat_map
has_urgent = emails.any(e => e.urgency == high)
sorted = items.sort_by(item => item.name)

# Task references as callables
task double(n: int) -> int { n * 2 }
result = [1, 2, 3].map(double)             # [2, 4, 6]
```

#### Prompt Escape Hatch
Raw LLM access when the built-in AI primitives don't give enough control.

```keel
type LiabilityClauses {
  clauses: list[str]
  total_risk: float
}

result = prompt {
  system: "You are a legal document analyzer.",
  user: "Extract all liability clauses from: {document}"
} as LiabilityClauses
# result: map with parsed JSON fields
```

#### REPL (`keel repl`)
Interactive session for testing types, tasks, and expressions.

```
$ keel repl
Keel REPL v0.1
Type expressions, define tasks, or :help for commands.

keel> type Mood = happy | sad | neutral
  ✓ type Mood
keel> task greet(name: str) -> str { "Hello {name}!" }
  ✓ task greet
keel> :types
  Mood = happy | sad | neutral
keel> :quit
```

Commands: `:help`, `:types`, `:env`, `:clear`, `:quit`

#### Formatter (`keel fmt`)
Auto-format .keel files with consistent indentation and spacing.

```bash
keel fmt agent.keel
# ✓ Formatted agent.keel
```

#### Project Scaffold (`keel init`)
Generate a starter project with one command.

```bash
keel init my-email-bot
# ✓ Created project 'my-email-bot'
#   my-email-bot/main.keel
#
#   Run it:  keel run my-email-bot/main.keel
```

#### Short Duration Units
Abbreviations for duration literals.

```keel
every 30.sec { ... }    # same as 30.seconds
every 1.min { ... }     # same as 1.minutes
every 2.hr { ... }      # same as 2.hours
every 1.d { ... }       # same as 1.days
```

#### Source Span Error Diagnostics
Type errors now point to exact source locations with miette formatting.

```
  × Type error
   ╭─[agent.keel:8:5]
 7 │   every 1.day {
 8 │     greet(42)
   ·     ────┬────
   ·         ╰── Argument 'name' of task 'greet': expected str, got int
 9 │   }
   ╰────
```

#### Real Email Connections (IMAP/SMTP)
`connect email via imap` now establishes a real connection. `fetch email where unread` returns actual emails via IMAP. `send reply to email` sends via SMTP.

```keel
connect email via imap {
  host: env.IMAP_HOST,
  user: env.EMAIL_USER,
  pass: env.EMAIL_PASS
}

every 5.minutes {
  emails = fetch email where unread   # real IMAP fetch
  for email in emails {
    send reply to email               # real SMTP send
  }
}
```

```bash
export IMAP_HOST=imap.gmail.com EMAIL_USER=you@gmail.com EMAIL_PASS=app-password
keel run email_agent.keel
```

#### Real HTTP Fetch
`fetch "https://..."` makes a real HTTP GET and returns `{status, body, headers, is_ok}`.

```keel
response = fetch "https://api.example.com/data"
if response.is_ok {
  notify user response.body
}
```

#### Wait / Wait Until
Pause execution for a duration or until a condition is true.

```keel
wait 5.seconds
wait until is_ready
```

#### Bytecode Compiler and VM
`keel build` compiles .keel files to a register-based bytecode format (.keelc). The VM supports 40+ instruction types.

```bash
keel build agent.keel
# ✓ Compiled agent.keel → agent.keelc (28 ops, 2 functions)
```

#### Show with Table Formatting
`show user` now renders lists of maps as aligned tables.

```keel
show user [
  {name: "Alice", status: "active"},
  {name: "Bob", status: "away"}
]
```

Output:
```
  name   status
  ─────  ──────
  Alice  active
  Bob    away
```

#### mdBook Documentation
19-page documentation site built with mdBook. Custom Keel syntax highlighting in all code blocks.

```bash
cd docs && mdbook serve   # preview at localhost:3000
```

### Fixed

#### Keywords as field names
Keywords like `user`, `from`, `model`, `type` can now be used as struct field names and map keys, matching how they appear in real-world data.

```keel
# These now parse correctly:
connect email via imap {
  host: env.IMAP_HOST,
  user: env.EMAIL_USER,    # "user" is a keyword but valid as field name
  pass: env.EMAIL_PASS
}

task triage(email: {body: str, from: str}) -> Urgency {  # "from" as field name
  classify email.body as Urgency
}

show user {
  from:    email.from,     # "from" after dot also works
  subject: email.subject
}
```

#### if/else ?? null coalescing
`if/else` followed by `??` now works as a single expression.

```keel
reply = if has_guidance {
  draft "response" { guidance: guidance }
} else {
  draft "response" { tone: "friendly" }
} ?? "(draft failed)"
```

#### Newline-separated struct fields
Struct type definitions now accept newline separators (not just commas).

```keel
type TriageResult {
  urgency: Urgency
  category: Category     # no comma needed
}
```

#### Statement forms in when arms
`archive`, `notify`, and `send` now work as single-line when arm bodies without braces.

```keel
when category {
  spam => archive email                    # no braces needed
  urgent => notify user "Urgent mail!"
  _ => { process(email) }
}
```

---

## [0.1.0] — 2026-04-15 — "Spark"

First working implementation of the Keel language.

### Added

#### Lexer
80+ token types via `logos`. All Keel keywords, operators, literals, and string interpolation. Newline normalization for statement separation.

#### Parser
Complete `chumsky` 0.9 grammar: type declarations (simple enums, rich enums, structs), connect, task, agent, run statements, expressions with operator precedence, AI primitives, control flow, pattern matching.

#### Tree-Walking Interpreter
Async execution with scoped environments. Agent state, pattern matching, string interpolation, field access, binary/unary operators, method calls on strings and lists.

#### Agent Declarations
```keel
agent EmailAssistant {
  role "Professional email assistant"
  model "claude-sonnet"
  tools [email]
  memory persistent

  state {
    handled_count: int = 0
  }

  task handle(email: {body: str, from: str, subject: str}) {
    urgency = triage(email)
    when urgency {
      low => archive email
      high, critical => {
        guidance = ask user "How should I respond?"
        reply = compose(email, guidance)
        confirm user reply then send reply to email
      }
    }
  }

  every 5.minutes {
    emails = fetch email where unread
    for email in emails { handle(email) }
  }
}

run EmailAssistant
```

#### AI Primitives: classify, summarize, draft
```keel
urgency = classify email.body as Urgency considering [
  "mentions a deadline" => high,
  "newsletter"          => low
] fallback medium using "claude-haiku"

brief = summarize article in 1 sentence fallback "(no summary)"

reply = draft "response to {email}" {
  tone: "friendly",
  max_length: 150
} ?? "(draft failed)"
```

#### Human Interaction: ask, confirm, notify, show
```keel
answer = ask user "How should I respond?"
confirm user reply then send reply to email
notify user "3 new emails"
show user {from: email.from, subject: email.subject}
```

#### Type Declarations
```keel
type Urgency = low | medium | high | critical

type Action =
  | reply { to: str, tone: str }
  | forward { to: str }
  | archive

type EmailInfo {
  sender: str
  subject: str
  body: str
}
```

#### String Interpolation
```keel
notify user "Hello, {name}! You have {count} new messages."
notify user "From: {email.from}, Subject: {email.subject}"
```

#### Control Flow
```keel
if urgency == high { escalate(email) }

when urgency {
  low, medium => auto_reply(email)
  high        => flag_and_draft(email)
  critical    => escalate(email)
}

for email in emails { handle(email) }
```

#### CLI
```bash
keel run agent.keel     # execute
keel check agent.keel   # parse + type check
keel --version
keel --help
```

### Performance
- 8ms cold startup (release build)
- 5.2MB single binary
- 102 tests at launch
