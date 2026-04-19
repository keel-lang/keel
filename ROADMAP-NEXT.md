# Keel Release Agenda — v0.1.3 → v0.1.5, plus v0.2 horizon

## Context

v0.1.2 just shipped (Rust 2024 migration + release auth hardening).
Both recent releases were infrastructure, not language features. The
backlog is broad — type checker gaps, parser corners, most attributes
parsed-but-ignored, Ai.* primitives accepting-but-dropping parameters,
stubbed namespaces. Splitting the backlog by random theme would leave
Keel looking like "Rust with an agent decorator." The creator's
identity constraint is explicit:

> Agents are first-class citizens. I don't want something I could do
> with a Rust crate or a Python package. The promise is to write less
> code than Python.

This plan sequences the next three patch releases (v0.1.3 → v0.1.5) so
each one makes Keel's *unique shape* visible, and frames v0.2 as the
first milestone with subsystem-scale design work and initial API
stability commitments. It consciously defers features that would shape
Keel toward existing languages.

(A remote Ultraplan pass ran against this plan; it timed out before
emitting a final document but confirmed the critical file:line
references and caught two stale assumptions, now folded in below.)

## Design lens — Hejlsberg-style, applied to Keel

Four principles drive every pick. None are feature-parity goals.

1. **Declarations do work.** A single-line attribute should produce
   behaviour that Python needs 10+ lines of scaffolding for. If a
   declaration doesn't change what the runtime does, it's noise.
2. **The LLM is not a library — it's the expression.** `Ai.*`
   primitives aren't thin wrappers around a chat API; they're language
   operators with types (classify returns your enum, extract returns
   your struct). Python/Rust cannot express this at the language
   level.
3. **Runtime owns identity.** What makes `@on_stop`, `Agent.delegate`,
   `@team` earn their keep is that the *runtime* owns the mailbox, the
   `self.` state, the event loop, the LLM session. A library can't
   reach in without reimplementing the agent model.
4. **No feature enters the language that fits in a library.** `!`-
   unwrap, typed `let`, `list + list`, `if`-as-expression — shapes
   imported from Rust/TS/Scala. Backlog only, until a concrete pain
   point promotes one *and* no library fix exists.

Each release has a single narrative, independently bisect-able.

---

## `@on_stop` — why it earns its place in the language

The creator asked specifically for scenarios. Four that a library
`atexit`/`Drop` handler can't cleanly cover — each depends on state
the runtime owns:

1. **State checkpoint on shutdown.** `self.` is declared at the agent
   level; the runtime already has its schema. `@on_stop { State.save() }`
   (or an implicit auto-save) pairs with a future
   `run(Agent, from: path)` resume. Python/Rust both need manual
   `Serialize`/pickle and explicit file management.
2. **LLM session summary-on-exit.** Before stopping, the agent calls
   `Ai.summarize(self.history)` and logs it as an audit trail. One
   line, because the runtime already owns the provider + history.
3. **Graceful mailbox drain.** Today's `stop_agent` at
   `src/interpreter/mod.rs:1063–1066` just drops the agent from
   `live_agents` — any pending events in its mailbox are lost. A
   clean pipeline (*stop accepting new events → drain → `@on_stop` →
   exit*) is only expressible if the language owns lifecycle.
4. **Coordinated multi-agent shutdown.** Once `@team` lands (v0.1.5),
   a parent's `@on_stop` can wait for children's shutdowns. The
   runtime has the agent graph; a library can't.

Scenarios that *don't* justify a language feature — logging a
goodbye, closing an HTTP client, unsubscribing a webhook — are one
line of imperative code but wouldn't justify inventing the attribute
on their own. (1)–(4) are why the attribute belongs in the language.

**v0.1.4 scope — only (3) the mailbox drain + `@on_stop` execution.**
(1) checkpoint/resume is a serialisation-format design; that lands in
v0.1.5. (2) and (4) fall out naturally once (3) exists.

---

## v0.1.3 — "Declarations reach the model"

**Narrative:** What the user declares — about an agent or about a
call — actually reaches the LLM. Today we parse `format:`,
`response_format:`, `@rules` and silently drop them. That's the worst
kind of silent fallback: the user believes the declaration worked.

**Correction from code-path verification:** `considering:` is
*already* forwarded via `extract_criteria()`
(`src/runtime/mod.rs:491–505` → `llm.rs:237–242`). ROADMAP's "accepted
but not sent" claim is stale. Drop it from scope; update ROADMAP in
the same release.

**Scope:**

- **`@rules [...]` injected into LLM system prompts** alongside
  `@role`. Extension point: `src/runtime/llm.rs:166–170` (where `@role`
  is prepended). Add a `current_rules()` sibling to `current_role()`
  (`src/interpreter/mod.rs:230–251`) — same shape, but reading an
  `AttributeBody::Expr(Expr::ListLit(...))` into `Vec<String>`.
  Rules prepend as bullets beneath the role string.
- **`Ai.summarize(x, format: bullets|prose, unit: sentences|words, max: N)`**
  emits matching output directives into the system prompt
  (`"Reply as a bulleted list of at most N sentences"`). Builder site
  in `llm.rs`.
- **`Ai.prompt(x, response_format: json)`** injects `"Respond with
  valid JSON only, no prose"` and the runtime parses the reply as
  JSON (error if it isn't). First case where a named arg changes
  return-type *shape* — a template for v0.1.4's `Decision[T]`.
- **`Ai.extract(x, as: T)` derives a JSON schema from `T`** and
  injects it as a system prompt. Today's `extract` takes a `schema:`
  map (`runtime/mod.rs:364–390`); the `as: T` form is new. Requires a
  **struct-type registry** in the interpreter (today only `enum_types`
  exists) — build it minimally here, reuse in v0.1.5 for `@team`
  type-checking.
- **Drop the "accepted but not sent" lines from ROADMAP** — they're
  inaccurate after this release.

**Identity this sharpens:** a 1-line `@rules` declaration becomes a
runtime-enforced prompt prefix on *every* LLM call the agent makes —
Python needs an `if/else` ladder in a system-prompt builder function
to do the same. `Ai.extract(x, as: Invoice)` *is* "give me a
structured Invoice"; no schema string, no JSON-mode toggle, no parser
written by the user.

**Explicitly out of scope:** `@limits` (creator dropped), `@tools`
(v0.2 — needs capability-stack design), `Decision[T]` return for
`Ai.decide` (moved to v0.1.4 where it fits lifecycle types).

**Done when:** every documented parameter on an `Ai.*` primitive has
observable effect or raises at parse time. The phrase "parsed but
ignored" no longer appears next to any `Ai.*` entry in `ROADMAP.md`.

**Critical files:** `src/runtime/llm.rs` (prompt builders),
`src/runtime/mod.rs:303–459` (`ai_namespace()` — extract named args
via `find_arg`), `src/interpreter/mod.rs:230–251` (`current_role()` —
add `current_rules()`), `src/interpreter/` (new struct-type registry).

---

## v0.1.4 — "Agents have lifecycles"

**Narrative:** Agents don't just exist — they start, run, wind down,
and end. The runtime owns the lifecycle; the language reflects it
declaratively.

**Scope:**

- **`@on_stop { ... }` executes** on `Agent.stop` and on Ctrl-C /
  `KEEL_ONESHOT` idle exit. Extension: `src/interpreter/mod.rs:1063–
  1066` mirrors the `@on_start` pattern at `src/interpreter/mod.rs:
  1037`/1050–1059. `self.` remains readable inside the block.
- **Mailbox drain on stop.** Between "stop requested" and `@on_stop`
  executing, the runtime refuses *new* events but finishes pending
  ones. Event loop extension: `src/interpreter/mod.rs:321–352`,
  specifically the Ctrl-C branch at line 334. Keep the existing
  hard-exit fallback on a second Ctrl-C (no change).
- **`Ai.decide` returns `Decision[T]`** — a first-class generic type:
  `{ choice: T, reason: str, confidence: float }`. Today `Ai.decide`
  (`runtime/mod.rs:418–440`) returns a plain `Value::Map`. Users
  destructure via pattern matching: `when result { Decision { choice:
  go, reason, .. } => ... }`. First generic ADT in the language;
  template for future `Result[T, E]`.
- **Structured trace events.** `--trace` currently emits free-form
  lines from `runtime::trace_enabled()` readers. Emit a typed event
  stream — per-LLM-call start/end, per-handler start/end, per-event-
  loop tick — as JSON-per-line so a downstream tool can parse without
  heuristics. No UI yet; just the stream.

**Identity this sharpens:** Keel is the only language where an
agent's lifecycle *is* the program structure. `@on_start` / `@on_stop`
/ `self.` / mailbox drain are the grammar, not library patterns.
`Decision[T]` makes an LLM's answer a pattern-matchable ADT — Python
gets a `dict`, Keel gets a type.

**Explicitly out of scope:** state checkpoint + resume (design work —
v0.1.5), `@on_error` (interacts with `Control.retry` which is v0.2).

**Done when:** a program with `@on_stop` prints a final summary
*after* the event loop finishes, with no dropped mailbox events.
`Ai.decide` pattern-matches into `Decision[T]` in the type checker.
`keel run --trace` emits JSON-per-line events.

**Critical files:** `src/interpreter/mod.rs:321–352` (event loop),
`src/interpreter/mod.rs:1063–1066` (`stop_agent`), `src/ast.rs:200`
(`BLOCK_BODY_ATTRIBUTES` — `@on_stop` already listed), `src/types/`
(new generic `Decision[T]`), `src/runtime/llm.rs` (structured trace).

---

## v0.1.5 — "Agents are a team"

**Narrative:** A single agent is interesting; coordinated agents is
where Keel's runtime-owned graph beats a pile of Python functions.
`@team` stops being parsed-and-ignored and becomes a declaration the
type checker and runtime both understand.

**Scope:**

- **`@team [Researcher, Writer]` on an orchestrator agent** — the
  type checker verifies each named agent exists at check time (today:
  silent success on unknown names). Reuses the struct-type registry
  added in v0.1.3.
- **`Agent.delegate(target, task, args)`** — posts a request/response
  event to `target`'s mailbox and awaits the reply. `run_agent_task`
  already exists at `src/interpreter/mod.rs:1068`; the simplest
  `delegate` path calls it directly via a new `Response` variant on
  the event enum (today's `Agent.send` uses `Dispatch` only).
- **`Agent.broadcast(team, data)`** — posts the same event to every
  member of the named `@team`. Fire-and-forget; no reply collect.
- **`self.peers` / `self.team`** — read-only view on the team list
  from inside a handler, so a handler can address peers by logical
  role (`self.peers.writer`) not hardcoded agent names.
- **Checkpoint/resume MVP** — `Agent.save(X, path)` serialises `self.`
  as JSON; `run(X, from: path)` restores it before `@on_start` fires.
  JSON-only for v0.1.5; schema evolution is a v0.2 concern.

**Identity this sharpens:** a three-agent research → write → review
pipeline becomes `@team [Researcher, Writer, Reviewer]` + three
`Agent.delegate` calls. Python LangGraph/CrewAI equivalent is ~150
lines of boilerplate. The 10-to-1 ratio the creator wants *is*
achievable for this shape, because the language owns the graph, the
mailbox, and the LLM session.

**Explicitly out of scope:** `@tools` capability gating (v0.2),
vector-backed `Memory` (v0.2 — dependency-heavy subsystem), provider
pluggability (v0.2), schema migration on resume (v0.2).

**Done when:** a `multi_agent_inbox.keel`-style example runs end-to-
end using `@team` + `delegate` + `broadcast` with zero workarounds,
and a program with `self.history` resumes across a process restart.

**Critical files:** `src/runtime/mod.rs` (`agent_namespace` —
register `delegate`, `broadcast`), `src/interpreter/mod.rs:1068`
(`run_agent_task` reuse) + event-loop `Response` variant, `src/
types/` (team membership check), `src/ast.rs` (@team body is
`AttributeBody::Expr(Expr::ListLit([Ident("X"), ...]))` — already
parsed).

---

## v0.2 — "Beyond alpha" (horizon)

v0.2 is the first milestone with API-stability commitments — semver
starts mattering. It gathers subsystems that each need their own
design pass rather than fitting in a patch release. The existing
ROADMAP says v0.2+ is "deliberately un-planned"; keeping that
discipline while sketching focus areas for long-run orientation:

Focus areas (ordered by likely dependency, not strict release slots):

1. **Pluggable LLM providers.** `Ai.install(MyProvider)` and
   `@provider MyProvider` per-agent. Forces the interface-registry
   design. Gating question for any cost-sensitive or privacy-
   sensitive user; likely first.
2. **Capabilities & safety.** `@tools [Email.send, Http.get]`
   capability gating (calls to non-listed namespaces raise),
   `@limits { timeout, max_tokens, max_cost }` enforced. Needs a
   capability-stack interpreter change. This is the "production
   ready" story: wrong-tool-access protection and cost caps.
3. **Memory subsystem real.** `Memory.remember` / `recall` / `forget`
   backed by a pluggable store (kv first, vector second). Interacts
   with `@memory persistent|session|none` semantics. Dependency-
   heavy: separate design from everything above.
4. **Control subsystem real.** `Control.retry(n, backoff:
   exponential)`, `Control.with_timeout(duration, fn)`,
   `Control.with_deadline(time, fn)`. Plus `@on_error { ... }` (needs
   retry to exist first). Every production agent needs these.
5. **IDE ergonomics.** LSP hover (types on LLM call return values),
   completion, go-to-definition, rename. Hejlsberg lens: tooling is
   part of the language design, and this is where the shape-of-types
   discipline pays off.
6. **Type system stability.** Full nullable-safety enforcement,
   return-type matching, struct/map subtyping, generic inference on
   collections. These are API-stability foundations.
7. **State & context extensions.** Schema migration on resume, richer
   `Agent.save` formats, time-travel debugging via structured trace
   events from v0.1.4.
8. **Ecosystem namespaces.** `Search`, `Db`, `Time` — documented but
   not registered. Each is its own subsystem; prioritise by which
   unblocks the most examples.
9. **`keel build` + VM.** Only if a concrete motivator lands
   (WASM/embedded target, JVM/Python FFI, no-Rust distribution).
   Otherwise defer — the tree-walking interpreter is fast enough for
   alpha workloads. Likely punts to v0.3.

**Sequencing principle:** pair every capability with a motivating
example. If no `.keel` program in `examples/` would be meaningfully
shorter with a focus area, defer it.

**What v0.2 explicitly is NOT:** a feature-parity push against
LangChain/CrewAI/OpenAI Agents SDK. Each focus area only enters
scope when it's *more concise in Keel* than in a Python library.

---

## What stays in the backlog (and why)

Listed in ROADMAP but deliberately *without* a release theme — they
fail the "would I do this with a Python package?" test, or their
design isn't yet forced by user pain:

- **Parser corners** (`!` unwrap, typed `let`, `if`-as-expression
  RHS, `list + list`). Backlog only. Fix opportunistically when a
  v0.1.3–v0.1.5 commit needs one. The only Keel-shaped item is
  function-calls-in-`{...}`-interpolation; fold into v0.1.3 if
  `@rules` work triggers it.
- **`@limits`** — creator dropped. Re-enters v0.2 capabilities theme.
- **Full nullable-safety enforcement** — background hygiene, not
  release-themeable until v0.2.
- **Major dep bumps** (chumsky 1.0, imap 3) — already tracked in
  ROADMAP Dependencies section; independent of these releases.

---

## Verification (per release)

Not traditional test plans — the question is "did we sharpen
identity?"

- **v0.1.3.** Pick three examples (`daily_digest`, `code_reviewer`,
  `customer_support`). Add `@rules` blocks and observe the LLM's
  behaviour shift. Convert any `Ai.extract(x, schema: {...})` site
  to `Ai.extract(x, as: T)` and verify the derived schema lands in
  the outgoing prompt (`KEEL_TRACE=1`). Every ROADMAP/docs claim
  saying "parsed but ignored" should now be wrong.
- **v0.1.4.** Write an example where `@on_stop` is the whole point
  (an agent collecting events for a shift, summarising on shutdown).
  Confirm no mailbox drops under forced stop. `Ai.decide` round-trips
  through `when` without `Map.get` escape hatches.
- **v0.1.5.** Port a toy multi-agent pipeline — bar is a CrewAI
  hello-world in under 30 lines of Keel. Resume: start an agent,
  `Io.notify` some state, `Agent.save`, restart, confirm state.

## Follow-ups (separate commits)

- Replace the "v0.1 — Alpha" attribute table rows with per-release
  scope as each ships.
- Drop `Coming soon` badges from `docs/src/guide/*.md` attribute
  pages as each feature lands.
- `CHANGELOG.md` per release — keep the "what changed" in user voice
  (not infrastructure voice that v0.1.2 used).
