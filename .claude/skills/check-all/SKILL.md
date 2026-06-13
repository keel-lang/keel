---
name: check-all
description: Run the full Keel validation pipeline mirroring CI — fmt check, clippy, doc, tests. Use before declaring any feature or fix done.
disable-model-invocation: true
---

Run the following steps in order. Stop and report failure on the first error.

1. **Format check** — `cargo fmt --check`
2. **Lint** — `cargo clippy --all-targets --all-features -- -D warnings`
3. **Docs** — `cargo doc --no-deps --document-private-items`
4. **Tests** — `KEEL_LLM=mock cargo test`

For each step, print `✓ step-name` on success or `✗ step-name` followed by the error output on failure. After all steps (or on first failure), print a one-line summary: how many passed, which failed.
