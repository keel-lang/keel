# LLM Providers

Keel's `ai.*` operations dispatch through swappable backends. Three ship built in:

| Provider | Name | Key | Notes |
|---|---|---|---|
| Ollama | `ollama` | — | Local, the default |
| OpenAI | `openai` | `OPENAI_API_KEY` | Chat Completions API |
| Anthropic (Claude) | `anthropic` | `ANTHROPIC_API_KEY` | Messages API |

## Selecting a provider

Three ways, **most-specific wins**:

1. **Per-call** — a `provider:` prefix on the model tag, via `@model` or a
   `using:` argument:
   ```keel
   agent Assistant { @model "anthropic:claude-opus-4-8" }   # → Claude
   task quick(q: str) -> str {
     ai.prompt(system: "Be brief.", user: q, using: "openai:gpt-4o") ?? "—"
   }
   ```
2. **Per-agent** — `@provider <name>` sets the backend for that agent's bare
   (unprefixed) model tags:
   ```keel
   agent Helper {
     @provider openai
     @model "gpt-4o"
   }
   ```
   `@provider` accepts only `ollama`, `openai`, or `anthropic`; anything else is a
   compile-time error.
3. **Per-program** — `KEEL_PROVIDER` sets the default backend:
   ```bash
   export KEEL_PROVIDER=anthropic   # default: ollama
   ```
   It accepts only `ollama`, `openai`, or `anthropic`; an unrecognised value
   throws `AiError { reason: "provider" }` on the first `ai.*` call rather than
   silently falling back to Ollama.

A bare model tag with no prefix and no `@provider` routes to the program default.

## OpenAI and Anthropic

```bash
export OPENAI_API_KEY=sk-...
export ANTHROPIC_API_KEY=sk-ant-...
```

A missing key throws `AiError { reason: "provider" }` — never a silent fallback.
Unlike Ollama, these backends have no built-in default model: set `@model` (or a
`using:`/prefixed tag). An agent with neither also throws `AiError { reason: "provider" }`.
Endpoints can be overridden (for proxies, OpenAI-compatible servers, or testing):

```bash
export OPENAI_BASE_URL=https://api.openai.com        # default
export ANTHROPIC_BASE_URL=https://api.anthropic.com  # default
```

`@limits { max_tokens: N }` caps generation; without it a sensible default is used.

## Ollama (default)

```bash
# Install Ollama from https://ollama.com, then pull a model:
ollama pull gemma4
export KEEL_OLLAMA_MODEL=gemma4
```

### Custom host

```bash
export OLLAMA_HOST=http://192.168.1.10:11434   # default: http://localhost:11434
```

### Named model aliases

`@model "fast"` or `using: "smart"` map to Ollama tags via environment variables:

```bash
export KEEL_MODEL_FAST=gemma4
export KEEL_MODEL_SMART=mistral:7b-instruct
```

Ollama model resolution order for model `X` (after routing has chosen Ollama):

1. `ollama:X` prefix — strip and use `X` directly as the Ollama tag
2. `KEEL_MODEL_<X>` environment variable (`X` uppercased, `-` → `_`)
3. `KEEL_OLLAMA_MODEL` (catch-all)
4. Configuration error — the call fails with instructions for fixing it

## Testing without a real LLM

```bash
export KEEL_LLM=mock
```

In mock mode every `ai.*` call returns `none` regardless of provider —
deterministic *absence*, never a failure — so `??` defaults fire predictably.
(Real provider failures throw `AiError`; mock mode does not simulate them.)

## User-authored providers

For proprietary or self-hosted models the built-in backends don't cover, write a
provider **in Keel**. Any field-less type with `impl LlmProvider` becomes a
backend `ai.*` can dispatch through:

```keel
use std/ai
use std/env
use std/http

type MyProvider {}

impl LlmProvider for MyProvider {
  task complete(self, req: CompletionRequest) -> str {
    # The provider is constructed with no fields, so read configuration from
    # the environment — not from struct fields.
    key = env.get("MY_LLM_KEY")!
    http.post(
      "https://my-llm.example/v1/complete",
      headers: { "Authorization": "Bearer {key}" },
      body: { system: req.system, prompt: req.user, model: req.model, max_tokens: req.max_tokens }
    )["text"]
  }
}

ai.install(MyProvider)   # program-wide default
```

`complete(self, req: CompletionRequest) -> str` returns the **raw** model text;
Keel applies its own prompt construction and output parsing (enum matching for
`ai.classify`, schema validation for `ai.extract`) on top, so `??`, `when`, and
the typed `AiError` / `AiSchemaError` behave identically to the built-in
backends. The `CompletionRequest` struct carries `system`, `user`, `model`, and
`max_tokens`.

Select a user provider the same two ways as a built-in:

- `ai.install(MyProvider)` — program-wide default (lowest precedence, below a
  `provider:` model-tag prefix and `@provider`).
- `@provider MyProvider` — per agent.

`ai.install(X)` and `@provider X` require `X` to implement `LlmProvider`;
anything else is a compile-time error. A provider must not call `ai.*` from
inside its own `complete()` — that re-entry raises an `AiError` rather than
recursing without bound.

A provider is **trusted transport**, like a built-in backend: its `complete()`
may call effectful modules (`env`, `http`, …) regardless of the consuming
agent's `@tools`. An agent only needs `@tools [ai]` to use a provider that talks
HTTP under the hood — just as it does for the built-in OpenAI and Anthropic
backends.

## Troubleshooting

**`ANTHROPIC_API_KEY is not set` / `OPENAI_API_KEY is not set`** — export the key for
the backend you selected.

**`Ollama unreachable at http://localhost:11434`** — the daemon isn't running. Start it: `ollama serve &`.

**`Model 'X' has no mapping`** — you called `@model "X"` on Ollama but there's no matching `KEEL_MODEL_X` variable and no `KEEL_OLLAMA_MODEL`. Set one of them.

**`Ollama returned 404`** — the tag isn't pulled locally. Fix it: `ollama pull <tag>`.
