# CLAUDE.md

## Project

**Keel** — a programming language where AI agents are first-class citizens. Built in Rust.

## Structure

```
src/
  lexer.rs          # Token definitions (logos)
  parser.rs         # Grammar (chumsky 0.9, BoxedParser)
  ast.rs            # AST node types
  types/            # Type checker (inference, nullable safety, exhaustiveness)
  interpreter/      # Tree-walking async interpreter
  vm/               # Bytecode compiler + register-based VM
  runtime/          # LLM client (Anthropic/Ollama), email (IMAP/SMTP), human I/O
  formatter.rs      # Pretty-printer (keel fmt)
  repl.rs           # Interactive REPL
  lsp.rs            # Language Server Protocol
  main.rs           # CLI entry (clap)
examples/           # .keel example programs
tests/              # Lexer, parser, type checker, interpreter, integration tests
docs/               # mdBook documentation
editors/vscode/     # VS Code extension (TextMate grammar + LSP config)
```

## Key Design Decisions

- **Statically typed with full inference** — every expression has a known type, `SPEC.md` is the source of truth
- **No silent fallbacks** — unmapped LLM models fail with actionable errors, not mock responses
- **Newlines as statement separators** — lexer normalizes newlines, parser uses them for statement boundaries
- **BoxedParser everywhere** — required to avoid macOS linker crash on deeply nested chumsky types
- **Async recursion** — interpreter uses `Pin<Box<dyn Future>>` for all recursive async functions
- **KEEL_LLM=mock** for tests, **KEEL_REPL=1** suppresses agent boilerplate in REPL

## Conventions

- `.keel` file extension, `.keelc` for compiled bytecode
- Examples in `examples/`, tests in `tests/`
- Update `SPEC.md` before implementing new language features
- Update `CHANGELOG.md` with every feature (include .keel example) and bug fix (explain what broke)
- Env vars: `KEEL_OLLAMA_MODEL`, `KEEL_MODEL_CLAUDE_*`, `KEEL_LLM`, `KEEL_REPL`, `KEEL_ONESHOT`

## CLI

```
keel run file.keel       # execute
keel check file.keel     # type-check only
keel build file.keel     # compile to bytecode
keel fmt file.keel       # auto-format
keel init project-name   # scaffold
keel repl                # interactive
keel lsp                 # language server (stdin/stdout)
```

## Reserved Keywords

```
agent task role model tools connect memory state config
every after at fetch send search classify extract summarize
draft translate decide ask confirm notify show remember
recall forget delegate broadcast run stop wait retry
fallback when use from for if else try catch in where
to via with true false none now env self user type
extern parallel race prompt http sql set and or not
return team archive format rules limits using
```
