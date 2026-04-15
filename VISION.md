# Keel — The Agent-Native Programming Language

> *What if programming languages were invented today, knowing that AI is the runtime?*

---

## The Problem

Building AI agents today means stitching together frameworks on top of general-purpose languages that were never designed for autonomous, intelligent systems. The result is hundreds of lines of Python boilerplate to express what should take five:

```python
# Python + LangChain: ~40 lines just to classify an email
from langchain.chat_models import ChatOpenAI
from langchain.prompts import ChatPromptTemplate
from langchain.output_parsers import EnumOutputParser
from enum import Enum
import asyncio

class Urgency(Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"

parser = EnumOutputParser(enum=Urgency)
prompt = ChatPromptTemplate.from_template(
    "Classify the urgency of this email:\n{email}\n{format_instructions}"
)
chain = prompt | ChatOpenAI(model="gpt-4") | parser

async def classify_email(email_body: str) -> Urgency:
    return await chain.ainvoke({
        "email": email_body,
        "format_instructions": parser.get_format_instructions()
    })
```

Now the same thing in **Keel**:

```keel
task triage(email) -> Urgency [low, medium, high, critical] {
  classify email.body
}
```

**That's it.** One task. One keyword. The language understands what `classify` means because AI isn't a library — it's the runtime.

---

## The Insight

Every generation of programming languages was shaped by its era's fundamental challenge:

| Era | Challenge | Language Response |
|-----|-----------|-------------------|
| 1950s | Hardware is expensive | Assembly, Fortran — close to the metal |
| 1970s | Software is complex | C, Pascal — structured programming |
| 1990s | The internet exists | Java, JavaScript — networked, portable |
| 2010s | Data is everywhere | Python, R — data science, ML pipelines |
| **Now** | **Intelligence is programmable** | **???** |

We're still writing AI agents in languages designed for data pipelines. Keel is the language designed for the age of programmable intelligence.

---

## Core Philosophy

### 1. Agents Are Primitives, Not Patterns

In Keel, an `agent` is as fundamental as a `function` in C or a `class` in Java. You don't *build* agents from components — you *declare* them.

```keel
agent Researcher {
  role "You find and synthesize information on any topic"
  model "claude-sonnet"
  tools [web, files]
}
```

### 2. Intent Over Implementation

Traditional code tells the computer *how* to do things. Keel tells the AI *what* you want. AI operations are **keywords**, not library calls:

```keel
summary   = summarize article in 3 sentences
urgency   = classify ticket as [low, medium, high]
entities  = extract {people: list[str], dates: list[str]} from contract
reply     = draft "response to {email}" { tone: "friendly" }
```

### 3. Humans in the Loop — By Design

Most agent frameworks treat human interaction as an afterthought. In Keel, the human is a first-class participant:

```keel
ask user "Should I proceed with this refund?" -> approval
confirm user draft_email then send
notify user "Task completed: {result}"
```

### 4. The Web Is a Native Data Type

No HTTP libraries. No request builders. The web is just *there*:

```keel
page    = fetch "https://example.com/api/data"
results = search "latest AI research papers 2026"
feed    = stream "wss://market-data.example.com"
```

### 5. Time Is Built In

Scheduling shouldn't require cron, Celery, or a separate infrastructure layer:

```keel
every 5 minutes { check_inbox() }
every monday at 9am { send_weekly_report() }
after 30 minutes { follow_up(ticket) }
```

### 6. Memory Is Persistent

Agents remember. Not because you wired up a vector database — because that's what agents *do*:

```keel
agent Support {
  memory persistent   # remembers across sessions
  
  on ticket(t) {
    history = recall similar issues to t   # semantic search, built-in
    ...
    remember resolution: t.solution         # learns for next time
  }
}
```

---

## What Makes Keel Different

| Feature | Python + Frameworks | Keel |
|---------|-------------------|------|
| Define an agent | ~50 lines of boilerplate | 4 lines |
| Classify text | Import parser, prompt template, chain, enum... | `classify x as [a, b, c]` |
| Ask the user | Custom input handler, async callback | `ask user "question"` |
| Schedule a task | Cron + Celery + Redis | `every 5 minutes { ... }` |
| Fetch a webpage | `import requests; requests.get(...)` | `fetch url` |
| Agent memory | Vector DB setup, embedding pipeline | `remember` / `recall` |
| Multi-agent | Orchestration framework | `agent A calls B` |

---

## The Elevator Pitch

**Keel** is a programming language where AI agents, human interaction, web access, memory, and scheduling are first-class citizens — not afterthoughts bolted onto a 30-year-old language. It reads like pseudocode but runs like production infrastructure.

If Python made programming accessible to data scientists, **Keel makes agent-building accessible to everyone.**

---

## Target Audience

1. **AI engineers** tired of framework boilerplate
2. **Product builders** who think in workflows, not code
3. **Automation specialists** who need agents that *just work*
4. **Researchers** prototyping multi-agent systems quickly

---

*Keel — because the future of programming isn't writing more code. It's writing less.*
