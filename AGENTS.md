# CLAUDE.md

## Project

**Keel** — a programming language where AI agents are first-class citizens. Built in Rust.

## Structure

```
src/
  lexer.rs          # Token definitions (logos)
  parser.rs         # Grammar (chumsky 0.9, BoxedParser)
  ast/              # AST node types
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
- **Before adding or modifying any language feature** (new syntax, new built-in, new stdlib namespace, behaviour change), invoke the `design-lang` skill to validate the idea against Hejlsberg's design principles. Do this before touching `SPEC.md` or any source file.
- Update `SPEC.md` before implementing new language features.
- Update `CHANGELOG.md` under the `## [Unreleased]` section with every feature (include .keel example) and bug fix (explain what broke). Never write a version number or date into the changelog by hand — the release agent stamps those automatically at release time. Keep the `%%TAGLINE%%` line at the top of `[Unreleased]` current — one sentence summarising the release in progress. The release agent stamps it into `docs/src/introduction.md` automatically and restores the placeholder for the next cycle.
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

Say "release" or "ship a release" to run the shared release workflow in `.agents/skills/release/SKILL.md`. Claude should delegate to the `release-engineer` agent (`.claude/agents/release-engineer.md`), which wraps that shared checklist. Codex should use the `release` skill directly. Both tools must walk through every step and gate on explicit confirmation before committing, pushing, or tagging.

## Runtime type dispatch checklist

When writing or modifying code that dispatches on `TypeExpr` or `Value`:
- **Enumerate ALL variants of the matched enum.** Justify every `_ =>` fallback in a comment.
  `TypeExpr` variants that need explicit handling: `Named`, `List`, `Set`, `Map`, `Tuple`,
  `Nullable`, `Struct` (inline), `Func`, `Generic`.
- **Audit all exit paths in param/arg loops.** Variadic `break` and named-arg `continue` both
  skip fall-through logic. Confirm any new logic runs on every exit path, not just the default one.

When changing a type-system invariant (e.g. struct nominal identity, new kind of type):
- **Grep all inference sites** that construct or compare that type — list literals, tuple
  literals, struct-spread updates, for-loop iterables, etc. — and verify the new invariant
  is enforced at every one of them.

When writing uniqueness/disambiguation logic (e.g. picking the single matching candidate):
- **Filter to the eligible set first, then check uniqueness within it.** A non-eligible
  candidate counted before filtering produces false ambiguity and silently suppresses the
  correct path.

## Namespace implementation checklist

When writing a new stdlib namespace method:
- **Validate all structural preconditions on inputs**, not just presence/type. For any data
  destined for a container: check uniqueness (map keys), non-empty (keys that will be
  looked up by name), and width consistency (row vs. header column count). Never silently
  drop, truncate, or overwrite data that violates the declared contract — raise an error.
- **Strict type matching over coercive display.** If the declared contract says `list[str]`,
  match on `Value::String` explicitly. `to_display_string()` as a catch-all silently accepts
  wrong types and produces un-round-trippable output.
- **Private helpers use generic error messages.** Never hardcode a specific public function
  name in a shared helper's error string. If the caller identity matters for diagnostics,
  accept `caller: &str` as a parameter.

## Reserved Keywords (v0.1)

```
agent task interface impl type extern
use from
state on self
if else when where
for while in break continue
try catch return raise
as and or not
true false none
set
```

Anything else — `classify`, `every`, `role`, `memory`, `tools`, `delegate`, `fetch`, `send`, `ask`, `confirm`, `run`, `stop`, … — is a prelude identifier, not a keyword. See `SPEC.md §10`.
