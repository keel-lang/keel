# Contributing to Keel

Thanks for being here. **Keel** is a programming language where AI agents are first-class
citizens — a small actor-model core with AI, scheduling, HTTP, email, memory, and human I/O
in the standard library. It's built in Rust and it's **alpha (v0.2)**: no production users, no
stable API, breaking changes expected between `0.x` releases.

That status is an invitation, not a warning. The surface is still soft enough that a good idea
can change the language. This guide tells you how to land one.

- 📐 **Spec:** [`SPEC.md`](SPEC.md) — the source of truth for the language surface
- 📓 **Changelog:** [`CHANGELOG.md`](CHANGELOG.md) — shipped history
- 🚫 **Non-goals:** [`NON-GOALS.md`](NON-GOALS.md) — deliberately declined ideas (check before proposing)
- 🎯 **Vision:** [`VISION.md`](VISION.md) — where this is going and why
- 🗺️ **Roadmap & issues:** [GitHub Issues](https://github.com/keel-lang/keel/issues) · [project board](https://github.com/orgs/keel-lang/projects/1)

---

## Table of contents

- [Code of conduct](#code-of-conduct)
- [Ways to contribute](#ways-to-contribute)
- [Quick start](#quick-start)
- [Project layout](#project-layout)
- [Development workflow](#development-workflow)
- [Adding or changing a language feature](#adding-or-changing-a-language-feature)
- [Testing](#testing)
- [Coding standards](#coding-standards)
- [Commits & pull requests](#commits--pull-requests)
- [Reporting bugs & proposing features](#reporting-bugs--proposing-features)
- [Releases](#releases)
- [License](#license)

---

## Code of conduct

Be decent. Assume good faith, critique ideas not people, and keep discussion technical.
Maintainers may remove comments, commits, and contributors that don't meet that bar.

---

## Ways to contribute

You don't need to write Rust to help.

| Contribution | Where it lands |
| --- | --- |
| 🐛 **Report a bug** | [open an issue](https://github.com/keel-lang/keel/issues/new) with a minimal `.keel` repro |
| 💡 **Propose a feature** | open a discussion/issue — run it past [`NON-GOALS.md`](NON-GOALS.md) first |
| 📝 **Improve docs** | `docs/src/` (mdBook) — guides, examples, CLI/config reference |
| ✨ **Write an example** | `examples/*.keel` — small, focused, runnable programs |
| 🔧 **Fix a bug / build a feature** | `src/` + `crates/` — see below |
| 🧪 **Add test coverage** | `tests/` — the parts of the surface that aren't pinned yet |

Good first issues are tagged on the [board](https://github.com/orgs/keel-lang/projects/1). If
you're unsure whether an idea fits, open an issue *before* writing code — it's cheaper to align
on direction than to rework a PR.

---

## Quick start

**Prerequisites**

- **Rust** — current stable toolchain (`rustup default stable`). The workspace targets Rust
  **edition 2024**, so you need a recent stable (1.85+). CI runs on `dtolnay/rust-toolchain@stable`.
- **[`just`](https://github.com/casey/just)** *(recommended)* — task runner for the recipes below.
- **[`mdbook`](https://rust-lang.github.io/mdBook/)** *(docs only)* — `cargo install mdbook`.

```bash
git clone https://github.com/keel-lang/keel.git
cd keel

# Build the toolchain
cargo build                 # or: just build debug

# Run an example (mock LLM — no API keys, no network)
KEEL_LLM=mock KEEL_ONESHOT=1 cargo run -- run examples/hello_world.keel
just hello                  # shorthand for the line above

# Everything CI checks, in one command
just check                  # fmt --check · clippy -D warnings · doc · build · test
```

`just` with no arguments lists every recipe.

---

## Project layout

Keel is a Cargo **workspace**. The root binary (`keel-lang`) wires together four library crates
plus the CLI, REPL, and LSP.

```
src/
  main.rs / cli/      # CLI entry (clap): run · check · fmt · init · repl · lsp
  pipeline.rs         # source → tokens → AST → HIR → typed program → execution
  repl.rs             # interactive REPL
  lsp/                # Language Server (diagnostics, completion, hover, go-to-def)
  catalog.rs          # stdlib namespace surface
  vm/                 # placeholder — `keel build` is deferred post-v0.1

crates/
  keel-syntax/        # front-end: lexer (logos), AST, parser (chumsky), formatter, linter
  keel-compiler/      # middle-end: HIR, type checker, module graph, IDE queries
  keel-runtime/       # execution engine: tree-walking async interpreter + stdlib runtime
  keel-catalog/       # neutral stdlib method descriptors + capability metadata

examples/             # .keel programs (every feature ships with one)
tests/                # integration + pipeline coverage tests
docs/src/             # mdBook documentation (guides, CLI, config, release notes)
brand/                # logo, color tokens, mdBook theme (single source of truth)
SPEC.md               # language spec — source of truth for the surface
```

**Architectural anchors** (from `AGENTS.md`):

- **BoxedParser everywhere** — chumsky parsers are boxed to avoid a macOS linker crash on
  deeply nested types. Keep new parser combinators boxed.
- **Async recursion via `Pin<Box<dyn Future>>`** — the interpreter's recursive async functions
  follow this pattern; match it.
- **No silent fallbacks** — an unmapped LLM model, a malformed input, a contract violation: all
  fail with an actionable error. Never mock, truncate, or silently drop data to make something
  "work."
- **Newlines are statement separators** — the lexer normalizes them; the parser uses them for
  statement boundaries.

---

## Development workflow

All recipes are in the `justfile`. The raw `cargo` equivalent is shown for reference.

| Task | `just` | `cargo` |
| --- | --- | --- |
| Build (release) | `just build` | `cargo build --release` |
| Build (debug) | `just build debug` | `cargo build` |
| Format | `just fmt` | `cargo fmt` |
| Format check | `just fmt check` | `cargo fmt --check` |
| Lint | `just lint` | `cargo clippy --all-targets --all-features -- -D warnings` |
| Test (all) | `just test` | `cargo test` |
| Test (unit / integration) | `just test unit` · `just test integration` | `cargo test --lib` · `cargo test --test '*'` |
| Filter tests | `just test-filter <name>` | `cargo test <name>` |
| Coverage | `just cov` / `just cov html` | `cargo llvm-cov` |
| Run a file | `just run path.keel` | `cargo run -- run path.keel` |
| Run an example | `just run-example hello_world` | — |
| Run all examples | `just run-all-examples` | — |
| REPL | `just repl` | `cargo run -- repl` |
| Docs (serve / build) | `just docs` / `just docs build` | `cd docs && mdbook serve` |
| **Everything CI runs** | `just check` | — |

> **Before you push:** `just check` must pass clean. It mirrors CI exactly —
> `fmt --check`, `clippy -D warnings`, `cargo doc`, build, and the full test suite.

### Useful environment variables

Keel reads a number of env vars; these are the ones you'll want while hacking:

- `KEEL_LLM=mock` — deterministic mock LLM. **Use this for tests and local runs** — no keys, no network.
- `KEEL_ONESHOT=1` — exit after the first idle window (so examples terminate).
- `KEEL_REPL=1` — suppress agent boilerplate (REPL mode).
- `KEEL_TRACE=1` (`--trace`) — verbose narration of every LLM call.
- `KEEL_LOG_LEVEL=debug|info|warn|error` (`--log-level`) — threshold for `Log.*`.
- `KEEL_PROVIDER=ollama|openai|anthropic` — program-default LLM backend (default `ollama`).

The full list lives in [`AGENTS.md`](AGENTS.md).

---

## Adding or changing a language feature

Keel is a language, so changes to its surface go through more gates than a typical app. Follow
this order — it's the same one maintainers use.

1. **Validate the design first.** *Before touching `SPEC.md` or any source*, validate the idea
   against the project's design principles (gradual typing, IDE-driven design, pragmatic type
   systems, developer productivity). Contributors using Claude Code should invoke the
   `design-lang` skill; everyone should be able to articulate *why* the feature earns its
   complexity. Check it isn't already in [`NON-GOALS.md`](NON-GOALS.md).
2. **Update `SPEC.md`.** The spec is the source of truth. Write the surface before you implement it.
3. **Implement** across the relevant crate(s):
   - new syntax → `keel-syntax` (lexer/parser/AST)
   - type rules → `keel-compiler` (HIR + type checker)
   - runtime behavior → `keel-runtime` (interpreter + stdlib)
4. **Add tests and an example.** Every feature ships with integration tests **and** a runnable
   `examples/*.keel` program. This is non-negotiable — see [Testing](#testing).
5. **Update `CHANGELOG.md`** under `## [Unreleased]`, with a `.keel` example for features and an
   explanation of what broke for fixes. Never hand-write a version number or date — the release
   process stamps those. Keep the `%%TAGLINE%%` one-liner current.
6. **Update `docs/src/`.** Touch `release-notes.md` *and* every guide page a user would land on
   from search. A change isn't done until `mdbook build` runs clean. Tag partial features with
   `<span class="badge badge-soon">Coming soon</span>` and a `> Status:` callout.

### Runtime type-dispatch checklist

When you write or modify code that dispatches on `TypeExpr` or `Value`, the project enforces a
few rules that have bitten before (full text in [`AGENTS.md`](AGENTS.md)):

- **Enumerate every variant** of the matched enum; justify every `_ =>` fallback in a comment.
  `TypeExpr` variants: `Named`, `List`, `Set`, `Map`, `Tuple`, `Nullable`, `Struct`, `Func`, `Generic`.
- **Audit every exit path** in param/arg loops — variadic `break` and named-arg `continue` skip
  fall-through logic.
- **Validate all structural preconditions** on stdlib inputs (uniqueness, non-empty, width
  consistency). Match types strictly (`Value::String`), don't lean on `.to_string()` (via
  `Display`) as a coercive catch-all. Never silently drop, truncate, or overwrite data — raise
  an error.

---

## Testing

> **Every new feature must include integration tests and a `.keel` example before it ships.**
> Bug fixes need a regression test that fails before the fix and passes after.

- **Unit tests** live alongside the code they test (`cargo test --lib`).
- **Integration tests** live in `tests/` (`cargo test --test '*'`).
- **Run tests with `KEEL_LLM=mock`** so they're deterministic and offline.
- **Examples are tested too** — `just run-all-examples` runs every `examples/*.keel`; a broken
  example breaks CI.

What makes a good test here: assert on **observable behavior** (program output, diagnostics,
emitted errors), not on internal mechanism. A test that survives an implementation rewrite which
preserves behavior is a good test; one that pins a private call order is not. Don't write
tautological tests that can't fail given the code under test.

---

## Coding standards

This is idiomatic, modern Rust. The bar:

- **`cargo fmt`** — run it after every edit. `rustfmt.toml` pins edition 2024 + Unix newlines so
  formatting is deterministic across contributors.
- **`cargo clippy --all-targets --all-features -- -D warnings`** — zero warnings. The workspace
  denies `clippy::correctness` and warns on `suspicious`, `perf`, `style`, and `complexity`.
- **`cargo doc --no-deps --document-private-items`** — must build clean; doc links are checked.
- **API boundaries validate input.** At any public boundary that accepts user data through a
  permissive mechanism (flexible parsers, `HashMap::insert`, `to_string()` coercions), confirm
  the input round-trips into the declared output. If it can't — duplicate keys, out-of-bounds
  indices, wrong element types — return an error. Never let a permissive default silently drop
  data.
- **Private helpers use generic error messages** — don't hardcode a public function name in a
  shared helper's error string; pass `caller: &str` if the identity matters.

The shortcut for all of the above: **`just check` must be green before you open a PR.**

---

## Commits & pull requests

**Commits**

- Write clear, imperative commit subjects that describe *what the change does*:
  `Fix race condition in file watcher initialization`, not `wip` or `fixes`.
- Keep history clean: squash noisy intermediate commits before opening the PR (aim for one
  logical change per commit). Only rewrite history on **unpushed** commits.
- If your change closes an issue, include `Close #N` in the commit message so GitHub links and
  closes it on merge.

**Pull requests**

1. Branch off `main` (don't commit directly to `main`).
2. Make the change, with tests, docs, changelog, and an example as applicable.
3. Run `just check` — it must pass.
4. Open the PR with a description of *what* changed and *why*. Link the issue it addresses.
5. CI runs on every PR (`fmt`, `clippy`, `doc`, release-profile check, full test suite). Green CI
   is required to merge.

PRs that touch the language surface should reference the `SPEC.md` change in the same PR.

---

## Reporting bugs & proposing features

**Bugs** — open an issue with:

- A **minimal `.keel` reproduction** (smaller is better).
- The command you ran (e.g. `keel run repro.keel`) and the full output.
- What you expected vs. what happened.
- `keel --version` and your OS.

**Features** — open an issue describing the problem first, not just the solution. State the use
case, sketch the surface, and explain why the standard library / language should own it rather
than user code. Check [`NON-GOALS.md`](NON-GOALS.md) so you're not re-proposing something that's
been deliberately declined — and if you think a non-goal should be reconsidered, say so explicitly.

---

## Releases

Releases are run by maintainers through a gated checklist and CI — contributors don't need to cut
them. Notably: **never hand-write version numbers or dates** into `CHANGELOG.md` or docs; the
release tooling stamps those automatically. Just keep `## [Unreleased]` accurate and the tagline
current, and your change will be released correctly.

---

## License

Keel is licensed under the **MIT License**. By contributing, you agree that your contributions
will be licensed under the same terms.

---

Questions that aren't a bug report? Open a discussion or an issue. Welcome aboard. ⚓
