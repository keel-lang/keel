# Release Notes

> **Alpha.** Keel is pre-1.0. Breaking changes are expected between 0.x releases.

## Unreleased

### Sets deduplicate, and maps and sets can be extended

`set[T]` used to share the list representation, which meant it wasn't really
a set: `set[1, 1, 2]` held three elements. A set is now its own value type
that deduplicates by the same `==` used everywhere else, keeping
first-insertion order.

```keel
ids = set[3, 1, 3, 2]
ids.count()      # 3
"{ids}"          # set[3, 1, 2] — insertion order, no sorting
typeof(ids)      # "set"
```

Element types stay unrestricted, unlike map keys — `set[float]` and sets of
structs are legal, and structs deduplicate by field values:

```keel
type Point { x: int, y: int }

a: Point = {x: 1, y: 2}
b: Point = {x: 1, y: 2}
set[a, b].count()   # 1
```

Alongside that, maps and sets gained mutation methods, and sets gained the
read methods the spec had promised but nothing implemented:

```keel
stock: map[str, int] = {apples: 12}
stock = stock.insert("pears", 5)   # insert overwrites an existing key

ids = set[1, 2]
ids = ids.add(3)                   # set[1, 2, 3]
ids = ids.add(2)                   # unchanged — already present
ids.contains(3)                    # true
ids.count()                        # 3
```

Both follow the `list.push` rule: they return a **new** container rather than
modifying the receiver, so `ids.add(3)` on its own is a no-op — bind the
result. `set.contains`/`.count`/`.is_empty` now infer proper types instead of
resolving to unknown, so a mistake like `n: int = ids.contains(1)` is caught
at check time.

Sets also accept the read-only list methods (`.map`, `.filter`, `.join`,
`.sort`, …) over their elements in insertion order, each returning a list or
a scalar, never a set. `.push` is not among them — use `.add`. And
`for x in someSet` type-checks now; previously only `...someSet` did, which
was the same capability arbitrarily split in two.

**Upgrading.** Four behaviors changed for programs already using sets:
`count()` drops duplicates; `typeof` reports `"set"` instead of `"list"`;
string interpolation and `to_str()` render `set[1, 2, 3]` rather than
`[1, 2, 3]` (`io.show` is unchanged); and a set no longer compares equal to a
list with the same elements.

### Tuple positional access — `pair.0`

Reading a tuple element by position is documented in the spec but never
parsed, so the spec's own example failed `keel check`. It works now:

```keel
use std/io

pair: (str, int) = ("hello", 42)
(name, count) = pair              # destructuring already worked
x = pair.0                        # str
io.show("{x} {pair.1} {name} {count}")   # hello 42 hello 42

maybe: (str, int)? = ("hi", 7)
io.show("{maybe?.0}")             # hi
```

Indices are bounds-checked when you compile, not when you run —
`pair.5` on a 2-tuple is a type error. And because `.N` means *tuple
position*, using it on a list or map tells you what to write instead:

```keel
xs: list[int] = [1, 2, 3]
# xs.0   # error: positional access `.0` is only valid on tuples;
         #        index `list[int]` with `[0]`
```

Nested access needs parentheses — `(t.0).1` rather than `t.0.1` — because
`0.1` lexes as a float literal. See [issue #185](https://github.com/keel-lang/keel/issues/185)
and the [Tuples guide](guide/types.md#tuples).

Destructuring (`(a, b) = pair`) and positional reads both compile through
the native backend as well, matching the interpreter byte for byte.

### `keel dap` — a step debugger over the Debug Adapter Protocol

Any DAP-speaking editor can now set breakpoints, step, and inspect variables
— including live agent `state` — in a running Keel program:

```keel
agent Counter {
  state {
    count: int = 0
  }

  task bump(n: int) -> int {
    result = n + 1        # set a breakpoint here
    return result
  }

  @on_start {
    self.count = self.bump(self.count)
  }
}

run(Counter)
```

```bash
keel dap counter.keel
```

It pauses the same interpreter `keel run`/`keel test` already use — no
separate debug build. `keel test --debug --filter <name>` debugs a single
test the same way. See [Debugging](./guide/debugging.md) for what's covered
in this first milestone and what's still a documented gap (code inside
`Async.spawn`, multi-frame variable inspection, and editor integration for
[vscode-keel](https://github.com/keel-lang/vscode-keel) are the notable
ones).

### `impl` method bodies are type-checked

The checker's validation walk never visited `impl` blocks, so a method body was
name-resolved but never type-inferred — `keel check` passed cleanly on any type
error inside one:

```keel
interface Greetable {
  task greet(self) -> str
}

type Person { name: str }

impl Greetable for Person {
  task greet(self) -> str {
    x: int = "oops"       # now: `x`: expected int, got str
    return self.nickname  # now: field `nickname` does not exist on `Person`
  }
}
```

Bodies are now checked exactly like task bodies, with `self` bound to the
implementing type: `self.field` carries the field's declared type, and `self`
can be passed anywhere a value of that type is expected. Two forms the
interpreter has always rejected now fail at check time instead of run time —
`self.field = value` (the receiver is passed by value) and `self.method(...)`
(agent syntax; it dispatches to the enclosing agent's task, never to another
`impl` method). See [Interfaces](./guide/interfaces.md#inside-a-method-body).

---

## v0.2.4 — 2026-06-21

### User-authored LLM providers — write a backend in Keel

For proprietary or self-hosted models the built-in backends don't cover, any
field-less type with `impl LlmProvider` is now a backend `ai.*` can dispatch
through:

```keel
use std/ai
use std/env
use std/http

type MyProvider {}
impl LlmProvider for MyProvider {
  task complete(self, req: CompletionRequest) -> str {
    key = env.get("MY_LLM_KEY")!
    http.post("https://my-llm.example/complete", body: { prompt: req.user })["text"]
  }
}

ai.install(MyProvider)        # program-wide, or `@provider MyProvider` per agent
```

`complete(self, req: CompletionRequest) -> str` returns the raw model text; Keel
applies its own prompt construction and output parsing on top, so `??`, `when`,
and the typed `AiError` / `AiSchemaError` behave the same as for built-in
backends. `CompletionRequest` is a built-in struct (`system`, `user`, `model`,
`max_tokens`). `ai.install(X)` and `@provider X` require `X` to implement
`LlmProvider` (a compile-time error otherwise), and a provider that calls `ai.*`
from inside its own `complete()` is rejected with an `AiError` instead of
recursing without bound. See
[User-authored providers](./config/llm-providers.md#user-authored-providers).

---

### Swappable LLM providers — Ollama, OpenAI, and Anthropic (Claude)

`ai.*` no longer hard-codes Ollama. Three backends ship built in and are selected
with no extra code, most-specific wins:

```keel
agent Researcher {
  @provider anthropic            # this agent's default backend…
  @model "claude-opus-4-8"        # …for bare model tags
  @limits { max_tokens: 1024 }
}

# Per-call override via the model-tag prefix — no @provider needed:
summary = ai.summarize(text, using: "openai:gpt-4o") ?? "(none)"
```

- **Per-call** — a `provider:` prefix on the model tag (`"openai:gpt-4o"`,
  `"anthropic:claude-opus-4-8"`, `"ollama:llama3"`), via `@model` or `using:`.
- **Per-agent** — `@provider ollama|openai|anthropic` sets the agent's default for
  bare tags. An unknown name is a compile-time error.
- **Per-program** — `KEEL_PROVIDER` sets the default backend (otherwise `ollama`).

OpenAI and Anthropic read `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`; a missing key
throws `AiError { reason: "provider" }`, never a silent fallback. `@limits {
max_tokens }` is now threaded into the request as the generation cap. You can
also write your own provider in Keel — see above and
[LLM Providers](./config/llm-providers.md).

---

### Interface conformance checks `impl` parameter types

When a struct's `impl` block satisfies an `interface`, the compiler now verifies
each method parameter's **type** — not just the parameter count and return type.
A parameter whose type differs from the interface signature is now a conformance
error in both `keel check` and `keel run`:

```keel
interface Fetcher {
  task fetch(self, url: str) -> str
}
type Client { id: int }
impl Fetcher for Client {
  task fetch(self, url: int) -> str { "x" }   # error: parameter `url` must be `str` but is `int`
}
```

`dynamic` in the interface's parameter position stays a wildcard — an `impl` may
narrow it to any concrete type, which is how `Comparable`/`Equatable` accept
`other: dynamic`. A concrete interface parameter type requires an exact match.
The check shares the same comparison path as the return-type check, so the
checker and the runtime always agree.

---

## v0.2.3 — 2026-06-18

### AI provider failures throw `AiError` instead of silently returning `none`

`ai.*` now separates *absence* from *failure*. `none` means the model genuinely
had no answer, no model is configured, or mock mode is active — handle it with
`??` or `when`. A real provider **failure** (Ollama unreachable, network error, or
a model that isn't mapped) now **throws** a typed `AiError` carrying a
machine-readable `reason`:

```keel
try {
  urgency = ai.classify(email.body, as: Urgency) ?? Urgency.medium
} catch err: AiError {
  io.notify("classifier {err.reason}: {err.message}")  # reason: "unavailable" | "provider"
  urgency = Urgency.medium
}
```

Previously these failures degraded to `none`, so `ai.classify(...) ?? Urgency.medium`
quietly produced `medium` whether the model had no answer **or the model was
down** — an outage was indistinguishable from a real classification. Now the `??`
default applies only to genuine absence; an outage surfaces with a reason you can
catch and act on.

`KEEL_LLM=mock` is unchanged in observable behavior — every `ai.*` call still
returns `none`, so existing `?? default` tests and examples keep working; mock now
models deterministic *absence* rather than a simulated failure. `AiSchemaError`
(unparseable output, with its `got` field) is unchanged.

---

## v0.2.2 — 2026-06-17

### `keel-lang` is now a layered crate workspace

The compiler is split from one ~41.6k-LOC crate into five that build in parallel
and recompile independently:

```
keel-syntax → keel-catalog → keel-compiler → keel-runtime → keel-lang
```

`keel-syntax` holds the lexer/AST/parser/formatter/linter; `keel-catalog` is a
zero-dependency leaf of stdlib method descriptors and `@tools` capability
metadata; `keel-compiler` holds HIR, the type checker, modules, and IDE queries;
`keel-runtime` is the interpreter plus stdlib namespaces (and owns the heavy I/O
dependencies); `keel-lang` is the published facade and `keel` binary.

**If you embed `keel-lang` as a library, nothing changes** — it re-exports
`ast`, `modules`, `catalog`, `diagnostics`, and `session` under their original
paths. The win is build locality: editing the parser rebuilds its tests in
~0.8s instead of inside the ~6.9s monolith, and no longer rebuilds
`rusqlite`/`reqwest`/`axum`. Clean full builds and release builds are unchanged.

### Parser upgraded to chumsky 0.13

`keel-syntax` moved from chumsky 0.9 to the current stable 0.13 parser API. This
is purely internal: the parser's public entry points, the grammar it accepts,
and its error and recovery diagnostics are all unchanged. If you embed
`keel-lang`, nothing changes.

### Container casts work at runtime

`as list[T]`, `as map[K, V]`, and tuple casts `as (T1, T2, …)` now execute. The
type checker already accepted them, but the interpreter only implemented scalar
conversions and raised `cannot cast to List(...)` for any container — so the
common `json.parse(body) as list[dynamic]` narrowing always failed at runtime.

```keel
use std/json

rows = json.parse(body) as list[dynamic]   # array → list
for row in rows {
  cells = row as list[dynamic]             # nested array → list
  close = (cells[4] as str).to_float() ?? 0.0
}

cfg   = json.parse(text) as map[str, dynamic]   # object → map
point = json.parse("[1,2]") as (int, int)       # array → tuple
```

A container cast asserts the runtime shape and recurses element-wise, raising on
a shape or element mismatch — `json.parse("42") as list[dynamic]` raises rather
than silently passing wrong-shaped data. A `dynamic` element/value type only
asserts the top-level shape. Tuples use parentheses (`(int, int)`), not
`tuple[...]`. See [Types → `as T`](./guide/types.md#type-coercions--as-t).

---

## v0.2.1 — 2026-06-13

### Struct pattern matching in `when`

Bind named fields from a struct value directly in `when` arms:

```keel
when signal {
  { price, volume } where price > 1000.0 and volume > 0.0 => "active"
  { price }         where price > 1000.0                  => "thin"
  _                                                        => "quiet"
}
```

An unguarded struct arm is total against a non-nullable struct — it matches any value of that struct type, so no separate `_` arm is required. The subject must be a struct and every named field must exist on it; a struct arm against an enum or a mistyped field name is a compile-time error rather than a silent `none` binding. See [Control Flow](./guide/control-flow.md#struct-patterns).

---

## v0.2.0 — 2026-06-12

### Deny-by-default `@tools`

Agent capabilities are now declared, never implied. A capability guards
*effects*: `ai`, `io`, `http`, `email`, `file`, `shell`, `db`, `search`,
and `env` require a declared `@tools` capability, and an agent without
`@tools` may call none of them. Pure-compute modules (`json`, `math`,
`time`, …) are never gated. `@tools all` is the explicit unrestricted
form. Uncovered direct calls fail at **compile time** with the
fix spelled out; calls reached through helpers raise `CapabilityError` at
runtime with the same guidance. See
[Agents & Attributes](./guide/agents.md#tools--capability-list).

### Module system: `use std/<name>` and local file imports

Keel programs can now span multiple files, and the standard library is
imported explicitly — the ambient PascalCase prelude is gone:

```keel
use std/io
use std/file
use "./validation.keel"
use Urgency from "./models.keel"

task load_config() -> str {
  file.read("config.json")
}

io.show("valid: {validation.email("ada@example.com")}")
```

- `use std/file` binds `file`; `use "./validation.keel"` binds `validation`
  (the file stem); `as` renames either; `use A, B as C from ...` pulls
  symbols unqualified.
- Top-level statements are the **implicit main**: they run only when the
  file is executed directly, never on import.
- `keel test file.keel` runs only that file's tests; imported modules
  contribute declarations (including test helpers), never their tests.
- Agent verbs are built into the language: `run`, `stop`, `send`,
  `delegate`, `broadcast` (the `Agent.*` namespace is gone).
- **Breaking:** `File.read(...)` and friends now fail with a migration
  hint — add `use std/file` and write `file.read(...)`. The bare `uuid()`
  free function is removed; use `std/uuid` and `uuid.v4()`.

See the new [Modules & Imports](./guide/modules.md) guide.

### Built-in test blocks

Keel now has a first language-level test runner:

```keel
use std/ai
use std/testing

type Severity = low | medium | critical

task classify(text: str) -> Severity {
    ai.classify(text, as: Severity) ?? Severity.low
}

test "mocked classify returns critical" {
    testing.mock(ai.classify).returns(Severity.critical)
    assert classify("payment outage") == Severity.critical, "expected critical"
}
```

Run tests with:

```bash
keel test triage.keel
```

Use `keel test triage.keel --filter classify` to run only tests whose names contain `classify`, `keel test triage.keel --list` to print matching test names without running them, `--fail-fast` to stop after the first failure, or `--quiet` to print only failures and the final summary. Passing a directory, such as `keel test examples/`, recursively runs `.keel` files with test blocks.

`use std/testing` brings the `testing` namespace into scope. `testing.mock(Namespace.method).returns(value)` overrides one prelude namespace method for the current test only. Repeating a mock target returns values in order, then repeats the final value. Mocked methods expose `Namespace.method.called`, `Namespace.method.call_count`, and `Namespace.method.called_with(...)` metadata inside the same test. `setup { ... }` prepares bindings for the current test body. `test "name" for case in cases { ... }` runs one indexed case per item in the `cases` list. `assert expr` requires a boolean expression and fails the test when it evaluates to `false`; `assert expr, "message"` customizes the failure message. Files with no test blocks print `0 tests found`. Result lines include per-test elapsed time, failed tests include source locations when available, and the final summary includes total suite time. `keel run` ignores test blocks, so production execution remains unchanged.

`test`, `setup`, and `assert` are contextual syntax words rather than new reserved keywords.

---

## v0.1.33 — 2026-06-07

### Typed runtime errors for all stdlib namespaces

Every stdlib namespace that can fail now raises a named, catchable error type. Previously, namespace errors embedded type names in message strings (`"CsvError: ..."`) without a consistent policy — `try/catch` could only match `Error` as a fallback and couldn't distinguish causes.

Now each failure domain has its own type: `FileError`, `CsvError`, `DbError`, `MathError`, `MemoryError`, `EmailError`, `HttpError`, `ShellError`, `JsonError`, `EnvError`, `AiError`, `AiSchemaError`, `CapabilityError`, `TimeoutError`, `DeadlineError`, `UserRaised`, and `RuntimeBusy`. The `raise` statement now produces `UserRaised` instead of a plain error.

`catch e: Error` continues to work as a catch-all for all types.

```keel
use std/control
use std/csv
use std/file
use std/io

agent A {
    @tools [file, io]
    @on_start {
        try {
            data = file.read("config.json")
        } catch e: FileError {
            io.show("file error: {e.message}")
        }

        try {
            rows = csv.parse_records("{}")
        } catch e: CsvError {
            io.show("CSV error: {e.message}")
        }

        try {
            control.with_timeout(5.seconds, () => { "ok" })
        } catch e: TimeoutError {
            io.show("timeout error: {e.message}")
        }

        try {
            raise "quota exceeded"
        } catch e: UserRaised {
            io.show("raised: {e.message}")
        }

        stop(self)
    }
}
run(A)
```

See the [Error Handling guide](guide/error-handling.md) for the full type registry.

---

## v0.1.32 — 2026-06-06

### All syntax errors now reported at once

The parser previously stopped at the first syntax error. If a file contained errors in two separate tasks, only the first was shown — requiring multiple edit-compile cycles to clear a freshly-written file.

The parser now uses declaration-level error recovery: when a declaration fails to parse, it skips forward to the next declaration keyword and continues. All accumulated errors are reported together — both in the LSP (as individual diagnostics on the affected lines) and in the CLI (as labeled source spans in a single miette report).

```keel
task greet(name: str) {
    result =       # ← error 1
}

task farewell(name: str) {
    reply =        # ← error 2 — now visible without fixing error 1 first
}
```

---

## v0.1.31 — 2026-06-04

### Bounded event queue and backpressure

The interpreter event queue is now bounded (default 1024; set `KEEL_EVENT_QUEUE_CAPACITY=<n>` to tune).
Each producer uses non-blocking delivery with explicit overflow policy:

| Producer | Overflow behaviour |
|---|---|
| `schedule.every` / `schedule.cron` (recurring ticks) | Drop (coalesce) — next tick fires on time |
| `schedule.after` / `schedule.at` / `schedule.every` first fire (one-shot) | Wait for queue space — delivery is guaranteed |
| `http.serve` | `503 Service Unavailable` returned to the HTTP client |
| `Agent.send` / `Agent.delegate` / `Agent.broadcast` | `RuntimeBusy` error raised |

`RuntimeBusy` is a typed, catchable error:

```keel
try {
    send(Worker, payload)
} catch e: RuntimeBusy {
    io.show("queue full — dropped: {e.message}")
}
```

See [Agent Communication](./guide/agent-communication.md) for backpressure strategies.

---

## v0.1.30 — 2026-06-03

### Bounded event queue and backpressure

The interpreter event queue is now bounded (default 1024; set `KEEL_EVENT_QUEUE_CAPACITY=<n>` to tune).
Each producer uses non-blocking delivery with explicit overflow policy:

| Producer | Overflow behaviour |
|---|---|
| `schedule.every` / `schedule.cron` (recurring ticks) | Drop (coalesce) — next tick fires on time |
| `schedule.after` / `schedule.at` / `schedule.every` first fire (one-shot) | Wait for queue space — delivery is guaranteed |
| `http.serve` | `503 Service Unavailable` returned to the HTTP client |
| `Agent.send` / `Agent.delegate` / `Agent.broadcast` | `RuntimeBusy` error raised |

`RuntimeBusy` is a typed, catchable error:

```keel
try {
    send(Worker, payload)
} catch e: RuntimeBusy {
    io.show("queue full — dropped: {e.message}")
}
```

See [Agent Communication](./guide/agent-communication.md) for backpressure strategies.

### Nominal struct type identity

Two declared struct types with identical fields are no longer interchangeable.
`type Point { x: int, y: int }` and `type Offset { x: int, y: int }` are now
distinct types — passing an `Offset` where a `Point` is expected is a compile-time
error.

Anonymous struct literals remain structurally compatible with any named type that
has the required fields. `p: Point = { x: 1, y: 2 }` continues to work.

`impl` dispatch is now based entirely on the value's declared type tag. To enable
`impl` methods on a list of struct values, declare the list type so elements are
tagged at assignment time:

```keel
scores: list[Score] = [{ val: 30 }, { val: 10 }, { val: 20 }]
sorted = scores.sort()   # Comparable.compare is used
```

Error messages for named struct mismatches now include the declared type name
(`expected Score, got Point`) rather than the generic `struct`.

### Read-only HIR semantic index

The checker and LSP now consume a read-only high-level intermediate representation
lowered from the parser AST. HIR assigns stable symbol IDs to declarations and
bindings, records resolved references for editor navigation, and classifies brace
literals as structs or maps once when an expected type is available. Agent-local
`self.task(...)` references and `self.field` reads/writes are also linked to their
symbols during lowering.

This is an internal compiler change. Keel syntax and runtime behavior are unchanged,
and the tree-walking interpreter intentionally remains AST-backed in this first phase.

### Structured checker diagnostics

Type-checker diagnostics are now structured internally. `keel check` and the LSP still show the same user-facing messages, but undefined names, type mismatches, wrong arity, and non-exhaustive `when` checks now carry typed data and precise source spans for editor tooling.

### Accurate interpolation diagnostics

Type-checker diagnostics inside string interpolation slots now underline the correct source range. This also applies to nested and triple-quoted strings.

### Strict runtime arguments for data APIs

Runtime APIs now enforce their declared inputs. Data APIs such as
`file.read(path: str)`, `cache.get(key: str)`, `json.parse(text: str)`, and
`shell.run(cmd: str)` reject non-string values with a clear error instead of silently
formatting them as strings. Required sleep and value-method arguments now also raise
an error when omitted instead of silently acting as no-ops or empty strings.
Display-oriented APIs such as interpolation, `io.*`, and `log.*` continue to format
arbitrary values.

### Stable diagnostic codes for typed runtime errors

`AiError`, `AiSchemaError`, and `FileError` now expose a stable machine-readable
code via `miette`'s diagnostic protocol. When an error propagates uncaught to the
CLI, the code appears in the output — for example `keel::runtime::FileError`. The
code follows the pattern `keel::runtime::<TypeName>` and can be inspected by tooling
without parsing the error message.

`FileError` is now a typed runtime error: `file.*` failures can be caught by type
name (`catch e: FileError`), matching the behavior of `AiError` and `AiSchemaError`.
`catch e: Error` continues to catch all failures as before.

---

## v0.1.30 — 2026-05-29

### AST `Node<T>` migration

Every `(T, Span)` tuple in the public AST has been replaced by `Node<T>` — a named struct
with `.kind` (the wrapped value) and `.span` (the source range). This is a purely internal
refactor: no Keel source syntax or runtime behaviour is affected.

**For contributors:** AST traversal code now uses uniform field access (`node.kind`,
`node.span`) instead of tuple indexing (`.0`, `.1`). Synthetic nodes produced by the prelude
and test helpers use `Node::synthetic(kind)`, which stores a `0..0` sentinel span.

### Expression spans + annotation-precise type errors

Every expression in the AST now carries its exact source byte range. The most
visible benefit: when a `let` binding has an explicit type annotation and the
inferred type doesn't match, the error caret points at the annotation — not the
whole statement.

```
error: `n`: expected int, got str
  --> example.keel:2:5
   |
 2 |   n: int = "hello"
   |      ^^^  ← caret lands here, not at the start of the line
```

See the [Types guide](guide/types.md#type-mismatch-diagnostics) for details.

### Richer type diagnostics — `Ty::Unknown` split

The single overloaded `Unknown` fallback type has been replaced with four semantically distinct variants that give the checker, strict mode, and contributors a precise vocabulary for why a type is not known:

| Variant | When it appears | Fires in `--strict`? |
|---|---|---|
| `dynamic` | Explicit `dynamic` annotation written by the programmer | **Never** |
| `Unknown(ExternalDynamic)` | Namespace method whose return depends on runtime data (`json.parse`, LLM outputs) | Yes |
| `Unknown(InferenceLimitation)` | Checker cannot cheaply infer the type (agent refs, unannotated lambdas, dispatch fallthrough) | Yes |
| `Unknown(UnsupportedFeature)` | Construct the checker does not yet implement | Yes |

**User-visible change — `--strict` and `json.parse`:**

`keel check --strict` no longer warns on `dynamic`-annotated bindings; it only warns on
`Unknown(_)` results. `let x: dynamic = json.parse("{}")` is clean in strict mode because
the programmer explicitly opted into the dynamic type. `let x = json.parse("{}")` (unannotated)
still fires `cannot infer type of 'x'` because the return type is now `Unknown(ExternalDynamic)`
rather than `dynamic`.

```keel
# always clean — explicit dynamic annotation
data: dynamic = json.parse(raw)

# clean in normal mode, flagged in --strict — unannotated binding
data = json.parse(raw)            # error in strict: cannot infer type of 'data'
```

Fix unannotated `json.parse` bindings with an explicit annotation or cast:

```keel
# annotate as dynamic (accept at call site)
data: dynamic = json.parse(raw)

# or narrow immediately to a concrete type
data = json.parse(raw) as Payload
```

---

### Type-safe `Agent.delegate` symbol form

Handler references are now first-class compile-time expressions.

**Symbol form** (new, preferred):

```keel
delegate(Worker.process, payload)
```

`Worker.process` is resolved at compile time. The checker validates that `process` is a
declared `on` handler on `Worker` and that `payload` matches the handler's parameter type.
A typo like `Worker.typo` is a compile-time error.

**String form** (legacy, now also validated for plain literals):

```keel
delegate(Worker, "process", payload)
```

Both forms are accepted. Prefer the symbol form for new code — handler renames update
the reference automatically.

---

## v0.1.29 — 2026-05-27

### `Csv` namespace — CSV serialization

Three functions cover the full round-trip. No `@tools` annotation required.

- `csv.parse(text) -> list[list[str]]` — every row as a list of strings; the caller decides whether to treat the first row as headers.
- `csv.parse_records(text) -> list[map[str, str]]` — first row becomes header keys; remaining rows become maps.
- `csv.stringify(rows) -> str` — converts `list[list[str]]` to RFC 4180 CSV; cells containing commas, quotes, or newlines are automatically quoted.

```keel
raw    = "symbol,price,volume\nBTC,67000,1234.5\nETH,3500,5678.9"
trades = csv.parse_records(raw)
for trade in trades {
    log.info("{trade["symbol"]} @ {trade["price"] as float:.2f}")
}

rows = [["symbol", "price"], ["BTC", "67000"], ["ETH", "3500"]]
text = csv.stringify(rows)
```

### `Db` namespace — SQLite database access

Query and modify SQLite databases. `db.connect(url)` opens a database and returns a connection with `.query()` and `.exec()` methods. URLs use the `sqlite://` scheme.

- `db.connect(url: str) -> DbConnection` — opens a SQLite database
- `DbConnection.query(sql, params?) -> list[map[str, dynamic]]` — SELECT queries return rows as maps
- `DbConnection.exec(sql, params?) -> int` — INSERT/UPDATE/DELETE return affected row count

```keel
db = db.connect("sqlite://trades.db")
db.exec("CREATE TABLE IF NOT EXISTS trades (id TEXT, symbol TEXT, price REAL)")
db.exec("INSERT INTO trades VALUES (?, ?, ?)", ["t1", "BTCUSDT", 67000.0])
rows = db.query("SELECT symbol, price FROM trades WHERE symbol = ?", ["BTCUSDT"])
for row in rows {
    log.info("{row["symbol"]} @ {row["price"] as float:.2f}")
}
```

SQLite is bundled into the binary. Other backends (Postgres, MySQL) are planned for v0.2.

---

## v0.1.28 — 2026-05-26

### Typed map keys — `map[K, V]` key types enforced at compile time

Map keys must now be one of the three hashable primitive types: `str`, `int`, or `bool`. Using any other type as `K` is a compile-time error with an actionable message:

```keel
scores:  map[str,  int]  = {alice: 100}    # valid
lookup:  map[int,  str]  = {1: "one"}      # valid
flags:   map[bool, str]  = {true: "on"}   # valid

# Type errors caught at compile time:
# m: map[float, str]  — float is not a valid map key type (NaN violates hash equality; use int)
# m: map[str?,  int]  — nullable types cannot be used as map keys
# m: map[Point, str]  — struct/enum keys require interface Hashable (coming in v0.2)
```

The runtime map representation now stores keys as a typed union (`str | int | bool`), so `map[int, str]` and `map[bool, str]` maps work correctly at runtime — `.keys()` returns values of the declared key type.

### Struct spread-update `{ ...base, field: new }`

Creating a modified copy of a struct no longer requires re-stating every field. Use the spread-update syntax to copy all fields from a base and override only the ones that changed:

```keel
type Order { id: str, status: str, amount: float }

o: Order = { id: "ord-1", status: "pending", amount: 9.99 }
filled   = { ...o, status: "filled" }   # id and amount unchanged
copy     = { ...o }                     # full copy

type Config { host: str, port: int, debug: bool }
base: Config = { host: "localhost", port: 8080, debug: false }
prod = { ...base, host: "api.example.com" }
```

The `...base` spread must appear first and can appear only once. For struct bases, override field names must exist in the base struct (unknown fields are a compile-time error, and a runtime error on dynamic paths). For map bases (`map[K, V]`), any key may be added or overridden freely — values must match the map's value type. The result preserves the base's type tag, so `impl` dispatch works on the returned value. Spreading a `none` value raises at runtime.

### `Shell` namespace — subprocess bridge

Agents can now invoke external commands and capture their output via `shell.run`. The command is passed to `/bin/sh -c`, so pipes, redirects, and shell builtins work as expected. Requires `@tools [shell]`.

```keel
use std/io
use std/shell

agent Builder {
    @tools [shell]

    @on_start {
        r = shell.run("cargo test --quiet 2>&1")
        if r.exit_code != 0 {
            raise "tests failed:\n{r.stdout}"
        }
        io.show("all tests passed")
    }
}
run(Builder)
```

`shell.run` returns `{ stdout: str, stderr: str, exit_code: int }`. A non-zero exit code is not automatically an error — check it and raise yourself if needed. Spawn failures (e.g. `/bin/sh` not in `PATH`) do raise. Optional named args: `stdin: str` (piped to the process) and `cwd: str` (working directory).

### `Math` namespace

Transcendental and power functions are now available under the `Math` namespace. All functions accept `int` or `float` and return `float`. Domain errors (`math.sqrt(-1)`, `math.log(0)`) raise at runtime and can be caught with `try/catch`.

```keel
h   = math.sqrt(math.pow(3, 2) + math.pow(4, 2))  # 5.0
ln2 = math.log(2)                                   # ≈ 0.693
s   = math.sin(math.PI() / 6.0)                    # 0.5
```

| Function | Returns | Notes |
|---|---|---|
| `math.PI()` | `float` | π |
| `math.E()` | `float` | e |
| `math.sqrt(x)` | `float` | Raises if `x < 0` |
| `math.pow(x, y)` | `float` | |
| `math.exp(x)` | `float` | e^x |
| `math.log(x)` | `float` | Natural log; raises if `x ≤ 0` |
| `math.log2(x)` | `float` | Raises if `x ≤ 0` |
| `math.log10(x)` | `float` | Raises if `x ≤ 0` |
| `math.sin(x)` | `float` | Radians |
| `math.cos(x)` | `float` | Radians |
| `math.tan(x)` | `float` | Radians |
| `math.asin(x)` | `float` | Raises if `x ∉ [-1, 1]` |
| `math.acos(x)` | `float` | Raises if `x ∉ [-1, 1]` |
| `math.atan(x)` | `float` | Radians |
| `math.atan2(y, x)` | `float` | Two positional args |

### `time.epoch_ms()`

Returns the current Unix timestamp as an `int` in milliseconds — the Keel equivalent of JS `Date.now()`. Useful for database `BIGINT` columns, signed payloads, and any context where a raw numeric timestamp is more ergonomic than an RFC 3339 string.

```keel
ms = time.epoch_ms()   # e.g. 1705314600500
```

### `as T` coercions and `typeof()`

`expr as T` now performs real runtime coercions. `typeof(x)` is a new prelude function returning the runtime type name.

```keel
1 as float          # 1.0
1.7 as int          # 1  (truncated)
"3.14" as float     # 3.14
"abc" as int        # raises

type Point { x: int, y: int }
p: Point = { x: 1, y: 2 }
typeof(p)           # "Point"
typeof(42)          # "int"
```

See the [Types guide](guide/types.md) for the full coercion table.

### `list.sort(by: key_fn)`

`.sort()` now accepts an optional `by:` key function, consistent with `min(by:)` / `max(by:)`. No `impl Comparable` needed.

```keel
by_price  = products.sort(by: p => p.price)          # cheapest first
by_name   = products.sort(by: p => p.name)           # alphabetical
by_rating = products.sort(by: p => 0.0 - p.rating)  # highest rated first
```

The key function must return `int`, `float`, or `str`. Ascending only; negate numeric keys for descending.

### String interpolation format specifiers

Slots now accept an optional format spec after a colon: `{expr:spec}`.

```keel
pi = 3.14159
io.show("{pi:.2f}")        # → "3.14"
io.show("{pi:>10.2f}")     # → "      3.14"
io.show("{"hi":<8}!")      # → "hi      !"
io.show("{"hi":^8}")       # → "   hi   "
n = 42
io.show("{n:.2f}")         # → "42.00"  (int auto-promoted to float)
io.show("{n:8}")           # → "      42" (bare width = right-align)
```

See the [String Interpolation guide](guide/strings.md) for the full spec.

---

## v0.1.27 — 2026-05-21

### `while` loops

Unbounded iteration is now supported. The condition must be `bool`; `break` and `continue` work identically to their `for`-loop counterparts.

```keel
n = 5
while n > 0 {
    io.show("tick: {n}")
    n -= 1
}

total = 0
i = 1
while true {
    total += i
    i += 1
    if total > 10 { break }
}
```

### Subscript access (`list[i]`, `str[i]`)

Lists and strings now support integer subscript syntax. Result type is `T` for `list[T]` and `str` for strings — no nullable wrapper. Out-of-bounds and negative indices raise a runtime error, so there is never ambiguity between a `none` value and a missing index:

```keel
items = ["alpha", "beta", "gamma"]
first = items[0]   # "alpha"
mid   = items[1]   # "beta"
# items[99]        # runtime error: index 99 out of bounds (length 3)

word = "keel"
ch   = word[0]     # "k"
# word[10]         # runtime error: string index 10 out of bounds (length 4)
```

Use `len()` to guard dynamic indices; `.first()` / `.last()` remain available when you want a nullable fallback.

### User-defined interfaces

Any user can now declare a named interface and implement it for a struct type. The compiler validates conformance — missing methods, wrong arity, and wrong return types are all compile-time errors:

```keel
use std/io

interface Printable {
  task print(self) -> str
}

type Point { x: float, y: float }

impl Printable for Point {
  task print(self) -> str { "({self.x}, {self.y})" }
}

p: Point = { x: 1.5, y: 2.0 }
io.show(p.print())   # → "(1.5, 2.0)"
```

- Interface declarations and their `impl` blocks can appear in any order in the same file.
- Impl methods take priority over built-in map methods — a user-defined `size()` method on a struct wins over the generic map `.size()` length accessor.
- Interface-as-type annotations (`task f(x: Printable)`) are not yet supported.

See the [Interfaces guide](guide/interfaces.md) for full documentation.

### Built-in interfaces: `Comparable`, `Equatable`, `Serializable`, `Iterable`

Four new built-in interfaces extend the standard library with user-driven behaviour:

**`Comparable`** — sort and compare user-defined types.

```keel
type Score { val: int }
impl Comparable for Score {
  task compare(self, other: Score) -> int { self.val - other.val }
}
items = [{ val: 30 }, { val: 10 }, { val: 20 }]
sorted = items.sort()   # ascending by val
lo = items.min()        # { val: 10 }
hi = items.max()        # { val: 30 }
```

**`Equatable`** — typed equality check alongside structural `==`.

```keel
use std/io

type Point { x: int, y: int }
impl Equatable for Point {
  task equals(self, other: Point) -> bool {
    self.x == other.x and self.y == other.y
  }
}
a: Point = { x: 1, y: 2 }
b: Point = { x: 1, y: 2 }
io.show("{a.equals(b)}")   # → true
```

**`Serializable`** — override `json.stringify` for a type.

```keel
use std/io
use std/json

type Event { name: str, score: int }
impl Serializable for Event {
  task to_json(self) -> str { "name={self.name};score={self.score}" }
}
e: Event = { name: "goal", score: 3 }
io.show(json.stringify(e))   # → "name=goal;score=3"
```

**`Iterable`** — use a struct in a `for` loop.

```keel
use std/io

type Range { lo: int, hi: int }
impl Iterable for Range {
  task items(self) -> list[int] {
    result: list[int] = []
    i = self.lo
    while i <= self.hi {
      result += [i]
      i += 1
    }
    result
  }
}
for n in Range { lo: 1, hi: 3 } { io.show("{n}") }   # → 1, 2, 3
```

> `Iterable` materialises the full list before iteration — it is not a generator protocol.

See the [Interfaces guide](guide/interfaces.md#built-in-interfaces) for the complete reference.

### `impl Stringable for Type` — custom string interpolation

User-defined struct types can now participate in `"{...}"` interpolation by implementing the `Stringable` interface.

The `impl` keyword introduces the block; `self` inside the block is the receiver value:

```keel
use std/io

type Point {
  x: float
  y: float
}

impl Stringable for Point {
  task to_str(self) -> str {
    "({self.x}, {self.y})"
  }
}

p: Point = { x: 3.0, y: 4.0 }
io.show("Origin to {p}")   # → "Origin to (3.0, 4.0)"
s = p.to_str()             # explicit call
```

- `impl` is a new reserved keyword.
- Any struct type can implement `Stringable` — each type gets its own `impl` block.
- Values that do not implement `Stringable` still render via their built-in display representation, so existing programs are unaffected.

See the [String Interpolation guide](guide/strings.md#stringable-interface--custom-interpolation) for the full reference.

### Fixes

- **Impl dispatch is now deterministic.** Calling an impl method on a struct value previously used field-set matching — if two types declared the same fields (e.g. `Point` and `Vec2` both `{x, y}`), the wrong impl could be selected. Struct values are now type-tagged at their binding site and impl dispatch is a direct O(1) lookup. `ai.extract(... as: T)` results are tagged with `T` automatically.

---

## v0.1.26 — 2026-05-20

### Numeric value methods

`.abs()`, `.floor()`, `.ceil()`, and `.round()` are now available on `int` and `float` values.
The return type always matches the receiver — `float` methods return `float`, `int` methods return `int`.
`floor`, `ceil`, and `round` are identity no-ops on `int`.

```keel
price = -3.75
price.abs()           # 3.75
price.abs().ceil()    # 4.0
count = -5
count.abs()           # 5    — int stays int
```

### `Random` namespace

`Random` is now available in the prelude for non-cryptographic pseudo-random values:

```keel
roll = random.int(min: 1, max: 6)
sample = random.float()
enabled = random.bool()
```

Use `Random` for simulation, sampling, games, and similar non-security work. `random.int`
uses an inclusive `min:` / `max:` range and raises a runtime error if `min > max`.

### `Uuid` type and namespace

`Uuid` is now a distinct runtime/type-checker value with factories, parsing, formatting, and the
top-level `uuid()` alias:

```keel
id: Uuid = uuid.v4()
trace = uuid.v7()
site = uuid.v5(ns: uuid.DNS, name: "keel-lang.dev")
parsed = uuid.parse("f47ac10b-58cc-4372-a567-0e02b2c3d479")
simple = id.format(as: "simple")
version = id.version()
```

`uuid.DNS`, `uuid.URL`, `uuid.OID`, and `uuid.X500` are built-in namespace constants for deterministic
version-5 UUIDs. Interpolation displays UUIDs in lowercase hyphenated form.

### `Crypto` namespace

`Crypto` is now available for security-sensitive hashes, HMACs, tokens, and random bytes:

```keel
digest = crypto.sha256("hello")
sig = crypto.hmac_sha256("message", key: secret)
token = crypto.token(bytes: 32)
bytes = crypto.random_bytes(16)
```

`Crypto` exposes fixed safe SHA-2 methods: `sha224`, `sha256`, `sha384`, `sha512`,
`sha512_224`, and `sha512_256`, with matching `hmac_` methods. Legacy MD5 and SHA-1 are not exposed.
`crypto.token` returns hex, so `bytes: 16` produces a 32-character token. Use `Random` only for
non-security work.

---

## v0.1.25 — 2026-05-19

### Variadic parameters (`...param: T`) and spread (`...expr`)

Tasks can now accept any number of positional arguments through a rest-parameter:

```keel
task greet(...names: str) -> str {
  result = ""
  for n in names { result += n + " " }
  result
}

greet("Alice", "Bob")    # names = ["Alice", "Bob"]
greet()                  # names = []
```

Spread a `list[T]` or `set[T]` at any call site with `...`:

```keel
xs = ["Dave", "Eve"]
greet("Alice", ...xs)    # ["Alice", "Dave", "Eve"]
greet(...xs, ...xs)      # merge two lists
```

Passing `list[T]` without `...` to a variadic param is a compile-time type error.

See [Tasks → Variadic parameters](guide/tasks.md#variadic-parameters).

### `min` and `max` prelude free functions

`min` and `max` are now available as global free functions — no namespace prefix needed.
They accept variadic positional arguments, an optional `by:` key-selector lambda, and
return `none` when called with zero items:

```keel
min(3, 1, 4)                           # 1
max("banana", "apple", "cherry")       # cherry

scores = [4, 9, 2, 7]
min(scores)                            # 2 — single list auto-spread
max(...scores, 99)                     # 99

people = [{name: "Alice", age: 30}, {name: "Bob", age: 25}]
youngest = min(people, by: p => p.age) # {name: "Bob", age: 25}
oldest   = max(people, by: p => p.age) # {name: "Alice", age: 30}
```

See [Prelude → Free Functions](guide/stdlib.md#free-functions).

### `File` namespace complete — `mkdir`, `remove`, `copy`, `move`, `glob`, `mktemp`

The `File` namespace now covers all common filesystem operations:

```keel
file.mkdir("output/reports")              # create directory (and parents)
file.copy("template.txt", "output/r.txt") # copy a file; parent dirs created automatically
file.remove("tmp/scratch.txt")            # delete file or directory (recursive for dirs)
file.move("draft.txt", "final.txt")       # rename / move; creates dst parent dirs
paths = file.glob("output/*.txt")         # list[str] of matching paths; empty list if none match
tmp = file.mktemp()                       # create a temp file; caller removes it
tmpdir = file.mktemp(dir: true)           # create a temp directory
```

`file.remove` auto-detects files and directories — directories are removed recursively.
`file.glob` supports standard glob patterns (`*`, `?`, `**`). An invalid pattern raises a runtime
error; no matches returns an empty list. `file.mktemp` returns the path and leaves cleanup to the
caller — use `file.remove(path)` when done.

### Bug fix: `datetime` comparison and arithmetic in `keel check`

Comparisons and arithmetic involving `datetime` and `duration` values were incorrectly rejected
by the static type checker. `datetime > datetime`, `datetime + duration`, `datetime - duration`,
and `datetime - datetime` now all pass `keel check`.

---

## v0.1.24 — 2026-05-16

### Bug fixes

This release focuses on runtime safety and performance:

- Fixed potential deadlock in agent team queries through proper lock ordering.
- HTTP client now uses connection pooling, eliminating redundant TCP+TLS handshakes.
- File and memory I/O operations now use async-safe blocking thread pools, preventing event loop stalls.
- Fixed async task spawning to properly route events to the parent interpreter.
- Reduced allocations in string truncation and improved deduplication performance.

---

## v0.1.23 — 2026-05-16

### Augmented assignment — `+=`, `-=`, `*=`, `/=`

Mutate an existing variable in its nearest enclosing scope without shadowing it:

```keel
total = 0
for i in 1..5 {
    total += i
}
# total is 15
```

Also works on `self.field` in agent handlers: `self.counter += 1`.

### `raise expr` — throw errors symmetrically with `try/catch`

`raise` makes the error model complete — you can now throw as well as catch:

```keel
task validate(n: int) {
    if n < 0 {
        raise "n must be non-negative"
    }
    return n
}
```

Strings become the error message; other values are converted via their display representation. Caught by `try/catch err: Error` like any other runtime error.

### `break` and `continue` in `for` loops

`break` exits the nearest enclosing `for` loop immediately. `continue` skips the rest of the current iteration and advances to the next. Both are reserved keywords.

```keel
# stop as soon as the target is found
for item in items {
    if item == target {
        break
    }
    process(item)
}

# skip even numbers
for n in 1..100 {
    if n % 2 == 0 {
        continue
    }
    process_odd(n)
}
```

Both affect only the **innermost** loop. No labeled jumps in v0.1.

### `list.zip(other)` — pair two lists

`zip` pairs elements from two lists into a list of 2-element tuples, stopping at the shorter list.
The return type is inferred as `list[(T, U)]`, so tuple destructuring in `for` loops is fully typed.

```keel
names  = ["alice", "bob", "carol"]
scores = [90, 85, 95]

for (name, score) in names.zip(scores) {
    log.info("{name} scored {score}")
}
```

---

## v0.1.22 — 2026-05-14

### `@tools` guards now use `if`

Conditional tool entries in `@tools` now use `if` instead of `when`:

```keel
@tools [
  email.send  if self.confirmed,   # only after confirmation
  db.exec     if self.admin,       # admin only
]
```

This separates tool guards from the `when` pattern-match keyword. Existing code using `when self.*` in `@tools` must be updated to `if self.*`.

### `when` as an expression

`when` now works in expression position — the matched arm's value becomes the result.

```keel
label = when score {
  "A" => "excellent"
  "B" => "good"
  _   => "needs work"
}
```

All arms must produce the same type; a mismatch is a compile error. Exhaustiveness
rules are identical to the statement form. See the [control flow guide](guide/control-flow.md).

---

## v0.1.21 — 2026-05-14

### Nullable safety enforced at task call sites

The type checker now rejects nullable arguments passed to non-nullable parameters
at every task call site — top-level tasks, `self.task(...)`, and `self.method(...)`.

```keel
use std/env

task process(text: str) { ... }

task t() {
  val: str? = env.get("PROMPT")
  process(val)          # error: expected str, got str? — use `!` or `??`
  process(val!)         # ok
  process(val ?? "")    # ok
}
```

Named arguments are also checked: `process(text: val)` produces the same error.
Type mismatches at call sites (e.g. `int` passed where `str` is expected) are
caught as well.

---

## v0.1.20 — 2026-05-14

### Generic type declarations

Type declarations can now be parameterised over one or more type variables.

```keel
type Paginated[T] {
  items: list[T]
  page: int
  has_more: bool
}

type Pair[A, B] {
  first: A
  second: B
}

type Bag[T] = list[T]
```

The type checker resolves generic instantiations to concrete types.
`Paginated[str].items` is now checked as `list[str]` rather than `unknown`.
Generic enums register variant names for exhaustiveness checking.

### Function type literals

Function types can now be written inline and used as type aliases or parameter types.

```keel
type Handler      = (str) -> bool
type Reducer      = (str, int) -> str
type Thunk        = () -> none
type Predicate[T] = (T) -> bool

task t(h: Handler) {
  ok: bool = h("hello")
}
```

Zero-parameter and multi-parameter forms both work. Tuple syntax `(T1, T2)` without `->` continues to produce a tuple type.

### Generic enum variant field types

Bindings destructured from a generic enum variant now resolve to the substituted
field type instead of `unknown`.

```keel
use std/io

type Pair[A, B] =
  | both { first: A, second: B }
  | only_first { value: A }
  | only_second { value: B }

task t(p: Pair[str, int]) {
  when p {
    both { first, second } => {
      f: str = first    # type-checked as str
      s: int = second   # type-checked as int
    }
    only_first { value } => { io.show(value) }
    only_second { value } => { io.show("{value}") }
  }
}
```

### Generic task declarations

Tasks may now declare type parameters. Type arguments are inferred at every
call site — no explicit instantiation syntax is needed.

```keel
task identity[T](x: T) -> T { x }
task first[A, B](a: A, b: B) -> A { a }

task main() {
  s: str = identity("hello")   # T = str
  n: int = identity(42)        # T = int
  f: str = first("hi", 99)     # A = str, B = int
}
```

The formatter round-trips `task name[T, U](...)` syntax correctly.

---

## v0.1.19 — 2026-05-14

### Type checker improvements (additive)

Seven type checker gaps closed — no existing programs break.

- **`?.` propagates nullable** — `x?.field` on a nullable struct now types as `FieldType?` instead of `unknown`.
- **`??` unwraps nullable** — `x ?? fallback` now returns the inner type of `x`, not the fallback's type.
- **`ai.extract` / `ai.decide` with `as:`** — when an `as: T` argument is present, the return type is inferred as `T?` rather than `unknown?`. Downstream field accesses on the result are now checked.
- **Lambda block bodies** — `x => { ... }` now infers its return type from the last expression, matching expression-body lambdas.
- **`set[]` literal type** — `set[1, 2, 3]` now infers as `set[int]` instead of `list[int]`.
- **Implicit return checking** — when a task's last statement is an expression, its type is checked against the declared return type. Control-flow statements (`return`, `when`, `for`) are excluded to avoid false positives.
- **`if`-expression branch unification** — both branches of an `if` expression must produce the same concrete type. When one branch exits via `return`, the other branch's type is used.

---

## v0.1.18 — 2026-05-13

### Explicit agent task calls

Agent-owned tasks are now invoked as `self.task(...)`. Inside an agent body,
bare `task(...)` resolves only through lexical and top-level scope, while
cross-agent work stays on mailbox APIs such as `send(...)` and
`delegate(...)`.

---

## v0.1.17 — 2026-05-08

### Conditional `@tools` guards

`@tools` entries now support a `when` guard — a boolean expression evaluated at the start of each handler turn. Tools whose guard is false are blocked for that turn. Guards can access `self.*` state and call tasks that return `bool`.

Entries can gate a whole namespace or a specific method:

```keel
use std/db
use std/email

agent SupportBot {
  state { confirmed: bool = false, admin: bool = false }

  @tools [
    email.fetch,                           # always allowed
    email.send when self.confirmed,        # only after confirmation
    db.query,                              # always allowed
    db.exec   when self.admin,             # admin only
    http,                                  # whole namespace, always
  ]
}
```

Calling a blocked method raises a `CapabilityError` at runtime.

See the [Agents guide](guide/agents.md#tools--capability-list) for full details.

### Readonly state fields

State fields declared with `readonly` between the colon and the type are **compiler-enforced read-only**. Any `self.field = ...` assignment is a compile-time error.

```keel
state {
  turns:      int          = 0
  session_id: readonly str = "default-session"
}
```

See the [Agents guide](guide/agents.md#readonly-fields) for full details.

---

## v0.1.16 — List & String Enhancements

### Extended list operations

`list[T]` gains thirteen new methods: `any`, `all`, `find`, `reduce`, `sum`, `min`, `max`, `join`, `sort`, `reverse`, `flatten`, `take`, `skip`. See the [Collections guide](guide/collections.md) for full reference.

### New string methods

Seven new string methods added: `trim_start`, `trim_end`, `repeat`, `slice`, `index_of`, `to_int`, `to_float`. See the [String Interpolation guide](guide/strings.md) for the full method table.

### `keel check --strict`

`keel check --strict <file>` now rejects bindings whose type the checker cannot infer. Normal `keel check` still accepts them silently. Use `--strict` to verify that type annotations are complete. See [`keel check`](../cli/check.md) for details.

---

## v0.1.15 — Error Handling Rework

### Breaking: `fallback:` removed from all `ai.*` calls

`fallback:` is no longer a valid argument on any `ai.*` function. Replace every call site with `??` at the expression level:

| Before | After |
|---|---|
| `ai.classify(text, as: T, fallback: T.x)` | `ai.classify(text, as: T) ?? T.x` |
| `ai.summarize(body, in: 3, unit: sentences, fallback: "none")` | `ai.summarize(body, in: 3, unit: sentences) ?? "none"` |

The type-checker now always infers `T?` for `ai.classify` regardless of arguments.

### Two-tier failure model

`ai.*` calls have two distinct failure modes:

| Failure | Result | Handle with |
|---|---|---|
| Network failure / mock mode / timeout | Returns `none` | `??` |
| LLM returned output that doesn't match the schema | Throws `AiSchemaError` | `try/catch` |

### `try/catch` — now wired

`try/catch` was parsed but ignored before this release. Catch clauses now execute and the bound `err` variable carries error fields:

```keel
try {
  urgency = ai.classify(email.body, as: Urgency) ?? Urgency.medium
} catch err: AiSchemaError {
  io.notify("Unexpected LLM output: {err.got}")
  urgency = Urgency.medium
} catch err: Error {
  io.notify("Failed: {err.message}")
}
```

**`AiSchemaError` fields:**

| Field | Type | Value |
|---|---|---|
| `message` | `str` | Human-readable description |
| `got` | `str` | The raw LLM output that failed to match |

`Error` is the catch-all for any other runtime error.

---

## v0.1.14 — 2026-05-06

### `for` loop `if` guard

`for` loops now accept an inline `if` filter, replacing the previous `where` keyword in that position:

```keel
for email in emails if email.unread { triage(email) }
for n in 1..10 if n % 2 == 0 { io.show(n) }
```

**Breaking:** `for x in list where cond` no longer parses. Use `if` instead.

### `Time` namespace — full rework

`now` is no longer a keyword. The `Time` namespace now provides timezone-aware datetime handling with method syntax on values.

```keel
# Factories — namespace style
now  = time.now()                          # UTC, millisecond precision
ny   = time.now(tz: "America/New_York")    # IANA timezone
dt   = time.parse("2026-05-06T09:00:00Z") # datetime? (none on failure)
dt2  = time.parse("2026-05-06", tz: "UTC") # coerce naive string with tz:

# Methods on the datetime value
p    = dt.parts()           # {year, month, day, hour, minute, second, millisecond, tz}
s    = dt.format(as: "%Y-%m-%d")   # str?

# Operators
elapsed  = finish - start   # datetime - datetime → duration
deadline = time.now() + 3.days
ok       = deadline > time.now()

# Millisecond duration
short = 500.ms              # aliases: millis, millisecond, milliseconds
```

**Breaking changes from the earlier v0.1.14 Time stub:**
- `time.format(dt, as:)` removed → use `dt.format(as:)`
- `time.diff(a, b)` removed → use `a - b`
- `time.parse()` rejects naive strings without a TZ offset — returns `none` instead of raising. Use `time.parse(str, tz: name)` to coerce.

Naive strings (no UTC offset in the string) are rejected by design — all datetimes in Keel are timezone-aware.

---

## v0.1.13 — 2026-05-05

### Destructuring (§8.4)

All five destructuring forms from SPEC.md §8.4 are now implemented. No new keywords.

```keel
{urgency, category} = result             # struct shorthand
{urgency: u, category: c} = result       # struct rename
(label, count) = ("alpha", 42)           # tuple
for {from, subject} in emails { ... }    # in for loop
task handle({body, from}: Email) { ... } # in task params
```

Keyword-named fields (`from`, `state`, `in`, etc.) work in all positions. Missing struct fields and tuple arity mismatches are compile-time type errors.

See the [Variables & Expressions guide](guide/expressions.md#destructuring) for full documentation.

---

## v0.1.12 — 2026-05-04

### Range operator `..`

`start..end` produces an inclusive `list[int]`. Both bounds must be `int`.

```keel
for i in 1..5 {
  io.notify("{i}")    # 1, 2, 3, 4, 5
}

xs = 0..3             # [0, 1, 2, 3]
xs.count()            # 4
```

- `5..3` → `[]` (empty when start > end)
- `4..4` → `[4]` (single element)
- Non-integer bounds are a type error at compile time
- All list methods work on ranges: `.filter`, `.map`, `.count`, etc.
- SPEC grammar updated: `RangeExpr <- AddExpr (".." AddExpr)?`

See the [Collections guide](guide/collections.md) for full documentation.

---

## v0.1.11 — 2026-05-04

### Memory — safe cross-process storage (**breaking path change**)

> **Breaking:** The persistent memory directory format changed. Move existing data manually (see below).

Persistent memory is now both path-safe and cross-process safe:

**New path format:** `~/.keel/memory/<stem>_<hash12>/<agent>.json`

The `<hash12>` is derived from the SHA-256 of the canonical source file path, ensuring two programs with the same filename in different directories never share a storage bucket.

**Cross-process writes are now safe.** Each `memory.*` operation holds an advisory `flock` on a sidecar `<agent>.lock` file. Multiple concurrent `keel run` processes against the same agent serialize correctly.

**Crash durability.** Writes call `fsync` on the temp file before rename, and `fsync` on the parent directory after rename.

**Path validation.** Agent names containing `.`, `/`, `\`, or `\0` are rejected with a hard error (previously `debug_assert`).

**Migration:** data at the old path `~/.keel/memory/<stem>/<agent>.json` is not auto-migrated. Move it to the new location:

```bash
# find the new directory name for your program
keel run --print-memory-dir myprog.keel  # not yet available; use the hash formula
# or simply let Keel recreate it from scratch
```

See the [Agents guide](./guide/agents.md#memory--agent-memory-scope) for the updated path format and multi-process safety notes.

---

## v0.1.10 — 2026-05-03

### Memory namespace

`memory.remember`, `memory.recall`, and `memory.forget` are now real — they were no-op stubs since v0.1.0.

The `@memory` attribute selects the scope:

- **`session`** (default) — in-process, cleared on restart.
- **`persistent`** — file-backed JSON at `~/.keel/memory/<stem>_<hash12>/<agent>.json`, survives restarts. (Path format updated in v0.1.11; the directory includes a SHA-256 hash of the source file path to avoid cross-program collisions.)
- **`none`** — `memory.*` calls raise `CapabilityError`.

```keel
use std/io
use std/memory

agent Counter {
  @tools [io]
  @memory session

  @on_start {
    prev = memory.recall("count")
    count = if prev == none { 1 } else { prev + 1 }
    memory.remember("count", count)
    io.show("Visit {count}")
    stop(self)
  }
}
```

See the [Agents guide](./guide/agents.md#memory--agent-memory-scope) for full syntax and the `persistent` mode example.

---

## v0.1.9 — 2026-05-03

### `keel init` fixes & `stop(self)`

**`keel init` with no argument** now initializes in the current directory instead of creating a duplicate subdirectory.

**Path arguments** (`keel init /tmp/mybot`) now use the basename as the project name — previously the full path was injected into the agent name.

**Runnable scaffold** — the generated template no longer uses `schedule.every`. It prints and exits immediately via `stop(self)`, so `keel run main.keel` works out of the box.

**`stop(self)`** — bare `self` now resolves to an `AgentRef` for the current agent anywhere inside an agent body.

```keel
use std/io

agent Worker {
  @tools [io]
  @on_start {
    io.show("Hello from Worker!")
    stop(self)
  }
}
```

---

### Tooling — Linter & Sharper Errors

New `keel lint` command and source-span diagnostics in `keel check`.

#### `keel lint` — style and best-practice checks

```bash
keel lint <file.keel>
keel lint --fix <file.keel>
```

Four rules ship in v0.1.9:

| Rule | Trigger |
|------|---------|
| Unused variable | binding assigned but never read |
| Uncalled task | `task` declared but never invoked |
| `ai.*` outside agent | LLM call without `@role` / `@model` context |
| State written, never read | `self.x =` appears but `self.x` is never used |

`--fix` auto-removes unused variable assignments. Prefix a name with `_` to suppress the unused warning.

#### `keel check` — source spans in every diagnostic

Every error from `keel check` now includes a line:column pointer and an underlined source excerpt. Arity errors include the expected parameter names as a hint:

```
  × Type error
   ╭─[agent.keel:8:5]
 7 │   @on_start {
 8 │     greet(42)
   ·     ────┬────
   ·         ╰── task `greet` takes 1 argument(s), got 0 — expected: name
 9 │   }
   ╰────
```

---

## v0.1.8 — 2026-04-30
### Reactive Agents & Text Processing

HTTP webhook handling, in-memory caching, regex & string tools, LSP go-to-definition and rename.

#### `Cache` namespace

Process-scoped in-memory cache with optional TTL. Useful for deduplication, rate-limit tokens, and short-lived computed results across agents.

```keel
cache.set("key", value, ttl: 5.minutes)
v = cache.get("key")    # value or none
cache.delete("key")
cache.clear()
```

#### String value methods — regex & formatting

Regex matching and string manipulation are available as methods directly on string values. The `Str` namespace was removed in a later release; see the migration note in the changelog.

```keel
text.matches("\\d+")                    # bool
text.extract("(\\d+)")                  # str? — first capture group
text.find_all("\\d+")                   # list[str] — all matches
text.sub("\\d+", "N")                   # str — regex replace all
"hello world".truncate(5)               # "hello…"
"42".pad(6, char: "0")                  # "000042"
```

#### `http.serve` — webhook listener

React to inbound HTTP requests without polling:

```keel
http.serve(8080, (req) => {
  io.show("Got {req["method"]} {req["path"]}")
  { status: 200, body: "OK" }
})
```

The handler receives `{method, path, body}` and returns `{status, body}`. The event loop keeps running as long as at least one server is active.

#### LSP go-to-definition & rename

- **Go-to-definition** — jump to `task`, `agent`, and `type` declarations from any usage
- **Rename** — rename a user-defined symbol and all its usages in the open file (prelude names are blocked)

---

## v0.1.7 — 2026-04-30
### Structured Concurrency & Agent Constraints

File I/O, JSON processing, async task spawning, cron scheduling, and agent capability enforcement.

#### File namespace

Read, write, and list files on disk. The runtime creates intermediate directories automatically for writes.

```keel
file.write("data.txt", "Hello Keel")
content = file.read("data.txt")

if file.exists("data.txt") {
  io.show(content)
}

entries = file.list("data/")
```

Methods: `read(path)`, `write(path, content)`, `exists(path)`, `list(dir)`.

#### Json namespace

Serialize and deserialize JSON. `parse` deserializes a JSON string into Keel values (maps, lists, scalars). `stringify` turns Keel values back into JSON.

```keel
data = json.parse("{\"name\": \"Alice\", \"age\": 30}")
name = data["name"]

user = { name: "Bob", age: 25 }
json_str = json.stringify(user)
```

#### schedule.cron

Schedule tasks using 5-field cron expressions. Supports standard cron syntax for minute, hour, day, month, and weekday.

```keel
schedule.cron("0 9 * * 1-5", () => {
  io.show("Morning digest")
})

schedule.cron("*/5 * * * *", () => {
  io.show("Every 5 minutes")
})
```

#### Async namespace — structured concurrency

Spawn independent Tokio tasks and await completion. `spawn` returns a task handle; `join_all` awaits a list of handles; `select` races handles to the first completion.

```keel
task1 = async.spawn(() => {
  result = http.get("https://api1.example.com")
  io.show(result)
})

task2 = async.spawn(() => {
  result = http.get("https://api2.example.com")
  io.show(result)
})

results = async.join_all([task1, task2])
io.show("All done")
```

#### @tools capability gating

Restrict which prelude namespaces are accessible inside an agent. Calls to unlisted namespaces raise `CapabilityError` at runtime.

```keel
use std/file
use std/http
use std/io

agent RestrictedAgent {
  @tools [io, file]

  @on_start {
    io.show("Allowed")
    file.write("x.txt", "Allowed")
    # http.get would raise CapabilityError
  }
}
```

If no `@tools` attribute is specified, all namespaces are allowed.

#### @limits agent attributes

Extract and enforce resource limits (timeout, max_tokens, max_cost) on a per-agent basis. The infrastructure for extracting limits is in place; timeout wraps calls via `control.with_timeout`.

```keel
use std/ai

agent LimitedAgent {
  @tools [ai]
  @limits { timeout: 30s, max_tokens: 1000, max_cost: 5.0 }

  @on_start {
    response = ai.prompt("...")
  }
}
```

#### LSP completion

`textDocument/completion` now suggests prelude namespace names, method names, and keywords. Triggered on `.` or manual invocation.

#### New example programs

Five example programs demonstrate the new v0.1.7 features:

- `file_processing.keel` — File read, write, exists, list
- `json_processing.keel` — JSON parse and stringify
- `cron_schedule.keel` — Cron expression scheduling
- `parallel_execution.keel` — Async task spawning and joining
- `capability_gating.keel` — @tools capability restrictions

---

## v0.1.6 — 2026-04-28
### Wiring & ergonomics

Every primitive that was a stub in 0.1.5 now does what its name promises.

#### Nested string literals inside `{interp}`

The lexer used to terminate a string at the first `"` it saw, even one
hiding inside a `{...}` slot. The lexer now scans slot bodies with brace
depth tracking and recursively handles nested `"..."`:

```keel
name = "world"
io.show("hi {"there {name}"}")        # → "hi there world"
```

#### `control.retry` / `with_timeout` / `with_deadline`

The three control-flow combinators now have real implementations.

```keel
# Re-invoke until success or the budget is spent.
result = control.retry(5, () => ai.prompt(system: "...", user: "..."))

# Abort if the closure runs past the duration.
fast = control.with_timeout(2.seconds, () => slow_call())

# Abort once the absolute deadline has passed.
done = control.with_deadline("2026-12-31T23:59:00Z", () => long_task())
```

`with_timeout` and `with_deadline` raise `TimeoutError` / `DeadlineError`
on expiry. `retry` surfaces the last attempt's error if every attempt fails.

#### `broadcast(team, data)`

Tag agents with `@team [...]` and dispatch a single event to every
member of a named team:

```keel
agent Alpha { @team ["frontline"]  on alert(m: str) { ... } }
agent Beta  { @team ["frontline"]  on alert(m: str) { ... } }
agent Gamma { @team ["backoffice"] on alert(m: str) { ... } }

broadcast("frontline", "incident", event: "alert")
# Alpha and Beta fire; Gamma stays silent.
```

#### `email.archive(message)`

`email.archive` performs an IMAP UID MOVE (with COPY + `\Deleted` +
EXPUNGE fallback for servers without the MOVE extension). The
destination folder is `Archive` by default; override with the
`IMAP_ARCHIVE_FOLDER` env var. `email.fetch` now returns each message's
UID under the `uid` key so `archive` can target the right one.

#### `map[K, V]` method inference

Map literals support the common operations on both the type checker and
runtime side.

| Expression | Inferred type |
|---|---|
| `map.get(k)` | `V?` |
| `map.keys()` | `list[K]` |
| `map.values()` | `list[V]` |
| `map.len()` / `map.count()` / `map.size()` | `int` |
| `map.is_empty()` | `bool` |
| `map.contains(k)` / `map.has(k)` | `bool` |

The checker also accepts a `{k: v, ...}` struct literal in any position
that expects `map[str, V]`, so the same surface syntax serves both
struct and map construction.

#### LSP hover

`textDocument/hover` returns the inferred type of the identifier under
the cursor — `let`-bindings, function parameters, agent state fields,
and prelude namespaces (`Io`, `Ai`, `Control`, …) all light up.

---

## v0.1.5 — 2026-04-27
### Type checker hardening

Four new checks land in the type checker; zero breaking changes to valid programs.

#### Nullable safety

`T?` is now enforced at assignment and return sites. Passing a nullable value where a non-nullable is expected is a compile-time error. Use `!` to assert non-null (throws `NullError` at runtime if `none`) or `??` to provide a fallback.

```keel
use std/env

task t() {
  x: str = env.get("KEY")          # error: expected str, got str?
  y: str = env.get("KEY")!         # ok — throws if none
  z: str = env.get("KEY") ?? ""    # ok — falls back to ""
}
```

#### Return-type matching

`return expr` is now checked against the task's declared `-> T`.

```keel
task greet() -> str {
  return 42     # error: return value: expected str, got int
}
```

#### Struct field checks

Struct literals are now checked against named `type` declarations. Missing required fields are reported; extra fields are allowed.

```keel
type Person { name: str, age: int }

task t() {
  p: Person = { name: "Alice" }           # error: missing field `age`
  q: Person = { name: "Bob", age: 30 }    # ok
}
```

#### List and string method type inference

Method calls on `list[T]` and `str` now return typed results:

| Method | Return type |
|---|---|
| `list.push(x)` / `list.filter(fn)` | `list[T]` |
| `list.len()` | `int` |
| `list.first()` / `list.last()` | `T?` |
| `str.upper()` / `str.trim()` / `str.replace()` | `str` |
| `str.split(sep)` | `list[str]` |
| `str.len()` | `int` |

---

## v0.1.4 — 2026-04-27
### Parser hardening — every expression-level feature now works

#### `if`-as-expression

`if` can now appear on any right-hand side, not just as a standalone statement. `else if` chains are fully supported.

```keel
label = if score > 0.8 { "high" } else if score > 0.5 { "medium" } else { "low" }
```

#### `let` type annotations validated

Declaring a type on a binding now produces a compile-time error when the value type doesn't match.

```keel
x: int = "hello"   # error: expected int, got str
y: str = "hello"   # ok
```

#### `list + list` and `list.push(item)`

Lists now support concatenation with `+` and a functional `push` that returns a new list (no mutation).

```keel
all  = ["a", "b"] + ["c", "d"]   # ["a", "b", "c", "d"]
more = all.push("e")              # ["a", "b", "c", "d", "e"]
```

#### Full string interpolation

`{…}` slots now accept any expression: method calls, binary operations, chained calls.

```keel
summary = "Items: {cart.count()}, subtotal: {price * qty}"
```

#### `@on_stop` lifecycle hook

Agents can now declare a shutdown block that runs before the agent is removed from the runtime.

```keel
use std/io

agent Logger {
  @tools [io]
  @on_stop { io.show("Logger shutting down") }
}
```

#### `delegate(target, task, args)`

Posts a named task event to another agent's mailbox; the receiving agent handles it via its `on <task>` handler.

```keel
delegate(Processor, "handle", payload)
```

#### `Search`, `Db`, `Time` stub namespaces

Calling any method on these namespaces now raises a clear `"… is planned for v0.2"` error instead of a generic crash.

---

## v0.1.3 — 2026-04-26
### Declarations reach the model

#### `@rules` injected into every LLM system prompt

```keel
agent Support {
  @role "Customer support specialist"
  @rules [
    "Never reveal internal pricing logic",
    "Escalate if the user expresses frustration 3+ times"
  ]
}
```

Rules are prepended as a bullet list in the system prompt, between the role preamble and the operation instructions. Enable `KEEL_TRACE=1` to see the full prompt for every call.

#### `ai.summarize` — `format:` and `max:` wired

```keel
summary = ai.summarize(article, format: bullets, max: 5, unit: sentences)
```

#### `ai.prompt` — `response_format: json` wired

Appends "Respond with valid JSON only" to the system prompt and validates the reply.

```keel
score = ai.prompt(system: "Rate 1–10.", user: review, response_format: json)
```

#### `ai.extract(x, as: T)` — schema derived from struct type

```keel
type Invoice { vendor: str, amount: float, date: str }
result = ai.extract("Invoice from ACME $99.99 on 2026-01-10", as: Invoice)
```

---

## v0.1.2 — 2026-04-19
### Internal hardening

- Bumped to **Rust edition 2024** (requires rustc ≥ 1.85).
- Runtime config decoupled from environment: `--trace` / `--log-level` / `log.set_level` now go through typed setters, removing all `unsafe` env mutation.
- Release pipeline: Homebrew tap push now uses a short-lived GitHub App installation token instead of a long-lived PAT.

---

## v0.1.1 — 2026-04-19
### Release

- Dropped prebuilt macOS Intel binaries. Apple Silicon and Linux x86_64 are shipped; Intel Macs build from source (`cargo build --release`).

---

## v0.1.0 — First public alpha
### Everything is new

First release of the language, runtime, standard library, and tooling.

**Language highlights:**
- Statically typed, inference-first — 28 reserved keywords
- `agent` is the only concurrency primitive (actor model, serial mailbox, isolated `self` state)
- Prelude-as-stdlib — `Ai`, `Io`, `Email`, `Http`, `Schedule`, `Agent`, `Log`, … in scope everywhere, no `use` needed
- Algebraic types: simple enums, rich enums with per-variant fields, structural interfaces
- Exhaustive `when` pattern matching, duration literals (`5.minutes`), nullable syntax (`T?`, `??`, `?.`)

**Tooling:**
- `keel run` — execute, with `--trace` and `--log-level`
- `keel check` — static analysis (scope, arity, enum exhaustiveness)
- `keel fmt` — idempotent AST pretty-printer
- `keel repl` — interactive, history-aware
- `keel lsp` — diagnostics over stdio (VS Code extension included)

**Distribution:**
- GitHub releases for `aarch64-apple-darwin` + `x86_64-unknown-linux-gnu`
- `curl https://keel-lang.dev/install.sh | sh`
- `brew install keel-lang/tap/keel`
