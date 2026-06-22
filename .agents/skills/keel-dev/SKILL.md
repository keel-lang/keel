---
name: keel-dev
description: Use for Keel feature development, behavior changes, multi-file fixes, architecture work, or issue implementation. Follow graphify-first exploration when available, then design, implement, verify, and update docs/tests/status surfaces.
---

# Keel Development

Use this skill for non-trivial Keel development: language features, runtime or checker behavior changes, namespace work, CLI/LSP behavior, docs-synced fixes, and issue implementation.

## Core Rules

- Understand the existing implementation before editing.
- If `graphify-out/GRAPH_REPORT.md` exists, read it before broad exploration. Use matching community hubs and node listings to choose files, then use targeted `rg` searches for gaps.
- For Rust edits, follow the `ms-rust` skill first.
- For language-surface changes, follow the `design-lang` skill before touching `SPEC.md` or source.
- Keep public behavior, docs, examples, tests, and editor-facing surfaces aligned.
- Preserve existing wording and output shape when the issue or spec says user-facing output should not change.

## Workflow

1. **Pin the request**
   - If the user references a GitHub issue, inspect the issue and treat it as the implementation spec.
   - Identify affected surfaces: parser, AST, checker, interpreter, runtime namespace, formatter, LSP, docs, examples, tests, status files.

2. **Explore**
   - Start with `graphify-out/GRAPH_REPORT.md` when present.
   - For larger tasks, use Codex subagents only when the user asks for parallel work or the task clearly benefits from it. Prefer the project `code-explorer` agent from `.codex/agents/code-explorer.toml`, or the built-in `explorer`.
   - Put this at the top of each explorer prompt:

     ```text
     A graphify knowledge graph is available at `graphify-out/GRAPH_REPORT.md`. Read that file first before doing any other exploration. Use the community clusters and node listings to navigate directly to relevant files; this replaces broad directory scanning. Read only the files the graph points you to, then use targeted searches for any remaining gaps.
     ```

3. **Clarify only when necessary**
   - Ask questions only if a reasonable implementation choice would be risky or materially change the language/API.
   - Otherwise make a conservative choice that matches local patterns.

4. **Implement**
   - Keep changes scoped to the request.
   - Update `SPEC.md` before source for new or changed language features.
   - Update `CHANGELOG.md` and relevant `docs/src/` pages when behavior, feature status, or public docs change. There is no `ROADMAP.md` or `TODO.md` — planned work is tracked in GitHub Issues, and deliberately-declined ideas in `NON-GOALS.md`.
   - Add or update focused tests for the changed behavior.

5. **Verify**
   - Run the narrowest useful checks first, then broader checks when the blast radius warrants it.
   - For Rust changes, run at least focused tests or `cargo test` when practical.
   - For docs changes, run `mdbook build` when docs were materially changed.

6. **Review**
   - Re-read the final diff.
   - Check for missed docs/tests/spec/LSP surfaces.
   - Report what changed, what was verified, and any remaining risk.

## Output

Be concrete. Include file references for important changes and name the exact verification commands run.
