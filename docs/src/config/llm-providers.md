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

A bare model tag with no prefix and no `@provider` routes to the program default.

## OpenAI and Anthropic

```bash
export OPENAI_API_KEY=sk-...
export ANTHROPIC_API_KEY=sk-ant-...
```

A missing key throws `AiError { reason: "provider" }` — never a silent fallback.
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

## User-authored providers <span class="badge badge-soon">Coming soon</span>

> Status: planned. Writing a provider in Keel (`impl LlmProvider for MyProvider`)
> and installing it with `ai.install(MyProvider)` is planned for a future release,
> for proprietary or self-hosted backends. The built-in providers above cover the
> common cases today. See [SPEC §5.5](https://github.com/keel-lang/keel/blob/main/SPEC.md).

## Troubleshooting

**`ANTHROPIC_API_KEY is not set` / `OPENAI_API_KEY is not set`** — export the key for
the backend you selected.

**`Ollama unreachable at http://localhost:11434`** — the daemon isn't running. Start it: `ollama serve &`.

**`Model 'X' has no mapping`** — you called `@model "X"` on Ollama but there's no matching `KEEL_MODEL_X` variable and no `KEEL_OLLAMA_MODEL`. Set one of them.

**`Ollama returned 404`** — the tag isn't pulled locally. Fix it: `ollama pull <tag>`.
