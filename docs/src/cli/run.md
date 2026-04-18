# keel run

> **Alpha (v0.1).** Breaking changes expected.

Execute a Keel program.

```bash
keel run <file.keel>
```

## Pipeline

1. **Lex** — tokenize the source
2. **Parse** — build the AST
3. **Type check** — verify types, exhaustiveness, argument types
4. **Execute** — run with tree-walking interpreter

## Examples

```bash
# Run an agent
KEEL_OLLAMA_MODEL=gemma4 keel run examples/email_agent.keel

# Test without a real LLM
KEEL_LLM=mock keel run examples/test_ollama.keel
```

## Behavior

- Agents with scheduled blocks (`Schedule.every`, `Schedule.after`, etc.) run continuously until `Ctrl+C`
- The first tick executes immediately
- Errors in the first tick are fatal (program exits)
- Errors in subsequent ticks are logged but don't stop the agent
