# keel run

Execute an Keel program.

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
keel run examples/email_agent.keel

# With Ollama
KEEL_OLLAMA_MODEL=gemma4 keel run agent.keel

# With Anthropic
ANTHROPIC_API_KEY=sk-ant-... keel run agent.keel
```

## Behavior

- Agents with `every` blocks run continuously until `Ctrl+C`
- The first tick executes immediately
- Errors in the first tick are fatal (program exits)
- Errors in subsequent ticks are logged but don't stop the agent
