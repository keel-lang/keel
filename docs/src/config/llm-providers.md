# LLM Providers

> **Alpha (v0.1).** Ollama is the only supported backend. Additional providers implementing the `LlmProvider` interface ([prelude.md](../guide/prelude.md)) will land in a future release.

Keel's `Ai.*` operations call a local Ollama instance.

## Required

```bash
# Install Ollama from https://ollama.com, then pull a model:
ollama pull gemma4

# Point Keel at it:
export KEEL_OLLAMA_MODEL=gemma4
```

That's the minimum. Every `Ai.classify(...)`, `Ai.draft(...)`, etc. now resolves through this model.

## Custom host

```bash
export OLLAMA_HOST=http://192.168.1.10:11434   # default: http://localhost:11434
```

## Named model aliases

If a program uses `@model "fast"` or `using: "smart"`, Keel maps those names to Ollama tags via environment variables:

```bash
export KEEL_MODEL_FAST=gemma4
export KEEL_MODEL_SMART=mistral:7b-instruct
```

The lookup order when a call wants model `X`:

1. `ollama:X` prefix — strip and use `X` directly as the Ollama tag
2. `KEEL_MODEL_<X>` environment variable (`X` uppercased, `-` → `_`)
3. `KEEL_OLLAMA_MODEL` (catch-all)
4. Configuration error — the call fails with instructions for fixing it

## Testing without a real LLM

```bash
export KEEL_LLM=mock
```

All `Ai.*` calls return `CallFailed` and hit their `fallback:` / `??` branch. Used by the integration test suite.

## Troubleshooting

**`Ollama unreachable at http://localhost:11434`** — the daemon isn't running. Start it: `ollama serve &`.

**`Model 'X' has no mapping`** — you called `@model "X"` but there's no matching `KEEL_MODEL_X` variable and no `KEEL_OLLAMA_MODEL`. Set one of them.

**`Ollama returned 404`** — the tag isn't pulled locally. Fix it: `ollama pull <tag>`.
