---
name: release
description: This skill should be used when the user says "release", "ship a release", "cut a release", "release v0.x.y", "publish a new version", "tag and release", or "release keel". Guides through the full keel release checklist, gates on explicit user confirmation before creating the tag, and pushes the tag so CI publishes the GitHub release.
---

# Keel Release Workflow

Delegate to the release agent, which runs on Haiku to keep costs low:

```
Agent: .claude/agents/release.md
```

The agent runs the full checklist, commits and pushes to `main`, gates on explicit user confirmation, then tags to trigger CI. The steps below are the authoritative reference if running manually.

## Step 1 — Format all code and examples

```bash
cargo fmt
```

This formats all Rust source. If it produces any diff, those changes will be included in the release commit — do not commit before running this.

Then format every new or modified `.keel` example:

```bash
cargo run -- fmt examples/<changed>.keel
```

Run `keel fmt` on **all** examples in `examples/` if in doubt — the formatter is idempotent.

## Step 2 — Lint

```bash
cargo clippy -- -D warnings
```

Applies to all source. Zero warnings allowed. Fix every warning — never suppress with `#[allow(...)]` unless the suppression already existed before this release.

## Step 3 — Unit + Integration Tests

```bash
cargo test
```

**Known exemption:** `email_fetch_without_config_is_empty_list` requires live IMAP credentials and is allowed to fail. All other failures are blocking — fix before continuing.

## Step 4 — Examples Parse

```bash
cargo test examples_all_parse --test integration_tests
```

Every `.keel` file in `examples/` must be listed in `examples_all_parse` and pass `keel check`. Add any newly created example to the list before running.

## Step 5 — Docs Update

Update every affected page **before** the build step:

- `docs/src/guide/` — the page users land on from search; must fully describe the feature with syntax and examples.
- `docs/src/examples/` — update or add example walkthroughs if any example program changed.
- `docs/src/release-notes.md` — one entry per release at the top.
- `docs/src/SUMMARY.md` — add a link if a new page was added.

Tag anything not yet implemented:

```html
<span class="badge badge-soon">Coming soon</span>
```

with a `> Status:` callout beneath it.

Touching only `release-notes.md` is not enough. Every guide and example page the change touches must be updated.

## Step 6 — Docs Build

```bash
mdbook build docs/
```

Must exit clean with no errors and no broken links.

## Step 7 — Spec & Metadata

Verify the following before committing:

- `SPEC.md` — updated if the language surface changed (new syntax, new built-in, behaviour change).
- `CHANGELOG.md` — has a new `[x.y.z]` section at the top with:
  - `.keel` code examples for every new feature.
  - Plain-English explanation for every bug fix (what broke, what changed).
- `ROADMAP.md` — every item shipped in this release is marked `[x]`; the release has its own section; no stale `[ ]` for anything already shipped in a prior release.
- `Cargo.toml` — `version` bumped to the new version.

## Step 8 — Commit and Push to `main`

Stage all changed files explicitly (avoid `git add -A` to prevent accidental inclusion of `.env` or large binaries). This commit includes formatting diffs from Step 1 as well as all feature changes:

```bash
git add Cargo.toml Cargo.lock CHANGELOG.md ROADMAP.md SPEC.md \
        src/ tests/ docs/ examples/
```

Write the commit message as a human developer would — **no AI attribution, no model names, no "Co-Authored-By"**:

```
Release v0.1.X — <short theme in plain English>

<Body: what changed and why, 3–6 lines. Use past tense.
Reference the key design decision, not implementation details.>
```

Good examples:
- `Release v0.1.11 — Memory storage safety (path hash + flock)`
- `Release v0.1.9 — keel init fixes, stop(self), and keel lint`

Bad (never write):
- Anything mentioning Claude, an AI model, or a ticket system shortlink.
- `Co-Authored-By: ...` lines.

Then push to `main`:

```bash
git push origin main
```

## Step 9 — Confirmation Gate (REQUIRED)

Before creating the tag, **always stop and show the user**:

1. The version about to be released (read from `Cargo.toml`).
2. The full CHANGELOG section for that version (so the user can review it).
3. The exact commands that will run.
4. The question: **"Ready to tag v\<version\> and trigger the CI release? (yes/no)"**

Do not proceed until the user explicitly confirms. If they say anything other than a clear yes, stop and ask what they want to change first.

Example gate message:

```
Version:  v0.1.11
Tag:      git tag v0.1.11 && git push origin v0.1.11

CHANGELOG excerpt:
──────────────────────────────────────────────
## [0.1.11] — 2026-05-04
...
──────────────────────────────────────────────

This will trigger the CI pipeline which will publish the GitHub release
for aarch64-apple-darwin and x86_64-unknown-linux-gnu.

Ready to tag v0.1.11 and trigger the CI release? (yes/no)
```

## Step 10 — Tag and Release (after confirmation only)

```bash
git tag v<version>
git push origin v<version>
```

The CI pipeline handles everything after the tag push:
- Builds release binaries for `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`.
- Publishes the GitHub release with assets attached.

**Never run `gh release create` manually.** The CI does it; a manual run creates a duplicate.

## Quick Reference

| Step | Command | Blocking? |
|------|---------|-----------|
| Format | `cargo fmt` + `keel fmt examples/` | Yes — diff included in commit |
| Lint | `cargo clippy -- -D warnings` | Yes (zero warnings) |
| Tests | `cargo test` | Yes (except IMAP test) |
| Examples | `cargo test examples_all_parse` | Yes |
| Docs update | guide/ + examples/ + release-notes.md | Yes |
| Docs build | `mdbook build docs/` | Yes |
| Commit + push main | `git add … && git commit && git push` | Yes |
| **Confirm** | **Ask user** | **Gate — mandatory** |
| Tag | `git tag v… && git push origin v…` | After confirmation only |

## Common Mistakes

- **Forgetting `cargo fmt`** on all source, not just examples — it applies to all Rust code.
- **Not including fmt diffs in the commit** — run fmt first, include its changes in the release commit.
- **Touching only `release-notes.md`** — guide pages and example pages the change touches must also be updated.
- **Skipping `docs/src/examples/`** — if an example program changed, its walkthrough page needs updating too.
- **Suppressing clippy warnings** — fix the code instead.
- **Running `gh release create`** — CI handles it; a manual run creates a duplicate release.
- **Pushing the tag without confirming** — always show the gate, always wait for yes.
- **Adding `Co-Authored-By` or AI attribution to commits** — never include these.
- **Forgetting to add new examples to `examples_all_parse`** — the test will miss them silently unless they are listed.
