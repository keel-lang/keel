# CLAUDE.md

## Project

**Keel** — a programming language where AI agents are first-class citizens. Built in Rust.

## Structure

```
src/
  lexer.rs          # Token definitions (logos)
  parser.rs         # Grammar (chumsky 0.9, BoxedParser)
  ast.rs            # AST node types
  types/            # Type checker (enum exhaustiveness, arg arity, scope;
                    #   full nullable-safety enforcement is WIP — see ROADMAP)
  interpreter/      # Tree-walking async interpreter
  vm/               # Placeholder module for v0.1 — `keel build` is deferred
                    #   post-v0.1; the tree-walking interpreter is the only
                    #   execution path shipping today
  runtime/          # LLM client (Ollama), email (IMAP/SMTP), human I/O, prelude namespaces
  formatter.rs      # Pretty-printer (keel fmt)
  repl.rs           # Interactive REPL
  lsp.rs            # Language Server Protocol (diagnostics only in v0.1)
  main.rs           # CLI entry (clap)
brand/              # Logo, color tokens, mdBook theme (single source of truth)
examples/           # .keel example programs
tests/              # Lexer, parser, type checker, formatter, lsp, integration
docs/               # mdBook documentation
                    # VS Code extension lives at github.com/keel-lang/vscode-keel
```

## Key Design Decisions

- **Statically typed, inference-first** — every expression has a known type; the checker currently catches scope, arity, and enum-exhaustiveness issues. Full nullable enforcement and return-type matching are in-progress — see `ROADMAP.md`. `SPEC.md` is the source of truth for the surface.
- **No silent fallbacks** — unmapped LLM models fail with actionable errors, not mock responses.
- **Newlines as statement separators** — lexer normalizes newlines, parser uses them for statement boundaries.
- **BoxedParser everywhere** — required to avoid macOS linker crash on deeply nested chumsky types.
- **Async recursion** — interpreter uses `Pin<Box<dyn Future>>` for all recursive async functions.
- **KEEL_LLM=mock** for tests, **KEEL_REPL=1** suppresses agent boilerplate in REPL.

## Conventions

- `.keel` file extension. (`.keelc` for compiled bytecode is reserved — `keel build` is deferred post-v0.1.)
- Examples in `examples/`, tests in `tests/`, brand assets in `brand/`.
- Update `SPEC.md` before implementing new language features.
- Update `CHANGELOG.md` with every feature (include .keel example) and bug fix (explain what broke).
- Update `ROADMAP.md` when a feature ships, gets stubbed, or shifts scope.
- **Update `docs/src/` for every feature/spec change — added, updated, or removed.** Touch `docs/src/release-notes.md` plus every relevant guide page in `docs/src/guide/` (and `docs/src/examples/`, `docs/src/cli/`, `docs/src/config/`). A release is not done until `mdbook build` runs clean over the updated pages. Touching only `release-notes.md` is not enough — the guide pages users land on from search must reflect the change too.
- Tag partial / unimplemented features in `docs/src/` with `<span class="badge badge-soon">Coming soon</span>` plus a `> Status:` callout.
- Env vars: `KEEL_OLLAMA_MODEL` (default model), `KEEL_MODEL_<ALIAS>` (per-alias model tags), `OLLAMA_HOST` (default `http://localhost:11434`), `KEEL_LLM=mock` (test mode), `KEEL_REPL=1` (REPL mode), `KEEL_ONESHOT=1` (exit after first idle window), `KEEL_TRACE=1` (verbose LLM call narration; `--trace` sets this), `KEEL_LOG_LEVEL=debug|info|warn|error` (threshold for `Log.*`; `--log-level` sets this).

## CLI

```
keel run file.keel       # execute
keel check file.keel     # type-check only
keel fmt file.keel       # auto-format
keel init project-name   # scaffold
keel repl                # interactive
keel lsp                 # language server (stdin/stdout)
# keel build             # deferred post-v0.1
```

## Release Checklist

> **Skill available:** use `/release` to run through this checklist interactively. The skill walks through every step, asks for confirmation before tagging, and prevents the most common mistakes. To save cost, switch to a cheaper model first: `/model haiku` → `/release`.

Every release must pass all of the following before the version is bumped and CHANGELOG/ROADMAP are updated. Do not skip steps or mark a release done until the full list is green.

**1. Format**
```
cargo fmt
```
Run `keel fmt` on every new or modified example in `examples/`:
```
cargo run -- fmt examples/<name>.keel
```

**2. Lint**
```
cargo clippy -- -D warnings
```
Zero warnings allowed. Fix, don't suppress.

**3. Tests — unit + integration**
```
cargo test
```
All tests must pass. The one known exception is `email_fetch_without_config_is_empty_list` which requires live IMAP credentials — all other failures are blocking.

**4. Examples parse cleanly**
```
cargo test examples_all_parse --test integration_tests
```
Every file in `examples/` must be listed in `examples_all_parse` and pass `keel check`.

**5. Docs update**
Every new or changed feature must be reflected in the docs before the build step:
- Add or update the relevant `docs/src/guide/` page — this is the page users land on; it must fully describe the feature with syntax and examples
- Add an entry to `docs/src/release-notes.md`
- Add a link in `docs/src/SUMMARY.md` if a new page was added
- Tag anything unimplemented with `<span class="badge badge-soon">Coming soon</span>` and a `> Status:` callout
- Touching only `release-notes.md` is not enough

**6. Docs build**
```
mdbook build docs/
```
Must exit clean with no errors or broken links.

**7. Spec & metadata**
- `SPEC.md` updated if the language surface changed
- `CHANGELOG.md` has a new `[x.y.z]` section with `.keel` examples for features and explanation for bug fixes
- `ROADMAP.md` items for this release marked `[x]` with status `shipped`
- `ROADMAP.md` has no stale `[ ]` markers — every item still open must be genuinely unimplemented; anything shipped in a prior release must be `[x]`
- `Cargo.toml` version bumped to the new version

**8. Integration tests**
Every new feature must have at least one integration test in `tests/integration_tests.rs`. Test names describe what is being tested — no version prefixes.

## Reserved Keywords (v0.1)

```
agent task interface type extern
use from
state on self
if else when where
for in
try catch return
as and or not
true false none now
set
```

28 words. Anything else — `classify`, `every`, `role`, `memory`, `tools`, `delegate`, `fetch`, `send`, `ask`, `confirm`, `run`, `stop`, … — is a prelude identifier, not a keyword. See `SPEC.md §10`.
