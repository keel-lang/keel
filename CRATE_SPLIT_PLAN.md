# Crate split plan

Workspace decomposition of the single `keel-lang` crate to improve incremental/test
build times and enforce acyclic layering as the language grows.

## Baseline (cold, measured 2026-06-15)

| metric | cold | warm + 1-file edit |
|---|---|---|
| `cargo check` | 19.1s | 1.8s |
| `cargo test --no-run` | 30.0s | 6.9s (link-dominated) |

Heaviest compile unit in the whole graph: **`keel-lang` at 15.6s** — a single unit,
recompiled on every source change, 2× the next-heaviest dependency. Splitting it is the lever.

These are the gate numbers. Re-measure after each phase; only proceed to the next phase
if the previous one moved the needle.

## Target crate graph

```
crates/
  keel-syntax     lexer, ast, parser, formatter, lint   (AST-only — verified zero outward edges
                  except lint -> LintWarning, which moves in)   deps: logos, chumsky, miette
     ^
  keel-catalog    builtin signatures, namespace metadata, typed-error helpers   (leaf, no exec)
     ^            breaks the one real cycle: types/prelude.rs -> runtime::namespaces::catalog
  keel-compiler   hir, types, modules, diagnostics       -> keel-syntax, keel-catalog
     ^
  keel-runtime    interpreter, runtime, pipeline, session-exec  -> keel-compiler, keel-catalog
                  └─ DEFERRED, gated on timings: peel out keel-stdlib-io
                     (rusqlite/reqwest/axum/lettre/imap) so interpreter edits stop
                     relinking heavy deps. Seam is already trait-based (RuntimeContext is
                     all Arc<dyn ...>; rusqlite confined to namespaces/db.rs).
keel-lang         published facade: re-exports ast/session/diagnostics; binary depends on it
```

Internal crates are `publish = false` path deps; `keel-lang` stays the only published crate.
Shared dep versions via `[workspace.dependencies]`.

## Phase order

0. **[done]** Cold `--timings` baseline.
1. **[done]** **keel-syntax** — lexer/ast/parser/formatter/lint extracted; re-exported from
   `keel-lang` under original paths (zero inbound churn, zero visibility widening). Validated:

   | gate | baseline | after |
   |---|---|---|
   | front-end-only test build (parser edit) | inside the 6.9s monolith | **0.78s** |
   | full workspace test build (parser edit) | 6.9s | **2.6s** |
   | interpreter edit rebuilds keel-syntax? | n/a (one crate) | **no** |

   All keel-lang tests green (666 + 455 + 147 + …). One fix needed: the formatter idempotency
   test's `project_root()` now ascends two levels to reach repo-root `examples/`. The 7
   `mdbook-keel-catalog` failures are pre-existing (fail identically on clean HEAD), unrelated.
2. **[done]** **keel-catalog** — moved the descriptor types (`builtins`), the 23 per-namespace
   `SPEC` tables, `catalog()`, and the `@tools` capability metadata into a zero-dep leaf crate.
   Done in two commits: 2a scaffold (move `builtins`, re-export), 2b bulk (SPEC tables +
   rewire the 4 edges: `types::prelude` ×2, `types::checker::expr`, `interpreter::call`).
   **Gate met:** `rg 'crate::(runtime|interpreter)' src/types` is empty — the `types → runtime`
   cycle is broken — and `keel-catalog` builds standalone with no path-deps. The
   `spec_matches_installed_methods` test stays in runtime (checks SPEC vs `namespace()` impls);
   `catalog_has_no_duplicate_entries` moved to keel-catalog with the data.
   Note: typed-error helpers (`make_typed_report`) were **not** in the cycle and stayed in runtime.
3. **[done]** **keel-compiler** — hir + types (checker + diagnostics) + `ide` + `modules`, moved
   as one atomic unit (the four are mutually cyclic; the cycles are intra-crate, which is fine).
   Cut the lone `modules → session` edge by inlining `parse_source` via keel-syntax. Re-export
   trick kept all 107 internal `crate::ast`/`crate::builtins` refs unchanged; only 8 `pub(crate)`
   items needed widening to `pub` (one E0446 cascade: `ModuleMembers`). **Gate met:** keel-compiler
   builds standalone depending only on keel-syntax + keel-catalog + miette + logos — so the
   types→runtime decoupling is now *compiler-enforced*. Editing the type checker rebuilds in
   ~1.1s and recompiles **zero** heavy deps.
4. **[done]** **keel-runtime** — interpreter + runtime moved as one unit (deeply mutually coupled);
   owns all heavy I/O deps (tokio, reqwest, rusqlite, axum, lettre, imap, native-tls). Only 4
   consumers (session, pipeline, repl, cli) and **zero** outward edges → **zero visibility
   widening** (the engine's public surface was already `pub`). Re-export stays `pub(crate)`, so the
   external embedding API is byte-for-byte unchanged. Gate met: builds standalone; full suite green;
   `keel run` smoke test interprets correctly across the boundary.
5. **[done]** **re-measured** (see below). Optional future work: peel `keel-stdlib-io`,
   `keel-lsp`, `keel-cli` — deferred until timings justify.

## Final crate graph

```
keel-syntax → keel-catalog → keel-compiler → keel-runtime → keel-lang (facade + binary)
```

Each arrow is compiler-enforced. keel-catalog is a zero-dep leaf; all heavy I/O deps live only in
keel-runtime.

## Results vs baseline

| iteration loop | baseline | after split |
|---|---|---|
| edit parser → its test build | inside the 6.9s monolith | **0.78s** |
| edit type checker → its test build | 6.9s (heavy relink) | **1.21s** |
| heavy deps (rusqlite/reqwest/axum) rebuilt on a parser edit | always (monolith) | **0** |

The single 15.6s `keel-lang` compile unit is gone, replaced by five units that build in parallel and
recompile independently. Front-end and middle-end iteration is insulated from the heavy runtime deps.

Independent cheap wins (any time, no refactor): feature-gate `rusqlite` `bundled`; `reqwest` -> `rustls`.

## Phase 1 detail — keel-syntax

**Why low-risk:** measured outward dependency of the whole bundle on the rest of the crate is
a single edge — `lint.rs` uses `crate::diagnostics::LintWarning`, and `LintWarning` is itself
defined in the 18-line `src/diagnostics.rs` (which otherwise only re-exports). It moves into
keel-syntax. Required external crates: `logos`, `chumsky`, `miette`. No serde derives in ast.

**Churn-minimizing trick:** after moving the modules into `keel-syntax`, re-export them from
`keel-lang`'s `lib.rs` under their original names:

```rust
pub use keel_syntax::{ast, lexer, parser, formatter, lint};
```

This makes every existing `crate::ast::…`, `crate::lexer::Span`, etc. resolve unchanged — so the
**40 inbound files do not need edits**. Churn collapses to lib.rs + Cargo.toml + the moved files.

### Steps
1. Add `crates/keel-syntax/Cargo.toml` (deps from `[workspace.dependencies]`: logos, chumsky, miette).
2. `git mv` `src/lexer.rs`, `src/ast/`, `src/parser/`, `src/formatter.rs`, `src/lint.rs`
   → `crates/keel-syntax/src/`. Add `crates/keel-syntax/src/lib.rs` declaring the modules.
3. Move the `LintWarning` struct from `src/diagnostics.rs` into keel-syntax (e.g. `lint` module);
   re-export it so keel-lang's `diagnostics` facade can re-export `keel_syntax::LintWarning`.
4. Root `Cargo.toml`: add `crates/*` to `[workspace] members`; add `[workspace.dependencies]`;
   add `keel-syntax = { path = "crates/keel-syntax" }` to keel-lang deps.
5. `lib.rs`: replace `mod lexer/ast/parser/formatter/lint` with `pub use keel_syntax::{...}`.
   Update `src/diagnostics.rs` to source `Span`/`LintWarning` from `keel_syntax`.
6. `cargo build` + fix fallout: any `pub(crate)` item in the moved modules that an *outside*
   file relied on must become `pub` (compiler will list them). Then `cargo fmt`, `clippy -D warnings`,
   `cargo test`.

### Known risks
- `pub(crate)` → `pub` widening for items consumed across the new crate boundary (compiler-driven).
- Span identity: `Span` re-exported from keel-syntax must be the *same* type used everywhere — the
  re-export guarantees this.
- chumsky/logos macro hygiene across the crate move (expected clean; no cross-crate macro use).

### Validation gate
Re-run: touch `crates/keel-syntax/src/parser/expr.rs`, time `cargo test --no-run`. Expect the
keel-syntax unit to rebuild in isolation and front-end test iteration to drop below the 6.9s baseline.
