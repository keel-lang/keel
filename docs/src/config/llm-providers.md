# LLM Providers

> **Alpha (v0.1).** Breaking changes expected. LLM providers implement the `LlmProvider` interface — see [The Prelude & Interfaces](../guide/prelude.md).

Keel supports multiple LLM providers. The provider is selected automatically based on environment variables.

## Provider priority

1. **`ANTHROPIC_API_KEY`** set → Anthropic Claude API
2. **Otherwise** → Ollama (local)

There is no silent fallback. If a model can't be reached, the program fails with a clear error message.

## Anthropic Claude

```bash
export ANTHROPIC_API_KEY=sk-ant-api03-...
keel run agent.keel
```

Model names in `.keel` files map to API model IDs:

| Keel name | API model ID |
|-----------|-------------|
| `claude-haiku` | `claude-haiku-4-5-20251001` |
| `claude-sonnet` | `claude-sonnet-4-6-20260415` |
| `claude-opus` | `claude-opus-4-6-20260415` |

## Ollama (local)

See [Ollama Setup](./ollama.md) for detailed instructions.

## Model mapping

When using Ollama, Keel model names (like `claude-haiku`) need to be mapped to local models:

```bash
# Catch-all: all models → one Ollama model
export KEEL_OLLAMA_MODEL=gemma4

# Per-model: different local models for different roles
export KEEL_MODEL_CLAUDE_HAIKU=gemma4              # fast/cheap
export KEEL_MODEL_CLAUDE_SONNET=mistral:7b-instruct  # capable
export KEEL_MODEL_CLAUDE_OPUS=gpt-oss:20b           # heavy
```

## Direct Ollama model names

You can also use Ollama model names directly in `.keel` files:

```keel
classify text as Mood using "ollama:gemma4"
draft "response" using "ollama:mistral:7b-instruct"
```

## Strict validation

If a model name can't be resolved, Keel fails immediately with instructions:

```
✗ Model 'claude-haiku' is not available locally.
Set one of:
  export KEEL_MODEL_CLAUDE_HAIKU=<ollama_model>
  export KEEL_OLLAMA_MODEL=<ollama_model>
```

## Test mode

For automated tests:

```bash
KEEL_LLM=mock keel run agent.keel
```

AI primitives use `fallback` values. No network calls are made.
