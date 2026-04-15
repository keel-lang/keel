# Keel Language Specification — v0.2 (Draft)

> This specification defines the syntax, semantics, type system, and execution model for Keel — a programming language where AI agents are first-class citizens.

### Version Legend

Each section is tagged with the version that introduces it. Sections without a tag are foundational and ship in v0.1.

| Tag | Version | Codename | Description |
|-----|---------|----------|-------------|
| — | v0.1 | Spark | Core syntax, type system, agent/task, AI primitives, control flow, error handling |
| `v2.0` | v2.0 | Cortex | Multi-agent, persistent memory, modules |

Keel is **statically typed from day one.** The type checker runs at compile time in every version. Types are inferred wherever possible — explicit annotations are rarely required — but every expression has a known type, and mismatches are compile errors. There is no untyped mode.

---

## 1. Program Structure

An Keel program is a `.keel` file containing top-level declarations. No `main()` function needed — execution begins at the first `run` statement or event listener.

```keel
# my_agent.keel

agent MyAgent { ... }

run MyAgent
```

### File Extension: `.keel`
### Comments: `#` for single line, `## ... ##` for multi-line

### Top-Level Declarations

A file may contain, in any order:
- `agent` declarations
- `task` declarations (free-standing, not bound to an agent)
- `connect` statements
- `use` imports
- `run` statements
- `type` declarations

---

## 2. Type System

Keel uses a **structural type system with full inference** — the compiler infers types from context so annotations are rarely needed, but every expression is typed and every mismatch is a compile error. There is no untyped mode.

### 2.1 Design Principles

1. **Structural typing.** Types are shapes, not names. A value matches a type if it has the required fields — no explicit `implements` needed.
2. **Full inference.** The compiler infers types from initializers, return expressions, and AI primitive signatures. Explicit annotations are optional and override inferences — but the type checker runs regardless.
3. **Algebraic data types.** Enums can carry associated data per variant, enabling exhaustive `when` matching on rich structured values.
4. **Nullable safety.** Types are non-nullable by default. `?` marks a type as nullable. AI operations that can fail return nullable types.
5. **No implicit `any`.** Every expression has a concrete type. The `dynamic` escape hatch exists only for FFI/interop boundaries and must be explicitly opted into.

### 2.2 Primitive Types

| Type | Example | Notes |
|------|---------|-------|
| `int` | `42` | 64-bit integer |
| `float` | `3.14` | 64-bit float |
| `str` | `"hello"` | UTF-8, interpolation with `{expr}`, escape sequences (see below) |
| `bool` | `true`, `false` | |
| `none` | `none` | Unit type / absence value (see note below) |
| `duration` | `5.minutes`, `2.hours`, `1.day` | Duration literals |
| `datetime` | `@2026-04-15`, `@monday_9am` | Prefixed time literals |
| `dynamic` | — | FFI/interop escape hatch: opts out of type checking at explicit boundaries |

**Built-in value constants:**

| Value | Type | Description |
|-------|------|-------------|
| `true` | `bool` | Boolean true |
| `false` | `bool` | Boolean false |
| `none` | `none` | Absence of value |
| `now` | `datetime` | Current date/time at evaluation |

**`none` semantics:** `none` serves as both the unit type (the return type of tasks with no return value) and the absence value (what nullable types hold when empty). This is a deliberate unification: `none` is the single value of type `none`, and `T?` is shorthand for "T or none." Consequently, `none?` is equivalent to `none` — it adds no information. The tuple unit type `()` is also equivalent to `none`. `list[none]` is a valid but useless type (a list whose elements can only be `none`).

**String escape sequences:** Strings support the following escape sequences: `\"` (double quote), `\\` (backslash), `\n` (newline), `\t` (tab), `\r` (carriage return), `\{` (literal open brace — prevents interpolation), `\}` (literal close brace). Unrecognized escape sequences (e.g., `\z`) are compile errors.

**Multi-line strings:** Triple-quoted strings (`"""..."""`) preserve literal newlines and leading indentation. They support the same interpolation and escape sequences as regular strings. Useful for `rules`, `prompt` system messages, and long `draft` descriptions:

```keel
rules [
  """
  Never reveal internal pricing logic.
  Always include a disclaimer for medical questions.
  Escalate if the user expresses frustration 3+ times.
  """
]
```

### 2.3 Collection Types

```keel
nums: list[int]          = [1, 2, 3]
names: list[str]         = ["alice", "bob"]
info: map[str, str]      = {name: "Zied", role: "builder"}
ids: set[int]            = set[1, 2, 3]
```

**Rules:**
- `[...]` is always a `list`. Elements must share a common type.
- `{key: value, ...}` is always a `map`. Keys are identifiers or strings.
- `set[...]` constructs a `set`. `set` is a **reserved keyword** followed by `[...]` — the parser handles this as a special form, distinct from generic type syntax (which appears only in type positions). There is no brace-based set literal — `{}` is always a map/struct.

**Built-in collection properties and methods:**

| Type | Property/Method | Return Type | Description |
|------|----------------|-------------|-------------|
| `list[T]` | `.count` | `int` | Number of elements |
| `list[T]` | `.first` | `T?` | First element or none |
| `list[T]` | `.last` | `T?` | Last element or none |
| `list[T]` | `.is_empty` | `bool` | True if count == 0 |
| `map[K,V]` | `.count` | `int` | Number of entries |
| `map[K,V]` | `.keys` | `list[K]` | All keys |
| `map[K,V]` | `.values` | `list[V]` | All values |
| `set[T]` | `.count` | `int` | Number of elements |
| `set[T]` | `.contains(v)` | `bool` | True if v is in the set |
| `set[T]` | `.is_empty` | `bool` | True if count == 0 |

**Built-in string properties and methods:**

| Method/Property | Return Type | Description |
|----------------|-------------|-------------|
| `.length` | `int` | Number of UTF-8 characters |
| `.is_empty` | `bool` | True if length == 0 |
| `.contains(s)` | `bool` | True if string contains substring |
| `.starts_with(s)` | `bool` | True if string starts with prefix |
| `.ends_with(s)` | `bool` | True if string ends with suffix |
| `.trim()` | `str` | Remove leading/trailing whitespace |
| `.upper()` | `str` | Convert to uppercase |
| `.lower()` | `str` | Convert to lowercase |
| `.split(sep)` | `list[str]` | Split by separator |
| `.replace(old, new)` | `str` | Replace all occurrences |
| `.slice(start, end?)` | `str` | Substring by index (end is exclusive, optional) |

```keel
name = "  Hello, World!  "
trimmed = name.trim()                # "Hello, World!"
words = trimmed.split(", ")          # ["Hello", "World!"]
has_hello = name.contains("Hello")   # true
shouted = trimmed.upper()            # "HELLO, WORLD!"
```

**Built-in type conversions:**

Every primitive type supports explicit conversion methods:

| From | Method | To | Notes |
|------|--------|----|-------|
| `str` | `.to_int()` | `int?` | Returns none if not a valid integer |
| `str` | `.to_float()` | `float?` | Returns none if not a valid float |
| `int` | `.to_str()` | `str` | Always succeeds |
| `int` | `.to_float()` | `float` | Always succeeds |
| `float` | `.to_str()` | `str` | Always succeeds |
| `float` | `.to_int()` | `int` | Truncates toward zero |
| `bool` | `.to_str()` | `str` | `"true"` or `"false"` |
| `duration` | `.to_seconds()` | `int` | Total seconds |
| any enum variant | `.to_str()` | `str` | Variant name as string |

```keel
port_str = "8080"
port = port_str.to_int() ?? 3000     # int — 8080, or default 3000
ratio = 3.to_float() / 4.to_float()  # 0.75
label = Urgency.high.to_str()        # "high"
```

**Design rationale:** Conversions that can fail (`str` to numeric) return nullable types, forcing the caller to handle the failure case with `??` or `when`. Conversions that always succeed (numeric to `str`) return non-nullable types. This keeps nullability honest.

### 2.4 Struct Types (Structural Records)

Structs are anonymous structural types — any value with matching fields satisfies the type.

```keel
# Define a named struct type
type EmailInfo {
  sender: str
  subject: str
  body: str
  unread: bool
}

# Inline struct types (anonymous)
task triage(email: {body: str, from: str}) -> Urgency {
  classify email.body as Urgency
}

# Any value with the right shape satisfies the type
info = {sender: "alice", subject: "Hi", body: "Hello!", unread: true}
# info is compatible with EmailInfo — no explicit annotation needed
```

**Structural compatibility rule:** A value of type `A` is assignable to type `B` if `A` has all fields of `B` with compatible types. Extra fields are allowed (width subtyping).

#### Generic Struct Types

Struct types can be parameterized with type variables:

```keel
type Paginated[T] {
  items: list[T]
  page: int
  total: int
  has_more: bool
}

type Cache[K, V] {
  entries: map[K, V]
  max_size: int
}
```

Type parameters are specified in `[...]` after the type name. When constructing or receiving a value of a generic type, the parameters are inferred from context where possible, or specified explicitly:

```keel
# Inferred from usage
results = fetch_paginated(query)         # Paginated[SearchResult] inferred from fetch_paginated's return type
page_items = results.items               # list[SearchResult]

# Explicit when needed
cache: Cache[str, int] = make_cache(100)
```

**Rules:**
- Type parameters are always specified in `[...]`, consistent with `list[T]`, `map[K, V]`, `Result[T]`.
- Type parameters are constrained by usage — if a field uses `T` as a map key, `T` must be a hashable type. The compiler infers these constraints.

### 2.5 Enum Types (Algebraic Data Types)

Enums define a closed set of variants. Variants can be **simple** (no data) or **rich** (carrying associated fields). They are Keel's algebraic data types — the primary mechanism for classification results, decision outcomes, state machines, and error types.

#### Simple Enums

```keel
# Named enum — simple variants (no associated data)
type Urgency = low | medium | high | critical

type Category = bug | feature | question | billing

# Built-in enums (provided by the standard library)
# Urgency = low | medium | high | critical
# Sentiment = positive | neutral | negative
```

**Enum values are not bare strings** — `low` in the context of `Urgency` is the value `Urgency.low`, which is distinct from the string `"low"`. Use `to_str()` for conversion.

#### Rich Enums (Variants with Associated Data)

Variants can carry structured fields, enabling type-safe unions:

```keel
type Action =
  | reply { to: str, tone: str }
  | forward { to: str }
  | archive
  | escalate { reason: str, urgency: Urgency }

type Shape =
  | circle { radius: float }
  | rectangle { width: float, height: float }
  | point
```

**Construction:**

```keel
action = Action.reply { to: "alice@example.com", tone: "friendly" }
shape = Shape.circle { radius: 5.0 }
simple = Action.archive   # no braces needed for data-less variants
```

**Pattern matching with destructuring:**

```keel
when action {
  reply { to, tone }     => send draft_reply(tone) to to
  forward { to }         => send email to to
  archive                => archive email
  escalate { reason, _ } => notify user "Escalation: {reason}"
}

description = when shape {
  circle { radius }            => "Circle r={radius}"
  rectangle { width, height }
    where width == height      => "Square {width}x{width}"
  rectangle { width, height }  => "Rectangle {width}x{height}"
  point                        => "Point"
}
```

**Rules:**
- All `when` matches on enums must be **exhaustive** — every variant must be handled, or `_` must be present.
- Rich variant fields are accessed via destructuring in `when` arms, not via dot notation on the enum value.
- Simple enums and rich enums use the same `type` declaration syntax. A variant with `{ fields }` is rich; without is simple. Mixing both in one enum is allowed (see `Action` above).
- Rich enum variants are **not** structurally compatible with structs — `Action.reply { to: "x", tone: "y" }` is not assignable to `{ to: str, tone: str }`. Use explicit extraction if needed.

#### Generic Enum Types

Enums can also be parameterized. `Result[T]` (Section 2.8) is a built-in example:

```keel
type Outcome[T, E] =
  | success { value: T }
  | failure { error: E }
  | pending

type Tree[T] =
  | leaf { value: T }
  | node { left: Tree[T], right: Tree[T] }
```

### 2.6 Nullable Types

Types are non-nullable by default. Append `?` to mark a type as nullable.

```keel
name: str       = "Keel"     # cannot be none
alias: str?     = none       # can be none

# Null-safe field access
subject = email?.subject     # str? — none if email is none

# Null coalescing
subject = email?.subject ?? "(no subject)"   # str — guaranteed non-none

# Null assertion (unsafe, throws on none)
subject = email!.subject     # str — throws NullError if email is none
```

**AI operations return nullable types when they can fail:**

```keel
result: Urgency? = classify email.body as Urgency   # may return none on failure
safe = result ?? Urgency.medium                      # provide a default
```

To get a non-nullable result, use `fallback`:

```keel
result: Urgency = classify email.body as Urgency fallback medium
# result is guaranteed non-none
```

### 2.7 Tuple Types

Tuples are fixed-size, ordered collections of heterogeneous values. They are useful for returning multiple values from a task without defining a named struct.

```keel
# Tuple type syntax
pair: (str, int) = ("hello", 42)
triple: (str, Urgency, bool) = ("ticket-1", Urgency.high, true)

# Type inference works — no annotation needed
coords = (3.14, 2.71)   # inferred as (float, float)
```

**Accessing tuple elements:**

```keel
# Positional access with .0, .1, .2, ...
x = coords.0    # 3.14
y = coords.1    # 2.71

# Destructuring (preferred)
(x, y) = coords
(name, urgency, handled) = triple
```

**In task signatures:**

```keel
task triage_full(email: {body: str}) -> (Urgency, str) {
  urgency = classify email.body as Urgency fallback medium
  summary = summarize email.body in 1 sentence fallback "(no summary)"
  (urgency, summary)
}

# Caller destructures the result
(urgency, summary) = triage_full(email)
```

**Rules:**
- Tuples are structurally typed: `(str, int)` is the same type regardless of where it was created.
- Tuples are immutable.
- Single-element tuples are not supported — `(str)` is just parenthesized `str`. Use a named struct for single-field wrappers.
- The unit type `()` is equivalent to `none`.

### 2.8 The `Result` Type

For operations that can fail with an error, Keel provides a built-in generic result type as a rich enum:

```keel
# Built-in definition
type Result[T] =
  | ok { value: T }
  | err { error: Error }
```

```keel
result = try fetch "https://api.example.com/data"
when result {
  ok { value }  => process(value)
  err { error } => notify user "Failed: {error.message}"
}
```

**`Result` is a standard rich enum.** It follows the same pattern matching rules as any other enum — `when` must be exhaustive, and variants are destructured in arms. This means `try` expressions that return `Result[T]` compose naturally with `when`, `??`, and pipelines.

### 2.9 Type Inference Rules

Every built-in keyword has a defined return type. The compiler infers the result type without annotations:

| Expression | Inferred Type | Notes |
|------------|---------------|-------|
| `classify X as T` | `T?` | Nullable; use `fallback` for `T` |
| `classify X as T fallback V` | `T` | Non-nullable |
| `extract {fields} from X` | `{fields}?` | Struct matching the schema, nullable |
| `summarize X in N UNIT` | `str?` | Nullable string |
| `summarize X ... fallback V` | `str` | Non-nullable with fallback |
| `summarize X format F` | `str?` | Format: bullets, prose, json |
| `draft "..." { opts }` | `str?` | Nullable string |
| `draft "..." { opts } ?? V` | `str` | Non-nullable via null coalescing |
| `translate X to LANG` | `str?` | Nullable string |
| `decide X { options }` | `Decision?` | `{choice: T, reason: str}?` |
| `fetch URL` | `Response?` | Nullable response |
| `search QUERY` | `list[SearchResult]` | May be empty list, never none |
| `ask user PROMPT` | `str` | Blocks until response; non-nullable |
| `ask user PROMPT options T` | `T` | Returns one of the options |
| `confirm user EXPR` | `bool` | `true` if approved, `false` if declined |
| `recall QUERY` | `list[Memory]` | May be empty, never none |
| `delegate TASK to AGENT` | Return type of TASK, nullable | `T?` |

### 2.10 The `dynamic` Type (FFI/Interop Only)

`dynamic` exists for boundaries where Keel receives data from external systems with unknown shape: `extern` function returns, `prompt as dynamic`, and raw `sql` query results. It is **not** a general-purpose escape hatch — it is the type of data crossing an untyped boundary, and it must always be opted into explicitly.

```keel
# extern function returning unstructured data
extern task parse_legacy(data: str) -> dynamic
  from "legacy_parser"

# Explicit dynamic prompt (prompt always requires "as T")
raw = prompt {
  system: "Analyze this document.",
  user: "{document}"
} as dynamic
# raw: dynamic — must be narrowed before use

# Narrowing dynamic to a concrete type
info: MyStruct = raw as MyStruct   # runtime check, throws TypeError on mismatch
```

**Rules:**
- `dynamic` values can be accessed freely (field access, indexing) — errors are runtime-only.
- `dynamic` values must be explicitly narrowed (via `as T`) before being passed to typed parameters.
- **`dynamic` must be opted into.** `prompt` requires `as T` (use `as dynamic` to opt out). `HttpResponse` provides `.json_as[T]()` (use `[dynamic]` to opt out). There is no path to `dynamic` that doesn't require writing the word `dynamic`.
- The compiler emits a **warning** when `dynamic` is used outside of `extern`, `prompt`, `http`, or `sql` contexts.
- `dynamic` defeats IDE autocomplete and compile-time error detection. Narrow it as early as possible.
- **`dynamic` is not used in any standard library type.** Built-in types (`HttpResponse`, `Memory`, etc.) use concrete types. `dynamic` only enters a program through explicit escape hatches (`prompt as dynamic`, `extern`, `http`, `sql`).

### 2.11 Built-in Runtime Types

The following types are provided by the standard library and available without imports:

```keel
type Message {
  from: str
  body: str
  channel: str?        # "email", "slack", "terminal", etc.
  timestamp: datetime?
}

type SearchResult {
  title: str
  url: str
  snippet: str
}

type Memory {
  content: map[str, str]
  relevance: float
  created_at: datetime
}

type HttpResponse {
  status: int
  body: str
  headers: map[str, str]
}

type Decision[T] {
  choice: T
  reason: str
  confidence: float
}

type Error =
  | AIError { model: str, tokens_used: int }
  | NetworkError { status: int?, url: str }
  | TimeoutError { duration: duration }
  | NullError
  | TypeError { expected: str, got: str }
  | ParseError { position: int }

# All Error variants implicitly carry: message: str, source: str?
# These base fields are accessible on any Error value without matching.
```

**`HttpResponse` methods:**

| Method | Return Type | Description |
|--------|-------------|-------------|
| `.json_as[T]()` | `T?` | Parse body as JSON and validate against type `T`. Returns `none` on parse failure. |
| `.is_ok` | `bool` | True if status is 200–299 |

```keel
response = fetch "https://api.example.com/users"
users = response?.json_as[list[User]]() ?? []
```

**Design rationale:** Earlier drafts included a `.json()` method returning `dynamic`. This was removed because it provided an easy path to untyped code where a typed alternative (`.json_as[T]()`) exists. If the response shape is truly unknown, use `.json_as[dynamic]()` to make the opt-out explicit.

**`Memory` design note:** `Memory.content` is `map[str, str]` — all values are serialized to strings on `remember` and deserialized on `recall`. This keeps `dynamic` out of the standard types. For structured recall, use `extract` on the memory content or store JSON strings and parse with `.json_as[T]()` on recall.

### 2.12 Variable Bindings and Mutability

Keel uses **immutable bindings by default**. All local variables created with `=` are immutable — once bound, they cannot be reassigned:

```keel
name = "Keel"
name = "Other"     # OK — this is a new binding that shadows the previous one
                   # The original "Keel" value is no longer accessible
```

**Shadowing** (rebinding the same name in the same scope) is allowed. This is binding, not mutation — the original value is untouched, and the name now refers to a new value. This follows Rust's `let` semantics.

**Agent `state` fields are the exception.** They are mutable and accessed via `self.`:

```keel
agent Counter {
  state {
    count: int = 0
    last_seen: datetime? = none
  }

  task increment() {
    self.count = self.count + 1
    self.last_seen = now
  }

  on message(msg: Message) {
    increment()
    notify user "Count: {self.count}"
  }
}
```

**Rules:**
- `x = expr` creates an immutable binding. Rebinding `x` in the same scope shadows the previous value.
- `self.field = expr` mutates an agent's `state` field. Only valid inside agent tasks and handlers.
- `self` is only available inside an agent body. Top-level tasks do not have `self`.
- Collections returned by expressions are immutable values. Use functional transforms (`.map`, `.filter`) rather than in-place mutation.
- This model eliminates data races: handlers are sequential per-agent, and all local state is immutable.

**Design rationale:** Default immutability catches a class of bugs at compile time (accidental reassignment, loop-variable mutation) and pairs naturally with the sequential handler model. Agent state is the one place where mutation is intentional and safe — `self.` makes it syntactically visible and greppable.

---

## 3. The `agent` Construct

An agent is the core building block — an autonomous entity with a role, capabilities, and behavior.

### 3.1 Minimal Agent

```keel
agent Greeter {
  role "You greet people warmly"
}
```

### 3.2 Full Agent Anatomy

```keel
agent AgentName {
  # --- Identity ---
  role "Natural language description of what this agent does"
  model "claude-sonnet"                 # AI model to use (string for flexibility)

  # --- Capabilities ---
  tools [email, calendar, web]         # external integrations
  connect slack via "workspace-token"  # authenticated connections
  memory persistent                    # none | session | persistent

  # --- Configuration ---
  config {
    temperature: 0.7
    max_tokens: 4096
    retry: 3
    timeout: 30.seconds
  }

  # --- State (mutable via self.) ---
  state {
    processed_count: int = 0
    last_run: datetime? = none
  }

  # --- Tasks (agent-scoped functions) ---
  task greet(name: str) -> str {
    draft "greeting for {name}" {
      tone: "warm"
    }
  }

  # --- Event Handlers ---
  on message(msg: Message) {
    response = greet(msg.from)
    send response to msg
    self.processed_count = self.processed_count + 1
    self.last_run = now
  }

  # --- Scheduled Behaviors ---
  every 1.day at 9am {
    notify user "Good morning! Here's your daily brief."
  }
}
```

### 3.3 Agent Lifecycle

```
  define -> configure -> run -> (listen | schedule | respond) -> stop
```

Agents are started with `run` and stopped with `stop`:

```keel
run MyAgent                  # start the agent
run MyAgent in background    # non-blocking
stop MyAgent                 # graceful shutdown
```

### 3.4 Agent State and Thread Safety

Agent `state` fields are **mutable via `self.`** and **isolated per agent instance**. The runtime guarantees that:
- State fields are accessed and mutated exclusively through `self.field` (see Section 2.12).
- Event handlers for a single agent execute sequentially (no concurrent access to state).
- Different agents run concurrently but do not share state.
- Shared data must go through `delegate`, `broadcast`, or `memory`.

See Section 13 (Concurrency Model) for details.

### 3.5 Composition Over Monoliths

Agent tasks can be defined at the **top level** (outside any agent) for reuse and testability. An agent can call top-level tasks directly. Prefer small agents that compose shared tasks over large agents that define everything inline:

```keel
# Top-level: testable, reusable across agents
task triage(email: {body: str}) -> Urgency {
  classify email.body as Urgency fallback medium
}

task brief(body: str) -> str {
  summarize body in 1 sentence fallback "(no summary)"
}

# Agent uses shared tasks — stays small and focused
agent EmailAssistant {
  role "You triage and respond to emails"
  model "claude-sonnet"

  on message(msg: Message) {
    urgency = triage(msg)           # calls top-level task
    summary = brief(msg.body)       # calls top-level task
    show user {urgency: urgency, summary: summary}
  }
}
```

**Design rationale:** Tasks defined inside an agent are scoped to that agent and cannot be reused or tested independently. Top-level tasks are the default for any logic that might be shared, composed, or unit-tested. Agent-scoped tasks should be reserved for behavior that genuinely depends on `self` (agent state access).

---

## 4. The `task` Construct

Tasks are named, reusable operations. They are the "functions" of Keel, but with built-in AI awareness.

### 4.1 Basic Task

```keel
task greet(name: str) -> str {
  "Hello, {name}!"
}
```

The last expression in a task body is the return value (implicit return). Explicit `return` is also supported for early exits:

```keel
task handle(email: {body: str, from: str}) -> str {
  if email.from.contains("noreply") {
    return "Skipped automated email"
  }

  urgency = classify email.body as Urgency fallback medium
  if urgency == Urgency.low {
    return "Auto-archived"
  }

  # Last expression is the implicit return
  draft "response to {email}" { tone: "professional" } ?? "(draft failed)"
}
```

**Implicit return rules:**
1. The last expression in a block is the return value.
2. `return expr` exits the enclosing task immediately with `expr`.
3. When `if`/`else` is the last expression, both branches must produce compatible types (see Section 10.1).
4. A block ending in a statement (e.g., `send`, `notify`) implicitly returns `none`.

### 4.2 Task with AI Operations

```keel
task triage(email: {body: str}) -> Urgency {
  classify email.body as Urgency fallback medium
}
```

### 4.3 Task Composition (Pipelines)

The `|>` operator chains tasks into pipelines. Each step receives the output of the previous step as its first argument:

```keel
task process(email: EmailInfo) {
  email |> triage |> respond |> log
}

# Equivalent to:
task process(email: EmailInfo) {
  result1 = triage(email)
  result2 = respond(result1)
  log(result2)
}
```

The pipeline operator passes the left-hand value as the **first argument** to the right-hand function. Additional arguments use parentheses:

```keel
email |> triage |> respond(tone: "friendly") |> log("email_responses")
```

### 4.4 Task Signatures

```keel
# No parameters, no return type (returns none)
task cleanup() { ... }

# Typed parameters, inferred return
task greet(name: str) { "Hello, {name}!" }

# Fully annotated
task triage(email: EmailInfo) -> Urgency { ... }

# Default parameters
task compose(email: EmailInfo, tone: str = "professional") -> str { ... }

# Structural parameter types (inline)
task quick(data: {body: str}) -> str { data.body }
```

---

## 5. AI Primitives

These are **built-in keywords**, not library functions. The runtime handles prompt construction, model routing, parsing, and validation.

Each primitive has a **formal grammar** defining exactly what syntax the parser accepts. Natural-language modifiers are restricted to a fixed set of clause keywords.

### Model Override with `using`

All AI primitives inherit the model from the enclosing agent's `model` field (or the global default from `keel.config`). To override the model for a specific operation, append `using STRING`:

```keel
# Fast classification with a lightweight model
urgency = classify email.body as Urgency fallback medium using "claude-haiku"

# High-quality drafting with a capable model
reply = draft "response to {email}" { tone: "formal" } using "claude-sonnet"

# Summarize with the agent's default model (no override)
brief = summarize email.body in 1 sentence
```

**Grammar clause (appended to any AI primitive):**
```
using_clause ::= "using" STRING
```

The `using` clause is always the **last** clause in any AI primitive expression. It is optional — if omitted, the enclosing agent's model is used.

**Design rationale:** Real agent workflows routinely need different models for different operations — fast/cheap models for classification, capable models for drafting. Making this a per-operation override rather than agent-level avoids the need for `prompt` escape hatches just to switch models.

### 5.1 `classify`

Categorizes input into a predefined enum type.

**Grammar:**
```
classify_expr ::= "classify" expr "as" enum_type
                  ( "considering" "[" criteria_list "]" )?
                  ( "fallback" enum_value )?
                  ( "using" STRING )?

enum_type     ::= IDENT                          # named: Urgency
                | "[" IDENT ("," IDENT)* "]"      # inline: [low, medium, high]

criteria_list ::= criteria ("," criteria)*
criteria      ::= STRING "=>" enum_value

enum_value    ::= IDENT
```

**Examples:**

```keel
urgency = classify email.body as Urgency
sentiment = classify review as [positive, neutral, negative]
category = classify ticket as Category fallback question
```

**With criteria (mapping hints for the LLM):**

```keel
urgency = classify email.body as Urgency considering [
  "mentions a deadline within 24h"  => high,
  "uses urgent/angry language"      => critical,
  "newsletter or automated message" => low
]
```

**Return type:** `T?` without fallback, `T` with fallback (where `T` is the enum type).

### 5.2 `extract`

Pulls structured data from unstructured text.

**Grammar:**
```
extract_expr ::= "extract" struct_schema "from" expr
                 ( "using" STRING )?

struct_schema ::= "{" field_def ("," field_def)* "}"
field_def     ::= IDENT ":" type
```

**Examples:**

```keel
info = extract {
  sender: str,
  subject: str,
  action_items: list[str]
} from email

dates = extract {
  start_date: str,
  end_date: str
} from contract
```

**Return type:** The struct type matching the schema, nullable: `{sender: str, subject: str, action_items: list[str]}?`.

### 5.3 `summarize`

Condenses content.

**Grammar:**
```
summarize_expr ::= "summarize" expr
                   ( "in" INT_LIT UNIT )?
                   ( "format" FORMAT )?
                   ( "fallback" expr )?
                   ( "using" STRING )?

UNIT           ::= "sentence" | "sentences" | "line" | "lines"
                 | "word" | "words" | "paragraph" | "paragraphs"
FORMAT         ::= "bullets" | "prose" | "json"
```

**Design rationale:** Earlier drafts used `as` for the format specifier (`summarize X as bullets`), but `as` already means "target type" in `classify X as Urgency`. Using `format` avoids overloading a keyword with two different semantics.

**Examples:**

```keel
brief = summarize article in 3 sentences
bullets = summarize report format bullets
tldr = summarize thread in 1 line
long_summary = summarize document in 2 paragraphs format prose
```

**Return type:** `str?`

### 5.4 `draft`

Generates text content.

**Grammar:**
```
draft_expr ::= "draft" STRING
               ( "{" draft_opts "}" )?
               ( "using" STRING )?

draft_opts  ::= (IDENT ":" expr ","?)*
```

The description is always a **string literal**, which may contain `{expr}` interpolations to reference in-scope variables. The interpolated values are included as context in the LLM prompt. Modifiers like tone and length go inside the optional `{}` block.

**Design rationale:** Earlier drafts allowed bare-word descriptions (`draft response to email`), but this made variable references indistinguishable from instruction words. Renaming a variable could silently change the prompt semantics, and IDEs could not reliably highlight, rename, or autocomplete within the description. String interpolation makes the boundary between instruction text and variable references explicit and toolable.

**Examples:**

```keel
# Simple — description only
reply = draft "response to {email}"

# With constraints — use the block form for modifiers
reply = draft "response to {email}" {
  tone: "friendly",
  max_length: 150,
  include: ["greeting", "action items", "sign-off"]
}

post = draft "blog post about {topic}" {
  tone: "professional",
  length: 500
}

# No variables — pure instruction
greeting = draft "a warm welcome message for new users"
```

**Return type:** `str?`

### 5.5 `translate`

Language translation.

**Grammar:**
```
translate_expr ::= "translate" expr "to" target_lang
                   ( "using" STRING )?

target_lang    ::= IDENT                          # single: french
                 | "[" IDENT ("," IDENT)* "]"     # multi: [spanish, german]
```

**Examples:**

```keel
french = translate message to french
localized = translate ui_strings to [spanish, german, japanese]
```

**Return type:** `str?` for single target, `map[str, str]?` for multi-target.

### 5.6 `decide`

Makes a structured decision with reasoning.

**Grammar:**
```
decide_expr ::= "decide" expr "{" decide_opts "}"
                ( "using" STRING )?

decide_opts ::= (IDENT ":" expr ","?)*          # must include "options"
```

**Examples:**

```keel
action = decide email {
  options: [reply, forward, archive, escalate],
  based_on: [urgency, sender, content]
}
# action.choice  : one of the options enum
# action.reason  : str — LLM's explanation
```

**Return type:** `Decision[T]?` where `T` is the enum of options (see Section 2.11 for `Decision` definition).

### 5.7 AI Operation Extensibility

The built-in AI keywords (`classify`, `extract`, `summarize`, `draft`, `translate`, `decide`) cover the most common agent operations. This set is **intentionally fixed** — adding a new keyword requires a language change (new grammar rule, parser update, type inference rule).

**Why keywords instead of functions?** Keywords enable custom grammar (`classify X as T`, `extract {schema} from X`) that reads like natural language and carries type information the compiler can check. A function call like `classify(x, Urgency)` would be less readable and harder to type-check structurally.

**When you need an operation not covered by the built-ins**, use these escape hatches in order of preference:

1. **`prompt` (§6.1)** — Raw LLM access with full control over system/user messages, temperature, and response format. Best for one-off custom AI operations.

2. **`extern` task (§6.4)** — Call a Python function that wraps any AI library. Best for operations that need specific models, APIs, or processing pipelines not available in the Keel runtime.

3. **Compose from primitives** — Combine `extract` + `classify` + `draft` to build complex operations from simple ones.

```keel
# Custom "score" operation via typed prompt
type SentimentScore {
  score: int
  explanation: str
}

score = prompt {
  system: "Rate the sentiment on a scale of 1-10.",
  user: "Text: {review}",
  response_format: json
} as SentimentScore

# Custom "rewrite" via extern
extern task rewrite(text: str, style: str) -> str
  from "text_utils.py"
```

**Future direction:** If a pattern of `prompt`-based operations becomes common (e.g., `score`, `rank`, `compare`), it may be promoted to a keyword in a future version. The threshold for adding a keyword is: (1) the operation is widely used, (2) custom grammar improves readability, and (3) the type system benefits from understanding the operation's structure.

---

## 6. Escape Hatches

Every abstraction layer has an escape hatch for cases where the built-in keywords are too constrained. These are intentional "trap doors" — use them when the higher-level constructs don't fit.

### 6.1 `prompt` — Raw LLM Access

When `classify`/`draft`/`summarize` don't give enough control, `prompt` sends a raw request to the configured model. **`prompt` requires `as T`** to specify the expected response shape:

```keel
type LiabilityClauses {
  clauses: list[str]
  total_risk: float
  jurisdiction: str?
}

result = prompt {
  system: "You are a legal document analyzer.",
  user: "Extract all liability clauses from: {document}"
} as LiabilityClauses
# result: LiabilityClauses? — the runtime parses and validates the response
```

**Return type:** `T?` — nullable because parsing/validation may fail. Use `?? default` or `fallback` to make non-nullable.

#### Untyped `prompt` with `as dynamic`

When the response shape is truly unknown, explicitly opt out of typing with `as dynamic`:

```keel
result = prompt {
  system: "You are a legal document analyzer.",
  user: "Extract all liability clauses from: {document}",
  model: "claude-sonnet",
  temperature: 0.0,
  max_tokens: 2000,
  response_format: json
} as dynamic
# result: dynamic — must be narrowed before use
```

**Return type:** `dynamic` when using `as dynamic`. The caller is responsible for narrowing.

A `prompt` without any `as` clause is a **compile error**. This forces the developer to make a conscious choice about typing.

**Design rationale:** Making the typed path the default and `dynamic` the explicit escape hatch prevents accidental `dynamic` propagation. The developer must write `as dynamic` to opt out of type safety — a visible, greppable decision that the IDE can flag. This follows the principle that typed code should require less ceremony than untyped code.

### 6.2 `http` — Raw HTTP Access

When `fetch` doesn't handle your API's auth scheme or you need full control over the request:

```keel
response = http {
  method: POST,
  url: "https://api.example.com/v2/classify",
  headers: {
    Authorization: "Bearer {env.API_KEY}",
    Content-Type: "application/json"
  },
  body: {text: email.body},
  timeout: 10.seconds
}
# response.status  : int
# response.body    : str
# response.headers : map[str, str]
```

**Return type:** `HttpResponse?` (see Section 2.11 for `HttpResponse` definition).

### 6.3 `sql` — Raw Database Access

When `connect database` and built-in queries don't suffice:

```keel
rows = sql "SELECT * FROM interactions WHERE contact = ? AND created_at > ?" 
  with [email.from, 30.days.ago]
# rows: list[dynamic]
```

**Return type:** `list[dynamic]`

### 6.4 `extern` — Call External Code

For calling Python (or other host language) functions from Keel:

```keel
# Declare an external function with its Keel-side type signature
extern task tokenize(text: str) -> list[str]
  from "nlp_utils.py"

# Use it like any other task
tokens = tokenize(document.body)
```

See Section 15 (Interoperability) for full details.

---

## 7. Human Interaction Primitives

### 7.1 `ask`

Prompts the user for input. **Blocks** the current task until the user responds.

**Grammar:**
```
ask_expr ::= "ask" "user" STRING
             ( "options" enum_or_list )?
             ( "via" channel )?

channel  ::= IDENT   # terminal, slack, email, etc.
```

```keel
answer = ask user "How should I respond to this?"
choice = ask user "Pick a priority" options [low, medium, high]
channel_choice = ask user "Approve deployment?" options [yes, no] via slack
```

**Return type:** `str` for free-text, `T` for `options T` where T is the enum.

### 7.2 `confirm`

Asks for yes/no approval. Returns `bool` — `true` if the user approved, `false` if declined.

**Grammar:**
```
confirm_expr ::= "confirm" "user" expr ( "via" channel )?
               | "confirm" "user" STRING ( "via" channel )?
confirm_stmt ::= confirm_expr "then" statement
```

```keel
# Expression form — returns bool, caller decides what to do
approved = confirm user "Send this reply?\n\n{draft_reply}"
if approved { send draft_reply to email }

# Statement form with `then` — syntactic sugar
confirm user draft_reply then send draft_reply to email
# Equivalent to: if confirm user draft_reply { send draft_reply to email }

# Can combine with via
confirm user "Delete these 50 files?" via slack then delete_all()
```

**Return type:** `bool`

**Statement form semantics:** `confirm user X then Y` is equivalent to `if confirm user X { Y }`. The `then` clause is skipped if the user declines. The expression form is preferred when you need to branch on the result or log the decision.

### 7.3 `notify`

Non-blocking notification to the user. Does not wait for a response.

**Grammar:**
```
notify_stmt ::= "notify" "user" expr ( "via" channel )?
```

```keel
notify user "Email classified as critical"
notify user "Weekly report ready" via slack
```

### 7.4 `show`

Presents data to the user for review.

**Grammar:**
```
show_stmt ::= "show" "user" expr
```

```keel
show user email_summary
show user table(results)
show user chart(metrics)
```

---

## 8. Web & External Access

### 8.1 `fetch`

Retrieves data from URLs or connected sources.

**Grammar:**
```
fetch_expr ::= "fetch" STRING                              # URL fetch
             | "fetch" IDENT                                # connection fetch (all)
             | "fetch" IDENT "where" predicate              # connection fetch (filtered)
```

The source identifier in connection-mode `fetch` must match the name of a `connect` statement in scope. This makes sources statically resolvable — the IDE can autocomplete source names from declared connections and validate filters against the connection's capabilities.

**Design rationale:** Earlier drafts allowed multi-word source expressions (`fetch new_emails where unread`), but this created parsing ambiguity (unlimited lookahead to find the `where` boundary) and made static analysis difficult. A single identifier matching the connection name is unambiguous and toolable.

```keel
# URL fetch — returns HttpResponse?
page = fetch "https://api.example.com/data"

# Connection-sourced fetch — source matches a connect statement name
emails = fetch email where unread
events = fetch calendar where date == today

# Unfiltered — fetch all from connection
messages = fetch slack
```

**Return type:** `HttpResponse?` for URLs, connection-specific type for sources.

### 8.2 `search`

Web search.

**Grammar:**
```
search_expr ::= "search" STRING
              | "search" IDENT "for" STRING
```

```keel
results = search "latest AI agent frameworks 2026"
papers = search arxiv for "transformer architecture improvements"
```

**Return type:** `list[SearchResult]` (see Section 2.11 for `SearchResult` definition).

### 8.3 `send`

Sends data to external services.

**Grammar:**
```
send_stmt ::= "send" expr "to" target ( "via" channel )?

target    ::= IDENT ( "." IDENT )*       # dotted identifier path only
            | IDENT "(" args? ")"         # function call
channel   ::= IDENT
```

```keel
send reply to email
send message to slack_channel("engineering")
send report to user via email
send reply to email.from    # target is email.from (dotted path)
```

**Parsing rule:** The target of `send ... to` is restricted to dotted identifier paths or function calls — not arbitrary expressions. This avoids ambiguity: `send reply to results.first` always parses `results.first` as the target, never as `(send reply to results).first`. If you need a computed target, bind it to a variable first:

```keel
target = compute_recipient(email)
send reply to target
```

### 8.4 `archive`

Moves a message or item to its archive. This is a connection-level action — the connected service determines what "archive" means (e.g., move to archive folder in IMAP, close in a ticket system).

**Grammar:**
```
archive_stmt ::= "archive" expr
```

```keel
archive email              # moves email to archive folder
archive ticket             # closes/archives the ticket
```

### 8.5 `connect`

Establishes authenticated connections to external services.

**Grammar:**
```
connect_stmt ::= "connect" IDENT "via" IDENT ( STRING )? ( "{" config_opts "}" )?
               | "connect" IDENT STRING                    # shorthand for DB URLs etc.
```

```keel
connect email via imap {
  host: env.IMAP_HOST,
  user: env.EMAIL_USER,
  pass: env.EMAIL_PASS
}

connect slack via "workspace-token"
connect database "postgres://localhost/mydb"
```

---

## 9. Time & Scheduling

### 9.1 Duration Literals

Durations use the `.unit` suffix:

```keel
5.seconds    30.minutes    2.hours    1.day    7.days
```

Both singular and plural unit names are accepted: `1.day` and `2.days` are both valid.

**Lexer disambiguation:** When the lexer encounters `INT "."`, it looks ahead to determine whether the `.` begins a duration unit or a float literal. If the character after `.` is a letter (e.g., `5.minutes`), it is tokenized as `INT_LIT "." DURATION_UNIT`. If the character is a digit (e.g., `3.14`), it is tokenized as `FLOAT_LIT`. This means `5.minutes` is always a duration, never a float followed by a field access.

**Permanent constraint:** Because the lexer claims `INT "." LETTER` as a duration, integer values will never support user-defined methods or extension methods. This is a deliberate trade-off — duration literals (`5.minutes`) are used far more often than integer methods would be. If you need to call a method on an integer, bind it to a variable first: `n = 5; n.to_str()`.

These are values of type `duration` and can be used in arithmetic:

```keel
timeout = 30.seconds
extended = timeout * 2    # 60 seconds
```

### 9.2 `every`

Recurring execution.

```keel
every 5.minutes { check_inbox() }
every 1.hour { sync_data() }
every monday at 9am { send_weekly_report() }
every day at 6pm { daily_summary() }
```

### 9.3 `after`

Delayed one-time execution.

```keel
after 30.minutes { follow_up(ticket) }
after 2.hours { remind user "Check on deployment" }
```

### 9.4 `at`

One-time scheduled execution at an absolute time.

```keel
at @2026-04-20_10am { launch_campaign() }
```

### 9.5 `wait`

Pauses the current task.

```keel
wait 5.seconds
wait until user_responds
wait for approval from user
```

---

## 10. Control Flow

### 10.1 `if` / `else`

`if` / `else` is an **expression** — it evaluates to the value of the taken branch. This means it can be used on the right side of assignments, as a return value, or with operators like `??`.

```keel
# As a statement (else is optional)
if urgency == Urgency.high {
  escalate(email)
}

# As a statement with else
if urgency == Urgency.high {
  escalate(email)
} else {
  auto_reply(email)
}

# As an expression — else is REQUIRED
reply = if guidance != none {
  draft "response to {email}" { tone: "professional", guidance: guidance }
} else {
  draft "response to {email}" { tone: "friendly", max_length: 150 }
}

# Combined with null coalescing
reply = if guidance != none {
  draft "response to {email}" { guidance: guidance }
} else {
  draft "response to {email}" { tone: "friendly" }
} ?? "(draft failed)"
```

**Expression vs. statement rule:** When `if` is used as an expression (assigned to a variable, returned from a task, or used with an operator like `??`), the `else` branch is **required** and both branches must produce values of compatible types. An `if` without `else` is only valid as a statement — its value is not used.

**Interaction with implicit return:** When `if`/`else` is the **last expression** in a task body, it determines the return value. An `if` without `else` at the end of a task body is a **compile error** if the task has a non-`none` return type:

```keel
# OK — both branches produce str
task describe(x: bool) -> str {
  if x { "yes" } else { "no" }
}

# COMPILE ERROR — if without else cannot be the return value of -> str
task bad(x: bool) -> str {
  if x { "yes" }
  # error: task returns none when x is false, but declared return type is str
}

# OK — explicit return before the end, final expression covers the else case
task good(x: bool) -> str {
  if x { return "yes" }
  "no"
}
```

**Design rationale:** Allowing expression-form `if` without `else` would silently produce `none`, creating a subtle source of nullable values. Requiring `else` makes the developer's intent explicit and prevents accidental null returns. This follows the Rust approach.

### 10.2 `when` (Pattern Matching)

`when` is an exhaustive pattern match. The compiler **requires** all cases of an enum to be handled (or a wildcard `_` must be present).

**Grammar:**
```
when_expr ::= "when" expr "{" when_arm+ "}"
when_arm  ::= pattern ("," pattern)* ("where" expr)? "=>" expr_or_block
pattern   ::= IDENT | "_" | literal | struct_pattern | variant_pattern | tuple_pattern

struct_pattern  ::= "{" (IDENT ":" pattern ","?)+ "}"
variant_pattern ::= IDENT "{" (IDENT ("," IDENT)* )? "}"    # rich enum variant
tuple_pattern   ::= "(" pattern ("," pattern)* ")"
```

```keel
# Simple enum matching
when urgency {
  low, medium => auto_reply(email)
  high        => flag_and_draft(email)
  critical    => escalate(email)
}
```

**Exhaustiveness checking:** If `urgency` is typed as `Urgency = low | medium | high | critical`, the compiler verifies all variants are covered. A missing case is a **compile error**:

```keel
when urgency {
  low    => archive(email)
  medium => auto_reply(email)
  # ERROR: non-exhaustive match — missing cases: high, critical
}
```

Use `_` as a wildcard to catch remaining cases:

```keel
when urgency {
  critical => escalate(email)
  _        => auto_reply(email)     # covers low, medium, high
}
```

**Rich enum variant matching (see Section 2.5):**

```keel
when action {
  reply { to, tone } => send draft_reply(tone) to to
  forward { to }     => send email to to
  archive            => archive email
  escalate { reason, urgency }
    where urgency == Urgency.critical => page_oncall(reason)
  escalate { reason, _ } => notify user "Escalation: {reason}"
}
```

**Result type matching:**

```keel
when result {
  ok { value }  => process(value)
  err { error } => notify user "Error: {error.message}"
}
```

**Tuple matching:**

```keel
(urgency, summary) = triage_full(email)
when (urgency, summary.is_empty) {
  (critical, _)    => escalate(email)
  (_, true)        => notify user "Could not summarize"
  _                => auto_reply(email)
}
```

**Matching on non-enum types (primitives, strings):**

`when` can match any value, not just enums. When matching on a primitive type (`int`, `str`, `bool`), the compiler **cannot** verify exhaustiveness — a wildcard `_` arm is **required**:

```keel
when response.status {
  200     => process(response)
  404     => notify user "Not found"
  500     => retry 3 times with backoff { refetch() }
  _       => notify user "Unexpected status: {response.status}"
}

when command {
  "help"  => show_help()
  "quit"  => stop self
  _       => notify user "Unknown command: {command}"
}
```

A `when` on a non-enum type without `_` is a **compile error**. Enum types use variant-based exhaustiveness; everything else requires the wildcard.

### 10.3 Destructuring

Destructuring extracts fields from structs, tuples, and maps into local variables.

**Struct destructuring:**

```keel
result = delegate triage(email) to Classifier
{urgency, category} = result

# With renaming
{urgency: u, category: c} = result

# Nested
{urgency, category: {name, priority}} = deep_result
```

**Tuple destructuring:**

```keel
(urgency, summary) = triage_full(email)
(x, y, z) = get_coordinates()
```

**In `for` loops:**

```keel
for {from, subject} in emails {
  notify user "Mail from {from}: {subject}"
}
```

**In task parameters:**

```keel
task handle({body, from, subject}: EmailInfo) {
  classify body as Urgency
}
```

**Design rationale:** Without destructuring, accessing struct fields requires repeated `result.field` access or manual assignment to locals. Destructuring is especially valuable in `for` loops over structured collections and when receiving multi-field results from `delegate` or `extract`.

### 10.4 `for` Loops

```keel
for email in emails {
  process(email)
}

for email in emails where email.unread {
  triage(email)
}
```

### 10.5 `try` / `catch`

```keel
try {
  send email
} catch err: NetworkError {
  retry 3 times with backoff { send email }
} catch err: Error {
  notify user "Failed to send: {err.message}"
}
```

See Section 12 (Error Handling) for the full error type hierarchy.

### 10.6 Lambdas & First-Class Functions

Lambdas are anonymous, inline functions. They enable concise collection transformations and callback patterns.

**Grammar:**
```
lambda_expr ::= "(" params? ")" "=>" expr
              | "(" params? ")" "=>" block
              | IDENT "=>" expr               # single-param shorthand
```

**Examples:**

```keel
# Single-parameter shorthand
triaged = emails.map(e => triage(e))

# Multi-parameter
pairs = list.zip_with(other, (a, b) => a + b)

# With block body
results = emails.map(e => {
  urgency = triage(e)
  {email: e, urgency: urgency}
})

# Storing in a variable
handler = (msg: Message) => draft "response to {msg}"
```

**Function types:**

```keel
# Type syntax for functions
type Handler = (Message) -> str
type Predicate[T] = (T) -> bool
type Transform = (str, int) -> str

# Task parameter accepting a function
task process_all(emails: list[EmailInfo], handler: (EmailInfo) -> str) {
  emails.map(handler)
}
```

**Built-in collection methods (enabled by lambdas):**

| Method | Signature | Description |
|--------|-----------|-------------|
| `.map(fn)` | `list[T].map((T) -> U) -> list[U]` | Transform each element |
| `.filter(fn)` | `list[T].filter((T) -> bool) -> list[T]` | Keep matching elements |
| `.find(fn)` | `list[T].find((T) -> bool) -> T?` | First matching element |
| `.any(fn)` | `list[T].any((T) -> bool) -> bool` | True if any match |
| `.all(fn)` | `list[T].all((T) -> bool) -> bool` | True if all match |
| `.sort_by(fn)` | `list[T].sort_by((T) -> U) -> list[T]` | Sort by derived key |
| `.group_by(fn)` | `list[T].group_by((T) -> U) -> map[U, list[T]]` | Group by key |
| `.flat_map(fn)` | `list[T].flat_map((T) -> list[U]) -> list[U]` | Map and flatten |

```keel
# Practical examples
urgent = emails.filter(e => triage(e) == Urgency.critical)
names = contacts.map(c => c.name).sort_by(n => n)
by_sender = emails.group_by(e => e.from)
has_unread = emails.any(e => e.unread)
```

**Task references:** Named tasks can be referenced as values by name (without calling them). The type is inferred from the task's signature:

```keel
task triage(email: EmailInfo) -> Urgency { ... }

# triage is a value of type (EmailInfo) -> Urgency
results = emails.map(triage)
```

**Design rationale:** Without first-class functions, `for` + `if` is the only way to transform collections, which is verbose and imperative. Lambdas and collection methods enable declarative data transformation — a natural fit for agent workflows that process lists of messages, results, or events.

---

## 11. Memory & State `v2.0`

### 11.1 `remember`

Stores structured data in the agent's persistent memory. Each record is automatically embedded for later semantic search. The argument can be any expression that evaluates to a map or struct.

```keel
# Inline map literal
remember {
  contact: email.from,
  preference: "prefers formal tone",
  last_interaction: now
}

# Variable reference
interaction = {
  contact: email.from,
  urgency: urgency,
  handled_at: now
}
remember interaction
```

**Return type:** `none` (side effect).

### 11.2 `recall`

Retrieves from memory using semantic search.

**Grammar:**
```
recall_expr ::= "recall" STRING                           # freeform query
              | "recall" STRING "limit" INT_LIT            # with result cap
```

```keel
history = recall "interactions with {email.from}"
similar = recall "issues similar to {ticket.description}" limit 5
prefs = recall "preferences for {user.name}" limit 1
```

**Return type:** `list[Memory]` — may be empty, never none (see Section 2.11 for `Memory` definition).

### 11.3 `forget`

Removes records from memory.

**Grammar:**
```
forget_stmt ::= "forget" STRING
              | "forget" "older_than" duration_expr
```

```keel
forget "interactions with old_user@example.com"
forget older_than 90.days
```

---

## 12. Error Handling

### 12.1 Error Type — An ADT

`Error` is a rich enum (ADT) defined in Section 2.11. All variants implicitly carry `message: str` and `source: str?`. Specialized variants add domain-specific fields.

### 12.2 Error Matching: Variant Pattern Matching

`catch` clauses use **variant matching** — the same mechanism as `when` on any enum. An error is caught when its variant matches the named variant in the `catch` clause. This is consistent with Keel's type system: no special nominal exception needed.

```keel
catch err: AIError { ... }      # matches Error.AIError variant
catch err: NetworkError { ... }  # matches Error.NetworkError variant
catch err: Error { ... }         # catches any Error variant (catch-all)
```

`catch err: Error` serves as a catch-all because `Error` is the enum type — matching the type name matches any variant.

**Catch clauses can destructure variant fields:**

```keel
catch err: NetworkError {
  notify user "Request to {err.url} failed with status {err.status}"
}
```

### 12.3 `try` / `catch`

```keel
try {
  result = classify email.body as Urgency
  send response to email
} catch err: AIError {
  # LLM refused, rate limited, or returned unparseable output
  notify user "AI failed ({err.model}): {err.message}"
  result = Urgency.medium   # manual fallback
} catch err: NetworkError {
  # Connection to email server failed
  retry 3 times with backoff { send response to email }
} catch err: Error {
  # Catch-all for any Error variant
  notify user "Unexpected error: {err.message}"
}
```

### 12.4 `retry`

```keel
retry 3 times with backoff {
  send email
}

# Backoff is exponential by default: 1s, 2s, 4s
# Custom backoff:
retry 5 times with delay 10.seconds {
  fetch "https://api.example.com/data"
}
```

### 12.5 `fallback`

Provides a default value when an AI operation fails, making the result non-nullable:

```keel
urgency = classify text as Urgency fallback medium
summary = summarize article in 3 sentences fallback "No summary available."
```

### 12.6 Safety: Limits and Rules

Agent safety is split into two mechanisms with different enforcement semantics:

#### `limits` — Static, Measurable Constraints

Limits are **numeric or boolean constraints** enforced by the runtime with deterministic checks. They go in the agent's `config` block:

```keel
agent Support {
  config {
    max_cost_per_request: 0.50       # USD — runtime rejects AI calls exceeding this
    max_tokens_per_request: 4096     # hard cap on tokens per AI call
    require_confirmation: [send, delete]  # these actions require confirm user
    timeout: 30.seconds
  }
}
```

| Field | Type | Enforcement |
|-------|------|-------------|
| `max_cost_per_request` | `float` | Runtime rejects AI calls exceeding this USD amount |
| `max_tokens_per_request` | `int` | Runtime caps tokens per AI call |
| `require_confirmation` | `list[str]` | Runtime inserts `confirm user` before these actions |
| `timeout` | `duration` | Runtime cancels operations exceeding this duration |

#### `rules` — LLM-Interpreted Behavioral Constraints

Rules are **natural-language instructions** injected into every AI prompt for this agent. They are interpreted by the LLM, not enforced deterministically:

```keel
agent Support {
  rules [
    "never reveal internal pricing logic",
    "always include disclaimer for medical questions",
    "escalate if user expresses frustration 3+ times"
  ]
}
```

**Design rationale:** Limits and rules look superficially similar but have fundamentally different enforcement mechanisms. Limits are checked by the runtime with deterministic logic (cost > threshold = reject). Rules are injected into LLM prompts and depend on model compliance. Mixing them in a single `guardrails` block obscures this critical distinction — a developer might assume `"never reveal pricing"` is as reliable as `max_cost: 0.50`, when in reality the LLM can violate rules.

### 12.7 Compile-Time Error Catalog

The compiler reports these categories of errors:

| Error | Example | Severity |
|-------|---------|----------|
| **Type mismatch** | Passing `int` where `str` expected | Error |
| **Non-exhaustive match** | Missing enum case in `when` | Error |
| **Unknown identifier** | Referencing undefined agent or task | Error |
| **Unreachable code** | Code after unconditional `return` | Warning |
| **Nullable access** | Accessing `.field` on `T?` without null check | Error |
| **Unused variable** | Assigning but never reading | Warning |
| **Unhandled nullable** | Using nullable AI result without `?`, `??`, or `fallback` | Error |
| **Shadowed name** | Variable shadows agent or task name | Warning |
| **Invalid grammar** | `classify` without `as`, `extract` without `from` | Error |
| **`self` outside agent** | Using `self.field` in a top-level task | Error |
| **Missing wildcard** | `when` on non-enum type without `_` arm | Error |
| **State without `self`** | Assigning to a state field name without `self.` prefix | Warning |
| **Unreachable `catch`** | `catch err: NetworkError` after `catch err: Error` (catch-all) | Warning |
| **Missing `prompt as`** | `prompt { ... }` without `as T` clause | Error |

---

## 13. Concurrency Model

Keel agents are inherently concurrent. This section defines the execution semantics precisely.

### 13.1 Execution Principles

1. **Agents are independent.** Each agent runs in its own lightweight coroutine. Agents do not share mutable state.
2. **Handlers are sequential per agent.** Within a single agent, event handlers and scheduled tasks execute one at a time. A new event waits in a queue until the current handler completes. This eliminates data races on `state` fields.
3. **`delegate` is async by default.** `delegate task to Agent` sends a message and awaits the result. The calling agent's handler is suspended (not blocked) while waiting.
4. **Blocking calls.** `ask user`, `confirm user`, and `wait` suspend the current handler. Other agents continue running. Other events for the *same* agent are queued.

### 13.2 `parallel` — Concurrent Execution

Run multiple independent operations concurrently within a single task:

```keel
parallel {
  urgency   = delegate triage(email) to Classifier
  sentiment = delegate analyze(email) to SentimentBot
  summary   = summarize email.body in 2 sentences
}
# All three results are available here
```

**Semantics:**
- All branches launch concurrently.
- The `parallel` block completes when **all** branches complete.
- If **one** branch throws an error, the remaining branches are **cancelled** and that error propagates.
- If **multiple** branches fail, the **first error** (in declaration order) propagates. Other errors are logged but discarded.
- Cancellation is best-effort: in-flight LLM calls may complete before cancellation takes effect. Tokens consumed by cancelled branches still count against cost limits.
- **No partial results.** If any branch fails, the entire `parallel` block fails — variables from successful branches are not available after the block. Use `try`/`catch` around the `parallel` block to handle failure.

```keel
# Handling parallel failures
try {
  parallel {
    urgency   = delegate triage(email) to Classifier
    sentiment = delegate analyze(email) to SentimentBot
  }
} catch err: Error {
  # Fallback when parallel classification fails
  urgency = Urgency.medium
  sentiment = Sentiment.neutral
}
```

### 13.3 `race` — First Result Wins

```keel
result = race {
  search google for query
  search bing for query
  search arxiv for query
}
# result contains the first successful response; others are cancelled
```

### 13.4 Agent Event Queue

```
                    ┌──────────────────────┐
   event ──────>   │   Event Queue        │ ──> Handler (sequential)
   event ──────>   │   (per agent)        │
   timer ──────>   │                      │
                    └──────────────────────┘
```

Events (incoming messages, timer fires, delegate requests) are placed in the agent's queue. The runtime processes them one at a time, in order.

If a handler calls `ask user` or `delegate`, the handler suspends and the runtime may process the *next* event for that agent **only if** the agent is configured with `config { concurrent_handlers: true }`. By default, the queue blocks.

---

## 14. Multi-Agent Collaboration `v2.0`

### 14.1 Agent-to-Agent Communication

```keel
agent Classifier { ... }
agent Responder { ... }

agent Orchestrator {
  role "Coordinate email handling"
  team [Classifier, Responder]

  on new_email(email: EmailInfo) {
    urgency = delegate triage(email) to Classifier
    delegate respond(email, urgency) to Responder
  }
}
```

### 14.2 `delegate`

Passes work to another agent. The target agent must have the named task defined.

**Grammar:**
```
delegate_expr ::= "delegate" IDENT "(" args ")" "to" IDENT
```

```keel
result = delegate triage(email) to Classifier
```

**Return type:** The return type of the target task, wrapped in nullable: `T?`. Delegation can fail if the target agent is stopped or errors.

### 14.3 `broadcast`

Sends a message to all agents in a team. Does not wait for responses.

```keel
broadcast "new policy update" to team
broadcast {type: "refresh", data: new_config} to team
```

**Return type:** `none`

---

## 15. Interoperability

Keel's runtime is built in Rust. External code is called via a plugin interface — the `extern` mechanism provides a typed boundary between Keel and host-language functions.

### 15.1 Calling External Code from Keel (`extern`)

Declare external functions with Keel-side type signatures:

```keel
# Import a single function — the runtime loads it via the plugin interface
extern task tokenize(text: str) -> list[str]
  from "nlp_utils"

# Use normally
tokens = tokenize(document.body)
```

**Rules:**
- The `extern` declaration tells the compiler what type to expect. The actual function is invoked at runtime via the plugin interface.
- If the external function's return type doesn't match the declared type, a runtime `TypeError` is thrown.
- `extern` functions are always typed — this is the one place where type annotations are mandatory, because the compiler cannot infer types across the language boundary.

**Plugin interface (v1.0):** External functions are loaded as shared libraries (`.so`/`.dylib`) or invoked via subprocess with JSON-based message passing. The exact mechanism is defined by the runtime, not the language grammar — `extern` declarations are runtime-agnostic.

### 15.2 Calling Keel from External Code

The Keel runtime exposes a CLI and (in v1.0+) a library interface:

```bash
# Run an Keel agent from any language/script
keel run email_agent.keel

# Invoke a specific task and get JSON output
keel exec email_agent.keel --task triage --input '{"body": "..."}'
```

### 15.3 Gradual Migration Path

| Phase | What changes | Example |
|-------|-------------|---------|
| **Phase 1** | Use `extern` to call existing code from Keel | Keep existing tools, orchestrate from Keel |
| **Phase 2** | Rewrite external tasks as Keel tasks | Replace `classify_email()` with `classify ... as Urgency` |
| **Phase 3** | Full Keel | Remove `extern` declarations, pure `.keel` |

Each phase is independently deployable. No big-bang rewrites required.

---

## 16. Environment & Configuration

### 16.1 Environment Variables

```keel
api_key = env.OPENAI_API_KEY
db_url = env.DATABASE_URL
```

`env` is a built-in map of type `map[str, str]`. Accessing a missing key throws a `ConfigError` at startup (fail-fast).

### 16.2 Configuration File (`keel.config`)

```yaml
# keel.config
model: "claude-sonnet"
memory_backend: sqlite
log_level: info

connections:
  email:
    provider: gmail
    credentials: env.GMAIL_CREDS
  database:
    url: env.DATABASE_URL

ai:
  default_temperature: 0.7
  default_max_tokens: 4096
  cost_limit_daily: 10.00     # USD
```

---

## 17. Modules & Imports `v2.0`

**Grammar:**
```
import_stmt ::= "use" STRING                          # import file
              | "use" IDENT "from" STRING              # import specific symbol
              | "use" package_path                     # import package

package_path ::= IDENT ("/" IDENT)+                   # e.g., keel/slack
```

```keel
# Import another Keel file
use "./email_utils.keel"

# Import a specific agent or task
use Classifier from "./classifiers.keel"

# Import official connectors
use keel/slack
use keel/notion

# Import community packages
use community/crm
```

---

## 18. Operator Reference

| Operator | Meaning | Example |
|----------|---------|---------|
| `\|>` | Pipeline (pass result as first arg) | `email \|> triage \|> respond` |
| `=>` | Case mapping (in `when` and `classify`) | `high => escalate(email)` |
| `->` | Return type annotation | `task greet(name: str) -> str` |
| `??` | Null coalescing (default if none) | `result ?? "unknown"` |
| `?.` | Null-safe field access | `email?.subject` |
| `!.` | Null assertion (throws on none) | `email!.subject` |
| `..` | Range (inclusive) | `1..10` |
| `in` | Membership test | `if x in list` |
| `as` | Type annotation in AI primitives | `classify x as Urgency` |
| `using` | Model override for AI primitives | `classify x as T using "model"` |
| `==`, `!=` | Equality | `if x == y` |
| `<`, `>`, `<=`, `>=` | Comparison | `if count > 10` |
| `and`, `or`, `not` | Boolean logic | `if a and not b` |
| `+`, `-`, `*`, `/`, `%` | Arithmetic | `total / count` |

**Operator disambiguation:**
- `->` is **only** used in type annotations: `task f() -> T` and agent lifecycle diagrams.
- `=>` is **only** used in pattern match arms (`when`) and `classify ... considering` criteria.
- `|>` is **only** used for pipelines. It has lower precedence than function calls.
- `using` is **only** used in AI primitives to override the model. It is always the last clause.

---

## 19. Reserved Keywords

```
agent    task     role     model    tools    connect
memory   state    config   every    after    at
fetch    send     search   classify extract  summarize
draft    translate decide  ask      confirm  notify
show     remember recall   forget   delegate broadcast
run      stop     wait     retry    fallback when
use      from     for      if       else     try
catch    in       where    to       via      with
true     false    none     now      env      self
user     type     extern   parallel race     prompt
http     sql      set      and      or       not
return   team     archive  format   rules    limits
using
```

---

## 20. Execution Model

Keel programs run on the **Keel Runtime**, built in Rust from day one using Tokio for async concurrency.

### Compilation Pipeline

```
v0.1:  .keel → Lexer (logos) → Parser (chumsky) → AST → Type Checker → Tree-Walking Interpreter
v1.0:  .keel → Lexer → Parser → AST → Type Checker → Bytecode Compiler → Keel VM
v3.0:  .keel → Lexer → Parser → AST → Type Checker → Keel IR → LLVM IR → Native Binary
```

### Runtime Services

1. **AI Engine** — routes AI primitives to configured LLM providers via `reqwest`, manages retries and fallbacks
2. **Scheduler** — manages `every`, `after`, `at` constructs via Tokio timers
3. **Memory Store** — handles `remember` / `recall` with semantic indexing (v2.0+)
4. **Connection Manager** — manages authenticated external connections (email, Slack, databases)
5. **Human Interface** — routes `ask` / `confirm` / `notify` / `show` to terminal I/O (v0.1) or UI (v2.0+)
6. **Event Loop** — Tokio async runtime handling concurrent agents and operations
7. **Type Checker** — validates types at compile time; every expression is typed, every mismatch is an error

```
┌──────────────────────────────────────────────────────────┐
│                   Keel Runtime (Rust)                      │
│                                                            │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐  │
│  │  AI Engine   │  │  Scheduler   │  │  Memory Store  │  │
│  │  (reqwest)   │  │  (tokio)     │  │  (sqlite+vec)  │  │
│  └──────────────┘  └──────────────┘  └────────────────┘  │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐  │
│  │  Connection  │  │   Human      │  │  Tokio Async   │  │
│  │  Manager     │  │  Interface   │  │  Event Loop    │  │
│  │ (lettre/imap)│  │  (terminal)  │  │                │  │
│  └──────────────┘  └──────────────┘  └────────────────┘  │
│  ┌──────────────────────────────────────────────────────┐ │
│  │         Type Checker (inferred, always-on)            │ │
│  └──────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
```

---

## 21. Formal Grammar (PEG Summary)

This section provides a condensed PEG grammar for the v0.1 language subset. The full grammar will be maintained alongside the parser implementation.

```peg
# === Top level ===
Program       <- (Declaration / Statement)* EOF
Declaration   <- AgentDecl / TaskDecl / TypeDecl / ConnectStmt / UseStmt

# Statement — all statement forms, with ExprStmt last to avoid
# greedy matching of keyword-prefixed forms.
# Note: IfExpr and WhenExpr are expressions that double as statements
# via ExprStmt — they do not need their own Statement entries.
Statement     <- ReturnStmt / SelfAssign / AssignStmt / ForStmt / TryStmt
               / SendStmt / NotifyStmt / ConfirmStmt / ShowStmt
               / ArchiveStmt / RememberStmt / ForgetStmt
               / RetryStmt / AfterStmt / RunStmt
               / ExprStmt

# === Top-level declarations ===
ConnectStmt   <- "connect" IDENT "via" IDENT (STRING)? ("{" (IDENT ":" Expr ","?)* "}")?
               / "connect" IDENT STRING
UseStmt       <- "use" STRING
               / "use" IDENT "from" STRING
               / "use" IDENT ("/" IDENT)+

# === Agent ===
AgentDecl     <- "agent" IDENT "{" AgentBody "}"
AgentBody     <- (RoleClause / ModelClause / ToolsClause / TeamClause
                / ConnectClause / MemoryClause / ConfigBlock / StateBlock
                / RulesBlock / TaskDecl / OnHandler / EveryBlock)*
ConnectClause <- "connect" IDENT "via" STRING

RoleClause    <- "role" STRING
ModelClause   <- "model" STRING
ToolsClause   <- "tools" "[" IDENT ("," IDENT)* "]"
TeamClause    <- "team" "[" IDENT ("," IDENT)* "]"
MemoryClause  <- "memory" ("none" / "session" / "persistent")
ConfigBlock   <- "config" "{" (IDENT ":" Expr ","?)* "}"
StateBlock    <- "state" "{" (IDENT ":" Type ("=" Expr)? ","?)* "}"
RulesBlock    <- "rules" "[" (STRING ","?)* "]"
OnHandler     <- "on" IDENT "(" Params? ")" Block
EveryBlock    <- "every" ScheduleExpr Block
ScheduleExpr  <- DurationExpr ("at" TimeExpr)?     # every 5.minutes
               / CalendarExpr ("at" TimeExpr)?      # every monday at 9am

# === Task ===
TaskDecl      <- "task" IDENT "(" Params? ")" ("->" Type)? Block
Params        <- Param ("," Param)*
Param         <- IDENT ":" Type ("=" Expr)?

# === Type ===
TypeDecl      <- "type" IDENT TypeParams? "=" EnumDef
               / "type" IDENT TypeParams? "{" FieldDef* "}"
TypeParams    <- "[" IDENT ("," IDENT)* "]"
EnumDef       <- EnumVariant ("|" EnumVariant)*
EnumVariant   <- IDENT ("{" FieldDef* "}")?         # simple or rich variant
FieldDef      <- IDENT ":" Type ","?

Type          <- TupleType
               / FuncType
               / IDENT ("[" Type ("," Type)* "]")?  "?"?
               / "{" FieldDef* "}"                   "?"?
TupleType     <- "(" Type "," Type ("," Type)* ")"  "?"?    # minimum 2 elements
FuncType      <- "(" (Type ("," Type)*)? ")" "->" Type

# === Expressions ===
Expr          <- NullCoalesce
NullCoalesce  <- PipeExpr ("??" PipeExpr)?
PipeExpr      <- OrExpr ("|>" OrExpr)*
OrExpr        <- AndExpr ("or" AndExpr)*
AndExpr       <- NotExpr ("and" NotExpr)*
NotExpr       <- "not"? CompExpr
CompExpr      <- AddExpr (("==" / "!=" / "<" / ">" / "<=" / ">=") AddExpr)?
AddExpr       <- MulExpr (("+" / "-") MulExpr)*
MulExpr       <- UnaryExpr (("*" / "/" / "%") UnaryExpr)*
UnaryExpr     <- ("-" / "not")? PostfixExpr
PostfixExpr   <- PrimaryExpr (FieldAccess / NullAccess / AssertAccess / Call / Index)*
FieldAccess   <- "." (IDENT / INT_LIT)     # .field or .0 (tuple index)
NullAccess    <- "?." IDENT
AssertAccess  <- "!." IDENT
Call           <- "(" Args? ")"
Index          <- "[" Expr "]"
Args           <- Expr ("," Expr)*

PrimaryExpr   <- Literal
               / "self"                                     # agent state access
               / IDENT
               / LambdaExpr
               / TupleLit / ListLit / MapLit / SetLit
               / IfExpr / WhenExpr
               / ClassifyExpr / ExtractExpr / SummarizeExpr
               / DraftExpr / TranslateExpr / DecideExpr
               / AskExpr / ConfirmExpr / RecallExpr / DelegateExpr
               / PromptExpr / HttpExpr / SqlExpr
               / FetchExpr / SearchExpr
               / ParallelExpr / RaceExpr
               / "(" Expr ")"

LambdaExpr    <- IDENT "=>" (Expr / Block)                     # single param
               / "(" LambdaParams? ")" "=>" (Expr / Block)    # multi param
LambdaParams  <- LambdaParam ("," LambdaParam)*
LambdaParam   <- IDENT (":" Type)?                             # type inferred from context

# === Literals ===
Literal       <- INT_LIT / FLOAT_LIT / STRING
               / "true" / "false" / "none" / "now"

# === Control Flow Expressions ===
# if/else is an expression: both branches produce a value.
# When used as a statement, the value is discarded.
IfExpr        <- "if" Expr Block ("else" (IfExpr / Block))?

# when is an expression: each arm produces a value.
WhenExpr      <- "when" Expr "{" WhenArm+ "}"
WhenArm       <- Pattern ("," Pattern)* ("where" Expr)? "=>" (Expr / Block)
Pattern       <- VariantPat / StructPat / TuplePat / IDENT / "_" / Literal
VariantPat    <- IDENT "{" (IDENT ("," IDENT)*)? "}"    # rich enum destructuring
StructPat     <- "{" (IDENT ":" Pattern ","?)+ "}"
TuplePat      <- "(" Pattern ("," Pattern)+ ")"

# === AI Primitives ===
# All AI primitives accept an optional trailing "using" STRING clause
# to override the agent's default model.
UsingClause   <- ("using" STRING)?

ClassifyExpr  <- "classify" Expr "as" EnumRef
                 ("considering" "[" CriteriaList "]")?
                 ("fallback" IDENT)?
                 UsingClause
EnumRef       <- IDENT / "[" IDENT ("," IDENT)* "]"
CriteriaList  <- Criteria ("," Criteria)*
Criteria      <- STRING "=>" IDENT

ExtractExpr   <- "extract" "{" FieldDef* "}" "from" Expr UsingClause

SummarizeExpr <- "summarize" Expr ("in" INT_LIT SumUnit)? ("format" SumFmt)?
                 ("fallback" Expr)? UsingClause
SumUnit       <- "sentence" / "sentences" / "line" / "lines"
               / "word" / "words" / "paragraph" / "paragraphs"
SumFmt        <- "bullets" / "prose" / "json"

DraftExpr     <- "draft" STRING ("{" DraftOpts "}")? UsingClause
DraftOpts     <- (IDENT ":" Expr ","?)*

TranslateExpr <- "translate" Expr "to" LangTarget UsingClause
LangTarget    <- IDENT / "[" IDENT ("," IDENT)* "]"

DecideExpr    <- "decide" Expr "{" DecideOpts "}" UsingClause
DecideOpts    <- (IDENT ":" Expr ","?)*

# === Escape Hatches ===
PromptExpr    <- "prompt" "{" (IDENT ":" Expr ","?)* "}" "as" Type   # "as T" is required
HttpExpr      <- "http" "{" (IDENT ":" Expr ","?)* "}"
SqlExpr       <- "sql" STRING ("with" "[" Expr ("," Expr)* "]")?

# === Other Expressions ===
AskExpr       <- "ask" "user" STRING ("options" EnumRef)? ("via" IDENT)?
RecallExpr    <- "recall" STRING ("limit" INT_LIT)?
DelegateExpr  <- "delegate" IDENT "(" Args? ")" "to" IDENT
FetchExpr     <- "fetch" (STRING / IDENT ("where" Expr)?)
SearchExpr    <- "search" (STRING / IDENT "for" STRING)
ParallelExpr  <- "parallel" Block
RaceExpr      <- "race" Block

# === Statements ===
# Note: IfExpr and WhenExpr double as statements via ExprStmt.
ReturnStmt    <- "return" Expr?
AssignStmt    <- AssignTarget (":" Type)? "=" Expr
SelfAssign    <- "self" "." IDENT "=" Expr           # agent state mutation
AssignTarget  <- IDENT / DestructPattern
DestructPattern <- "{" DestructField ("," DestructField)* "}"   # struct
                 / "(" IDENT ("," IDENT)+ ")"                   # tuple
DestructField <- IDENT (":" IDENT)?                # {x} or {x: renamed}
ExprStmt      <- Expr
RunStmt       <- "run" IDENT ("in" "background")?
SendStmt      <- "send" Expr "to" SendTarget ("via" IDENT)?
SendTarget    <- IDENT ("." IDENT)*                # dotted path only
               / IDENT "(" Args? ")"               # function call
ArchiveStmt   <- "archive" Expr
NotifyStmt    <- "notify" "user" Expr ("via" IDENT)?
ConfirmExpr   <- "confirm" "user" Expr ("via" IDENT)?
ConfirmStmt   <- ConfirmExpr "then" Statement
ShowStmt      <- "show" "user" Expr
RememberStmt  <- "remember" Expr
ForgetStmt    <- "forget" (STRING / "older_than" DurationExpr)
RetryStmt     <- "retry" INT_LIT "times" ("with" RetryStrategy)? Block
RetryStrategy <- "backoff" / "delay" DurationExpr
AfterStmt     <- "after" DurationExpr Block
ForStmt       <- "for" (IDENT / DestructPattern) "in" Expr ("where" Expr)? Block
TryStmt       <- "try" Block CatchClause+
CatchClause   <- "catch" IDENT ":" Type Block

Block         <- "{" Statement* "}"

# === Collections ===
TupleLit      <- "(" Expr "," Expr ("," Expr)* ")"  # min 2 elements
ListLit       <- "[" (Expr ("," Expr)*)? "]"
MapLit        <- "{" (MapEntry ("," MapEntry)*)? "}"
MapEntry      <- (IDENT / STRING) ":" Expr
SetLit        <- "set" "[" (Expr ("," Expr)*)? "]"

# === Duration/Time ===
DurationExpr  <- (INT_LIT / FLOAT_LIT) "." DurationUnit
DurationUnit  <- "second" / "seconds" / "minute" / "minutes"
               / "hour" / "hours" / "day" / "days" / "week" / "weeks"
CalendarExpr  <- "day" / "hour" / "monday" / "tuesday" / "wednesday"
               / "thursday" / "friday" / "saturday" / "sunday"
TimeExpr      <- INT_LIT ("am" / "pm")       # e.g., 9am, 6pm

# === Terminals ===
INT_LIT       <- [0-9]+
FLOAT_LIT     <- [0-9]+ "." [0-9]+
STRING        <- '"' StringChar* '"'
               / '"""' MultiLineChar* '"""'          # triple-quoted multi-line string
StringChar    <- '\\' EscapeSeq / '{' Expr '}' / [^"\\{]
EscapeSeq     <- '"' / '\\' / 'n' / 't' / 'r' / '{' / '}'   # \", \\, \n, \t, \r, \{, \}
MultiLineChar <- '\\' EscapeSeq / '{' Expr '}' / [^"\\{] / NEWLINE
IDENT         <- [a-zA-Z_] [a-zA-Z0-9_]*
```

---

## 22. IDE & Tooling Contract

Every language feature is designed with IDE support in mind. This section specifies what a compliant Keel language server must provide.

### 22.1 Autocomplete

| Context | What the IDE shows |
|---------|-------------------|
| After `classify X as ` | All in-scope enum types + `[` for inline enum |
| After `email.` | Fields of the structural type of `email` |
| After `delegate ... to ` | All in-scope agent names |
| After `when urgency { ` | All variants of the `urgency` enum, marking covered/uncovered |
| After `fetch ` | `"` for URL, or connected source names |
| After `via ` | Available channels (slack, email, terminal) |
| After `env.` | Known environment variable names from `keel.config` |
| Inside `config { }` | Valid config keys: temperature, max_tokens, retry, timeout, max_cost_per_request, require_confirmation |
| After `rules [` | Existing rule strings for reference; no autocomplete for content |
| After `Action.` | All variants of the `Action` enum (for rich enum construction) |
| After `self.` | All `state` fields of the enclosing agent |
| After `using ` | Known model names from `keel.config` and agent declarations |
| After `use ` | Available packages, local `.keel` files |

### 22.2 Hover Information

| Hover target | What the IDE shows |
|-------------|-------------------|
| Variable name | Inferred type, e.g., `urgency: Urgency` |
| Task name | Full signature: `task triage(email: EmailInfo) -> Urgency` |
| Agent name | Role string + model + tools |
| AI keyword | Return type + grammar synopsis |
| `fallback` value | The enum type and the fallback variant |
| `using` clause | Model name, agent's default model for comparison |
| `self.field` | Field type from agent's `state` block |
| `??` expression | Left type, right type, result type |

### 22.3 Diagnostics (Error Squiggles)

| Diagnostic | Severity | Example |
|-----------|----------|---------|
| Non-exhaustive `when` | Error | Missing enum variant |
| Nullable access without `?.` or `??` | Error | `result.value` when `result: T?` |
| Type mismatch in assignment | Error | `x: int = "hello"` |
| Unknown agent in `delegate` | Error | `delegate foo to NonExistent` |
| Unused variable | Warning | `x = compute()` never read |
| Unhandled nullable AI result | Error | `classify` result used without null handling |
| Unreachable `when` arm | Warning | Wildcard `_` before specific cases |
| `self` outside agent | Error | `self.count` in a top-level task |
| Missing `_` in non-enum `when` | Error | `when status_code { 200 => ... }` without wildcard |
| State mutation without `self.` | Warning | `count = count + 1` in agent (valid shadowing, but likely intended `self.count`) |
| Unreachable `catch` clause | Warning | `catch err: NetworkError` after `catch err: Error` |
| Missing `prompt as` | Error | `prompt { ... }` without `as T` clause |

### 22.4 Go-to-Definition

| Symbol | Navigates to |
|--------|-------------|
| Agent name | `agent X { ... }` declaration |
| Task name | `task X(...) { ... }` declaration |
| Type name | `type X = ...` or `type X { ... }` declaration |
| `extern` task | The `extern task` declaration (with a secondary jump to the source file) |
| Imported symbol | The declaration in the source file |

### 22.5 Refactoring

| Refactoring | Scope |
|------------|-------|
| Rename agent | All files that reference, delegate to, or import the agent |
| Rename task | All call sites, delegate expressions, pipeline uses |
| Rename enum variant | All `when` arms, `classify` expressions, comparisons |
| Extract task | Select code block -> wrap in a new `task` declaration |

---

## 23. Future Enhancements (Deferred)

The following enhancements were identified during design review but are deliberately deferred. They add complexity without proven user need at the current stage.

### 23.1 IDE: Inlay Hints (target: v1.0 LSP)

Inlay hints — inline type annotations displayed by the IDE for inferred types — are essential for a language that leans heavily on inference. The language server should display:

| Context | Inlay hint |
|---------|-----------|
| Variable binding without annotation | `: Urgency` after variable name |
| Task return type when omitted | `-> str` after parameter list |
| Pipeline intermediate types | Type between `|>` stages |
| `fallback` value type | Enum type of the fallback variant |

**Why deferred:** Inlay hints are a language server feature, not a language grammar feature. They require a working type checker (v0.1) and LSP (v1.0) before they can be implemented. Specifying them now risks over-constraining the LSP before real usage reveals what developers actually need displayed.

### 23.2 IDE: Semantic Highlighting Tokens (target: v1.0 LSP)

AI primitives (`classify`, `draft`, etc.) should have distinct semantic token types so themes can color them differently from regular keywords. A developer scanning code should instantly distinguish "this line invokes the LLM" from "this is pure logic." Proposed semantic token types:

- `ai_keyword` — AI primitive keywords (distinct from control flow)
- `model_name` — strings inside `using` clauses
- `env_variable` — `env.` references (distinct from regular field access)
- `connection_name` — identifiers after `fetch`/`send` that reference `connect` statements

**Why deferred:** Semantic highlighting requires a working LSP and is additive — it improves the experience but doesn't change language semantics.

### 23.3 Type Aliases (target: v1.0)

A general type alias mechanism would improve documentation and readability:

```keel
type Timestamp = datetime
type ContactEmail = str
type Handler = (Message) -> str
```

Type aliases are structurally transparent — `ContactEmail` and `str` are interchangeable. Their value is in hover info and documentation, not type safety. Nominal wrappers (where `ContactEmail` is *not* assignable to `str`) are a separate, more complex feature.

**Why deferred:** The `type` keyword currently supports struct definitions and enum definitions. Adding alias support (`type X = Y` where Y is a primitive or existing type, not an enum) requires distinguishing `type Urgency = low | medium | high` (enum) from `type Timestamp = datetime` (alias) in the parser. This is tractable but adds parser complexity that isn't justified until real code demonstrates the need.

### 23.4 Embedding Protocol (target: v1.0)

For Keel to be adopted incrementally, it must be embeddable in existing systems. A host application should be able to invoke Keel agents without the CLI:

- **Library interface:** `keel_runtime::run("agent.keel", input)` callable from Rust, with C ABI wrappers for Python/Node/Go
- **JSON-over-stdio protocol:** A subprocess protocol where the host sends JSON input on stdin and receives JSON output on stdout, enabling any language to embed Keel agents

This is the SQLite model: not replacing existing systems, but embedding inside them.

**Why deferred:** The v0.1 focus is the CLI (`keel run`). The embedding protocol depends on a stable runtime API, which will emerge from v0.1 implementation. Defining the protocol now would be speculative.

### 23.5 `parallel collect` — Partial Results (target: v2.0)

The current `parallel` block fails entirely if any branch fails. A `parallel collect` variant would wrap each branch result in `Result[T]`, allowing the caller to decide whether partial results are acceptable:

```keel
results = parallel collect {
  urgency   = delegate triage(email) to Classifier
  sentiment = delegate analyze(email) to SentimentBot
}
# results.urgency: Result[Urgency], results.sentiment: Result[Sentiment]
```

**Why deferred:** The all-or-nothing semantics of `parallel` are safe and simple. `parallel collect` adds a new block form with different typing semantics. Real multi-agent workflows (v2.0) will reveal whether partial results are a common need or an edge case.

### 23.6 `where` Clause Disambiguation (target: v1.0 docs)

The `where` keyword appears in four syntactic contexts with different semantics:

| Context | Semantics | Example |
|---------|-----------|---------|
| `for ... where` | Boolean filter on loop variable | `for e in emails where e.unread` |
| `fetch ... where` | Connection-specific predicate | `fetch email where unread` |
| `when` arm guard | Boolean guard on pattern | `escalate { reason, _ } where ...` |
| Generic constraints (future) | Type constraint | `where T: Hashable` |

The parser disambiguates by syntactic position: `where` after `in expr` is a loop filter, after a `fetch` source is a connection predicate, after a `when` pattern is a guard. These are all unambiguous in the grammar. A formal disambiguation section with examples should be added to the spec when the v1.0 grammar is finalized.

**Why deferred:** All four uses are already unambiguous in the PEG grammar (Section 21). The disambiguation is a documentation concern, not a grammar concern.

---

*This specification is a living document. Syntax and semantics will evolve as the language matures through its roadmap phases.*
