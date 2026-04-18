# Ollama Setup

> **Alpha (v0.1).** Breaking changes expected.

[Ollama](https://ollama.com) runs LLMs locally. No API key, fully offline. In v0.1 it is the only supported backend for `Ai.*`.

## Install Ollama

```bash
# macOS
brew install ollama

# Or download from https://ollama.com
```

## Pull a model

```bash
ollama pull gemma4                  # general-purpose
ollama pull mistral:7b-instruct     # smaller, fast for classification
```

## Start the server

```bash
ollama serve
```

Default address: `http://localhost:11434`.

## Configure Keel

```bash
# Use one model for everything
export KEEL_OLLAMA_MODEL=gemma4
keel run agent.keel
```

## Per-model aliases

If your program uses `@model "fast"` or `using: "smart"`, map those aliases to Ollama tags:

```bash
export KEEL_MODEL_FAST=gemma4
export KEEL_MODEL_SMART=mistral:7b-instruct
```

Then in your program:

```keel
urgency = Ai.classify(email.body, as: Urgency, using: "fast")
reply   = Ai.draft("response to {email}", using: "smart")
```

## Custom host

```bash
export OLLAMA_HOST=http://192.168.1.100:11434
keel run agent.keel
```

## Verify

```bash
KEEL_OLLAMA_MODEL=gemma4 keel run examples/test_ollama.keel
```

Expected output:

```
⚡ LLM provider: Ollama (http://localhost:11434)
   * → gemma4
▸ Starting agent LocalTest
  🤖 Classifying as [happy, neutral, sad] using gemma4 (ollama @ ...)
  ✓ Result: happy
```
