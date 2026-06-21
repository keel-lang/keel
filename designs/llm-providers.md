# Design: Swappable LLM providers (#44 + #46)

Status: **Phase 1 + Phase 2 shipped.** Built-in Ollama/OpenAI/Anthropic backends,
model-tag prefix routing, `@provider`, `KEEL_PROVIDER`, and `@limits { max_tokens }`
landed in Phase 1 (#46/#87). **Phase 2** (user-authored Keel providers — a built-in
`interface LlmProvider`, the `CompletionRequest` struct, `ai.install(MyProvider)`,
`@provider MyProvider`, re-entrant dispatch via the namespace-layer `Transport`
seam, and a recursion guard) landed in #44. The re-entry risk flagged below was
resolved by dispatching the user's `complete()` inline from the `ai.*` namespace
closure (which holds `&mut dyn Host`), not from inside the `'static` `LlmProvider`
future — see the `Transport` enum in `runtime/llm.rs`.

Validated against the `design-lang` (Hejlsberg) principles: developer productivity
first, make the common case one line, gradual adoption, IDE-friendly, escape hatches
for the long tail.

## Problem

`ai.*` is hardcoded to a single `Provider` enum (`Ollama` | `Mock`) inside
`LlmClient` (`crates/keel-runtime/src/runtime/llm.rs`). `@provider` parses but is a
silent no-op. There is no way to use OpenAI, Anthropic/Claude, or any other backend,
and no contract for *how* a provider is selected.

## Guiding insight

Most users will reach for **OpenAI or Claude**, not Ollama and not a hand-written
backend. Forcing them to author a provider in Keel for the mainstream case violates
"don't make simple things verbose." So the design is **two tiers**:

- **Tier 1 — built-in Rust backends** (covers ~90% of users): `ollama`, `openai`,
  `anthropic`, selected declaratively with zero Keel code.
- **Tier 2 — user-authored Keel providers** (the escape hatch): `impl LlmProvider for
  MyProvider` + `ai.install(...)`, for proprietary / self-hosted / novel backends.

## The seam

Every high-level primitive (`classify`, `summarize`, `draft`, `extract`, `translate`,
`decide`, `prompt`) funnels through one private method in `LlmClient`:

```
call(role, rules, system, user, model) -> Result<Option<String>, LlmError>
    └─ dispatches on self.provider → call_ollama(...)
```

`call_ollama` is the only provider-specific code. Prompt construction (role/rules/
system) and output parsing (enum match, schema validation) are provider-agnostic and
**stay in `LlmClient`** — they are Keel's value-add and must behave identically across
backends. We extract the transport, nothing else.

### Rust trait (mirrors `db_provider.rs::DbConnectionHandle`)

```rust
pub trait LlmProvider: Debug + Send + Sync {
    /// Returns the raw model output, or `None` for deterministic *absence*
    /// (mock mode / "model had no answer") — never a silent failure.
    /// Transport/config faults return Err(LlmError).
    fn complete(&self, req: CompletionRequest) -> LlmFuture<Option<String>>;
}

pub struct CompletionRequest {
    pub system: String,    // fully-built system prompt (role + rules + task system)
    pub user: String,      // the user content
    pub model: String,     // resolved model tag, provider prefix already stripped
    pub max_tokens: u32,   // REQUIRED by Anthropic & OpenAI — no provider default
}
```

`system` and `user` stay **separate fields** (not pre-merged) precisely because
providers place the system prompt differently — Anthropic takes a top-level `system`
field, OpenAI takes a `system` message role. Each backend assembles its own wire shape.

`max_tokens` is mandatory: Anthropic's `/v1/messages` rejects a request without it.
Source it from the agent's `@limits { max_tokens }` attribute (today parsed but **not
enforced** — see ROADMAP `@limits [~]`), falling back to a sane default (e.g. 4096) when
unset. Threading it here also begins enforcing that limit, closing part of the `@limits`
gap.

Built-ins (`OllamaProvider`, `OpenAiProvider`, `AnthropicProvider`, `MockProvider`)
implement this in Rust. A Tier-2 Keel provider is wrapped in a `KeelProvider` adapter
that implements the same trait by calling back into the interpreter — so `ai.*` always
talks to `dyn LlmProvider` and never knows the difference.

### Built-in backend wire facts (from the `claude-api` reference + OpenAI docs)

| | Anthropic | OpenAI | Ollama (have it) |
|---|---|---|---|
| Endpoint | `POST /v1/messages` | `POST /v1/chat/completions` | `POST {host}/api/chat` |
| Auth | `x-api-key` + `anthropic-version: 2023-06-01` | `Authorization: Bearer` | none (local) |
| Key env | `ANTHROPIC_API_KEY` | `OPENAI_API_KEY` | — |
| System prompt | top-level `system` field | `{role:"system"}` message | `{role:"system"}` message |
| Required | `model`, `max_tokens`, `messages` | `model`, `messages` (+ `max_tokens`) | `model`, `messages` |
| Response text | `content: [{type:"text", text}]` (extract text blocks) | `choices[0].message.content` | `message.content` |
| Refusal | HTTP 200 `stop_reason:"refusal"` → map to `AiError` | n/a | n/a |

Current model IDs (default to Opus 4.8): `claude-opus-4-8`, `claude-sonnet-4-6`,
`claude-haiku-4-5`, `claude-fable-5`; OpenAI per its own docs. **Pull exact IDs and any
request-shape changes from the `claude-api` reference / OpenAI docs at implementation
time — do not hardcode from memory.**

## Provider selection (the #46 dispatch contract)

Resolution, most-specific wins:

```
per-call using:  >  per-agent @provider  >  program ai.install()  >  built-in default
```

- **Model-tag prefix** infers the provider for the common case:
  `@model "anthropic:claude-..."`, `"openai:gpt-4o"`, `"ollama:llama3"`.
- **`@provider <name>`** sets an agent's default backend for bare model tags and is the
  explicit override.
- **`using:`** already exists in `resolve_model` (ai.rs) for per-call model override;
  the same hook extends to provider override.
- Bare tag with no prefix and no `@provider` → the program default (Ollama today,
  or whatever `ai.install` set).

API keys come from env: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`. Missing key or unmapped
model → `LlmError::ConfigError` → `AiError { reason: "provider" }`. No silent fallback.

## Type checking

Keel does **not** yet support interfaces as parameter types (`task f(x: SomeIface)`),
and we will **not** build that here. Instead:

**`@provider X` name resolution (decided).** The three built-in backend names —
`ollama`, `openai`, `anthropic` — are a known reserved set. The checker resolves
`@provider X` as follows:
- `X` ∈ {ollama, openai, anthropic} → a built-in backend (Tier 1).
- else, in **Phase 2**, `X` must be a type with `impl LlmProvider for X` (reuse
  `types/interface.rs::signature_satisfies`, shipped in #45/#86).
- else (Phase 1, or a name that is neither a built-in nor a conforming type) →
  compile error: "unknown provider `X`".

This avoids ambiguity: a built-in name is never also a user type, and the checker
never has to guess. `ai.install(x)` (Tier 2) is special-cased the same way — `x`'s
type must conform to `LlmProvider`. **General interface-as-type** (`task f(x: SomeIface)`)
stays a separate v0.2 item; we special-case only these two builtin sites.

## Error mapping

| Source | LlmError | AiError reason |
|---|---|---|
| Network / endpoint unreachable | `CallFailed` | `unavailable` |
| Missing key / unmapped model / misconfig | `ConfigError` | `provider` |
| Output didn't match enum/schema | `SchemaValidation` | (handled by parser) |
| Tier-2 provider `raise`s | `CallFailed`, message preserved | `unavailable` |

Absence (`Ok(None)`) is unchanged: stdlib parsing decides `none`, so `??`/`when` still
fire for "no answer" while real failures throw.

## Phase → issue mapping

- **Phase 1** delivers the #46 dispatch contract in full, plus the Rust-trait +
  built-in-backends slice of #44. It **partially** closes #44 (the `Close #44` line
  waits for Phase 2). It can `Close #46`.
- **Phase 2** delivers the user-authored-dispatch slice of #44 ("interface declarations
  parse but don't dispatch" as applied to providers) → `Close #44`.

> **ROADMAP principle change (deliberate).** Adding OpenAI + Anthropic reverses the
> stated "v0.1 ships Ollama only" line and pulls provider support forward from the v0.2
> "pluggable provider registry". The user explicitly chose this. The ROADMAP and SPEC
> §5.5 must be edited to reflect it — not left contradicting the code.

## Phasing

### Phase 1 — built-in backends (high value, low risk; no interpreter re-entry)
1. Extract `LlmProvider` Rust trait + `CompletionRequest`; refactor Ollama and Mock
   behind it. `LlmClient` holds a name→provider registry (per-call resolution — needed
   for per-agent `@provider`), not a single fixed `Box<dyn LlmProvider>`.
2. Add `OpenAiProvider` and `AnthropicProvider` HTTP backends. **Model IDs, request
   shape, and auth headers pulled from the `claude-api` reference (Anthropic) and
   OpenAI docs at implementation time.** API keys from env; missing key → `ConfigError`
   → `AiError{reason:"provider"}`.
3. Model-tag prefix routing (`provider:model`) + per-program default selection.
4. `Host::current_provider()` (read the agent's `@provider` attribute, analogous to the
   existing `current_model()`); `call()` selects the backend per-call from the model
   prefix / current provider, replacing the once-at-construction `self.provider`.
5. Thread `@limits { max_tokens }` → `CompletionRequest.max_tokens` (default when unset).
6. SPEC §5.5, ROADMAP (principle change above), CHANGELOG, docs/src guide pages,
   `.keel` examples, tests.

### Phase 2 — user-authored Keel providers (the escape hatch)
1. `KeelProvider` adapter: implement `LlmProvider::complete` by invoking the user's
   `impl LlmProvider for MyProvider` method through the `Host` trait (#11).
   **Open risk to confirm before building: the `Host` trait must be able to invoke a
   user method re-entrantly from inside a namespace call (async).**
2. `ai.install(MyProvider)` (per-program) + `@provider MyProvider` (per-agent).
3. Type-checks per above. SPEC/docs/examples/tests.

## Resolved open questions

- **`CompletionRequest` shape** — carries `max_tokens` (mandatory). `system`/`user`
  stay separate. A `format`/JSON-mode hint stays folded into `system` for Phase 1; add
  a structured field later only if a backend's native JSON mode proves worth it.
- **Registry shape** — name-keyed map on `LlmClient`, resolved per call (per-agent
  `@provider` requires it).
- **Per-call `using:` provider override** — `using:` already exists for model override;
  extending it to provider override is a small Phase-1 add, included.
- **`@provider` naming** — resolved (reserved built-in names; else conforming type in
  Phase 2; else error). See Type checking above.
