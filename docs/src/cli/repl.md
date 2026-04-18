# keel repl

> **Alpha (v0.1).** Breaking changes expected.

Interactive REPL for testing types, tasks, and expressions.

```bash
keel repl
```

## Usage

```
Keel REPL v0.1
Type expressions, define tasks, or :help for commands.

keel> type Mood = happy | sad | neutral
  ✓ type Mood

keel> task greet(name: str) -> str {
  ...   "Hello, {name}!"
  ... }
  ✓ task greet

keel> :types
  Mood = happy | sad | neutral

keel> :quit
Goodbye.
```

## Commands

| Command | Description |
|---------|-------------|
| `:help` | Show help |
| `:types` | List defined types |
| `:env` | Show environment |
| `:clear` | Reset all state |
| `:quit` | Exit |

## Features

- **Multi-line input** — open braces automatically continue to the next line
- **History** — up/down arrows navigate command history (saved to `~/.keel_history`)
- **Ctrl+C** — cancels current input (doesn't exit)
- **Ctrl+D** — exits
