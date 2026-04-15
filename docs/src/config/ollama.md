# Ollama Setup

[Ollama](https://ollama.com) lets you run LLMs locally. No API key needed, fully offline.

## Install Ollama

```bash
# macOS
brew install ollama

# Or download from https://ollama.com
```

## Pull a model

```bash
ollama pull gemma4              # 9.6 GB — good general model
ollama pull mistral:7b-instruct  # 4.4 GB — fast, good for classification
```

## Start the server

```bash
ollama serve
```

Ollama runs at `http://localhost:11434` by default.

## Configure Keel

```bash
# Use one model for everything
export KEEL_OLLAMA_MODEL=gemma4
keel run agent.keel
```

## Per-model mapping

Different AI operations benefit from different models. Map them individually:

```bash
export KEEL_MODEL_CLAUDE_HAIKU=gemma4              # fast: classify, triage
export KEEL_MODEL_CLAUDE_SONNET=mistral:7b-instruct  # capable: draft, summarize
```

Then in your `.keel` file:

```keel
# Uses gemma4 (fast)
urgency = classify email.body as Urgency using "claude-haiku"

# Uses mistral (capable)
reply = draft "response to {email}" using "claude-sonnet"
```

## Custom Ollama host

```bash
export OLLAMA_HOST=http://192.168.1.100:11434
keel run agent.keel
```

## Verify it works

```bash
KEEL_OLLAMA_MODEL=gemma4 keel run examples/test_ollama.keel
```

You should see:

```
⚡ LLM provider: Ollama (http://localhost:11434)
   * → gemma4
▸ Starting agent LocalTest
  ...
  🤖 Classifying as [happy, neutral, sad] using gemma4 (ollama @ ...)
  ✓ Result: happy
```
