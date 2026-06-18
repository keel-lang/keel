---
name: release
description: Full Keel release checklist. Trigger when the user says "release", "ship a release", "cut a release", "release v0.x.y", "publish a new version", "tag and release", or "release keel". Runs format → lint → tests → docs → metadata → confirmation gates → commit → tag. Always gates on explicit user confirmation before committing, pushing to main, or tagging.
---

# Keel Release Checklist

Run every step in order. Stop on any failure and report the error before asking what to do next. After all checks pass, gate on explicit user confirmation before committing, pushing, or tagging.

## Step 1 — Format

```bash
cargo fmt
```

Then format every new or modified `.keel` example:

```bash
cargo run -- fmt examples/<changed>.keel
```

`cargo fmt` applies to all Rust source. Any diff it produces is included in the release commit — do not commit before running it.

## Step 2 — Lint

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Zero warnings allowed. Fix every warning; never suppress with `#[allow(...)]` unless the suppression already existed.

## Step 3 — Tests

```bash
cargo test
```

**Known exemption:** `email_fetch_without_config_is_empty_list` requires live IMAP credentials and may fail. All other failures are blocking.

## Step 4 — Examples Parse

```bash
cargo test examples_all_parse --test integration_tests
```

Every `.keel` file in `examples/` must be listed in `examples_all_parse` and pass `keel check`. Add any new example to the list first.

## Step 4a — Doc Code Block Audit

Every `.keel` code block in `docs/src/` and every inline example in `CHANGELOG.md`, `SPEC.md`, and `ROADMAP.md` must be valid, runnable Keel code.

For each block, ask:

1. **Is it syntactically valid?** Paste it into `keel check` (via a temp file). If the block is intentionally a fragment (e.g. a single expression without a wrapping `agent`), wrap it in a minimal harness first:
   ```keel
   agent _Check { @on_start { <snippet> } }
   run(_Check)
   ```
2. **Is the type correct?** Pay special attention to operator expressions. `str + int` is invalid. String interpolation (`"{x}"`) is required when mixing types.
3. **Does it match the feature it documents?** The example must actually demonstrate the described behaviour — not a subtly wrong variant.

Any block that fails check or contains a type error is a **blocking failure**. Fix it before continuing to Step 5.

Common mistakes to look for:
- `str + int` instead of `"{str} {int}"` (string interpolation)
- Calling methods that don't exist on the given type
- Using `=` where the variable hasn't been declared yet in scope
- Missing `run(AgentName)` in standalone examples

## Step 5 — Docs Update

Update every affected page:

- `docs/src/guide/` — must fully describe the feature with syntax and examples.
- `docs/src/examples/` — update if any example program changed.
- `docs/src/release-notes.md` — one entry per release at the top.
- `docs/src/SUMMARY.md` — add a link if a new page was added.

Tag unimplemented features:

```html
<span class="badge badge-soon">Coming soon</span>
```

with a `> Status:` callout beneath it. Touching only `release-notes.md` is not enough.

## Step 6 — Docs Build

```bash
mdbook build docs/
```

Must exit clean with no errors and no broken links.

## Step 7 — Spec & Metadata

### Stamp the release date and tagline (required — never skip)

`CHANGELOG.md` and `docs/src/release-notes.md` both use an `[Unreleased]` section at the top. `docs/src/introduction.md` uses `%%VERSION%%` and `%%TAGLINE%%` placeholders. Stamp all of them now:

```bash
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
TODAY=$(date -u +%Y-%m-%d)

TAGLINE=$(grep '^%%TAGLINE%%' CHANGELOG.md | head -1 | sed 's/^%%TAGLINE%% //')
echo "Tagline: ${TAGLINE}"
```

**Stop here.** Read the tagline output. If it still says `update this line before releasing — one sentence summary of the release`, update the `%%TAGLINE%%` line in `CHANGELOG.md` to a real one-sentence summary before continuing.

Once the tagline is correct, stamp everything:

```bash
# macOS requires sed -i '' (empty string); Linux accepts sed -i without it.
# Use this portable form for both:
sed -i '' "s/%%TAGLINE%%/${TAGLINE}/" docs/src/introduction.md
sed -i '' "s/%%VERSION%%/${VERSION}/" docs/src/introduction.md
sed -i '' '/^%%TAGLINE%%/d' CHANGELOG.md
sed -i '' "s/^## \[Unreleased\]/## [${VERSION}] — ${TODAY}/" CHANGELOG.md
sed -i '' "s/^## \[${VERSION}\]/## [Unreleased]\n\n%%TAGLINE%% update this line before releasing — one sentence summary of the release\n\n---\n\n## [${VERSION}]/" CHANGELOG.md
sed -i '' "s/^## Unreleased/## v${VERSION} — ${TODAY}/" docs/src/release-notes.md
sed -i '' "s/^## v${VERSION}/## Unreleased\n\n---\n\n## v${VERSION}/" docs/src/release-notes.md
sed -i '' "s/\[${VERSION}\]/[%%VERSION%%]/" docs/src/introduction.md
```

Verify:

```bash
grep "Latest:" docs/src/introduction.md
```

Must show the real tagline (not `%%TAGLINE%%`) and `%%VERSION%%` restored as a placeholder.

### Verify before committing:

- `SPEC.md` — updated if the language surface changed.
- `CHANGELOG.md` — `[Unreleased]` stamped to `[VERSION] — DATE`.
- `ROADMAP.md` — shipped items marked `[x]`, new release row added.
- `Cargo.toml` — `version` bumped.
- `docs/status/features.json` — updated for any changed namespace, attribute, or CLI entry. `features.json` is the source of truth.

## Step 7a — Status Consistency

```bash
cargo test --test status_docs_tests
```

All five tests must pass.

## Step 8 — Integration Tests

Every new feature must have at least one integration test in `tests/integration_tests.rs`.

## Step 9 — Confirmation Gate for Commit (REQUIRED)

Show the user:

1. The list of files staged (`git status --short`).
2. The proposed commit message.
3. Ask: **"Ready to commit and push to main? (yes/no)"**

Do not run `git commit` or `git push` until the answer is an explicit yes.

Once approved:

```bash
git add Cargo.toml Cargo.lock CHANGELOG.md ROADMAP.md SPEC.md \
        src/ tests/ docs/ examples/ .github/
git commit -m "Release v0.1.X — <short theme>"
git push origin main
```

Commit message rules:

- **Subject line only — never add a body.** The release commit is a single `-m` line and nothing else. The CHANGELOG is the canonical record of what shipped; do not duplicate or paraphrase it in the commit. Never pass a second `-m`, a heredoc, or bullet points. A body is a blocking mistake.
- **The `<short theme>` must come from the actual release.** Derive it from the `%%TAGLINE%%`/CHANGELOG entry for this version — never invent feature names. If the subject doesn't match what the CHANGELOG says shipped, stop and fix it.
- Describe what the code change does — no AI attribution, no model names, no `Co-Authored-By`.

## Step 10 — Confirmation Gate for Tag (REQUIRED)

Show the user:

1. The version being released (from `Cargo.toml`).
2. The full CHANGELOG entry for that version.
3. The exact tag commands that will run.
4. Ask: **"Ready to tag v\<version\> and trigger the CI release? (yes/no)"**

Do not proceed until the answer is an explicit yes.

## Step 11 — Tag (after confirmation only)

Before tagging, verify HEAD contains the version bump:

```bash
git show HEAD:Cargo.toml | grep '^version'
```

The output must show `version = "<version>"`. If it does not, the release commit was never made — stop and commit before tagging.

```bash
git tag v<version>
git push origin v<version>
```

CI builds `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu` binaries and publishes the GitHub release automatically.

**Never run `gh release create` manually** — CI does it; a manual run creates a duplicate.
