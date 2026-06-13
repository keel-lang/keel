# keel run

Execute a Keel program.

```bash
keel run <file.keel>
```

## Pipeline

1. **Lex** — tokenize the source
2. **Parse** — build the AST
3. **Load modules** — resolve `use` imports transitively (each file parsed once; cycles rejected)
4. **Type check** — verify types, exhaustiveness, argument types across the whole module graph
5. **Execute** — run with tree-walking interpreter

The file you pass is the **entry file**: its top-level statements form the
implicit main and execute in order. Imported modules contribute
declarations only — their top-level statements run solely when that file is
executed directly. See [Modules & Imports](../guide/modules.md).

## Global flags

These flags apply to every subcommand; `keel run` uses them most.

| Flag | Effect |
|---|---|
| `--trace` | Surfaces internal runtime detail: LLM call metadata, input previews, per-call results, provider banner. Equivalent to `KEEL_TRACE=1`. Off by default. |
| `--log-level <LEVEL>` | Sets the threshold for `log.*` calls. One of `debug`, `info`, `warn`, `error`. Default `info`. Equivalent to `KEEL_LOG_LEVEL=<LEVEL>`. |

## Examples

```bash
# Run an agent
KEEL_OLLAMA_MODEL=gemma4 keel run examples/email_agent.keel

# Test without a real LLM
KEEL_LLM=mock keel run examples/hello_world.keel

# Verbose — show every ai.* call as it fires
keel --trace run examples/email_agent.keel

# Quiet production run — only warnings and errors from log.*
keel --log-level warn run examples/email_agent.keel
```

## Behavior

- Agents with scheduled blocks (`schedule.every`, `schedule.after`, etc.) run continuously until `Ctrl+C`
- The first tick executes immediately
- Errors in the first tick are fatal (program exits)
- Errors in subsequent ticks are logged but don't stop the agent
- `Ctrl+C` exits immediately regardless of what the runtime is blocked on (stdin prompt, LLM call, HTTP request, IMAP fetch) — exit code `130`
