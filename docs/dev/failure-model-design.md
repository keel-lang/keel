# Failure Model — Design Scope (issue #38 / TODO K-02)

> Status: **SHIPPED.** Validated against the Hejlsberg design principles via the
> `design-lang` skill. The implementation took the lower-churn variant of §3 —
> see "What actually shipped" below; the original proposal is kept for rationale.

## What actually shipped

During implementation, one frozen assumption turned out to be a choice: **mock
mode returned `CallFailed`**, and the `Err(CallFailed) => Ok(none)` line was what
made `ai.* ?? default` deterministic in tests. Flipping mock to `Ok(None)`
(deterministic *absence*) collapsed the blast radius and let `ai.*` stay nullable:

- **`ai.*` stays `T?`.** `none` strictly means absence — the model returned no
  answer, no model is configured, or mock mode.
- **Real failures throw `AiError`** carrying `reason: str` — `"unavailable"`
  (network / provider unreachable) or `"provider"` (model not mapped / config
  fault). A `??` default can no longer mask an outage.
- **`AiSchemaError` is kept, not folded.** It is a shipped public error type
  referenced across docs/tests/examples; unparseable output already throws it
  with `got`. The three causes the issue names (unavailable / unparseable /
  timeout-as-unavailable) are distinguishable by type + `reason` with no new type
  machinery and no breakage.
- **No `?` operator, no `try`-expression** (see §4) — deferred; tail-position
  `try/catch` plus `??` for absence cover the cases.

This is strictly the §2 rule — *absence is a value (`T?`); failure is an error
(thrown)* — realized with the minimum surface change. Net deltas: `mock → Ok(None)`,
the `Err(CallFailed) => none` paths now throw `AiError{reason}`, `AiError` gains
`reason`, docs/spec/tests/examples updated.

---

## Original proposal (rationale)

> The sections below are the pre-implementation scope. §3's "always non-nullable /
> fold `AiSchemaError`" was superseded by the lower-churn variant above; the rule
> (§2), the per-namespace audit (§1), and the deferral of `?`/`try`-expression
> (§4–5) all shipped as written.

## 1. The problem, restated against the actual code

Issue #38 lists `T?`, `Result[T,E]`, `try/catch`, `fallback:`, `??`, and `!` as
overlapping mechanisms. Audited against the current tree, the list is partly stale:

- **`Result[T,E]`** — not in the language. Explicitly dropped earlier; not reviving it.
  Where the issue says "Result for fallible ops," read it as **thrown typed errors that
  carry a reason**.
- **`fallback:`** — not a real argument. It only ever existed in a since-removed README
  snippet. Dead.
- **`!`** (`PostfixOp::NullAssert`) — live. Asserts non-null on a `T?`; raises if `none`.
- **`??` / `?.` / `when`** — live. Absence handling on `T?`.
- **`try/catch` + `raise`** — live. `Error` is a flat catch-all union (§2.10); `raise`
  produces `UserRaised`.

So the live model is already two clean channels:

| Channel | Mechanism | Handlers |
|---|---|---|
| **Absence** — succeeded, nothing there | `T?` | `??`, `?.`, `!`, `when` |
| **Failure** — operation could not complete | thrown typed error (`<: Error`) | `try`/`catch`, `raise` |

The incoherence is **not** language-wide. The rest of the fallible stdlib already obeys
the rule:

| Namespace | Happy-path return | On failure | Obeys rule? |
|---|---|---|---|
| `file.read` | `str` (non-nullable) | throws `FileError` | ✅ |
| `http.get` | `HttpResponse` (non-nullable) | throws `HttpError` | ✅ |
| `json.parse` | dynamic (non-nullable) | throws `JsonError` | ✅ |
| `csv.parse_records` | structured | throws `CsvError` | ✅ |
| `email.fetch` / `.send` | structured | throws `EmailError` | ✅ |
| **`ai.classify` / `.extract` / `.decide`** | **`T?` (nullable)** | **`none` on network/mock/timeout, *throws* `AiSchemaError` on bad output** | ❌ |
| **`ai.summarize` / `.draft` / `.translate` / `.prompt`** | **`str?`** | same split | ❌ |

**`ai.*` is the lone outlier.** It is the only fallible namespace that (a) returns
nullable for the happy path and (b) splits failure across two mechanisms. This is exactly
the bug the email-triage fix in `[Unreleased]` papered over: `ai.classify(...) ?? default`
rescues the `none` (network/mock) path but *not* the thrown `AiSchemaError` (bad-output)
path, so the same line behaves differently depending on which way the model fails.

## 2. The rule (canonical statement to land in SPEC §11 / §8)

> **Nullable `T?` means legitimate absence: the operation succeeded and there is simply no
> value** (missing map key, empty `.first()`, optional field, failed parse that has a
> defined "not a number" answer). Handle with `??`, `?.`, `!`, or `when`.
>
> **A thrown typed error means the operation could not complete** (network down, model
> unavailable, timeout, unparseable output, malformed input, quota exceeded). Every such
> error is a variant of `Error` and **carries a reason**. Handle with `try`/`catch`.
>
> **Programmer faults throw too** (out-of-bounds index, strict-boundary type mismatch,
> readonly write, capability violation, `raise`). Same `try`/`catch` machinery; semantically
> "fix the code," not "handle at runtime."

One sentence: **absence is a value (`T?`); failure is an error (thrown).** `??` is never the
failure-handling tool — `try`/`catch` is.

## 3. The change: bring `ai.*` into line (the breaking decision)

`ai.*` stops returning `T?` for the failure case and **always throws** a typed `AiError` on
any failure, carrying a machine-readable reason.

```keel
# BEFORE — failure split across two mechanisms; ?? rescues only one
urgency = ai.classify(email.body, as: Urgency) ?? Urgency.medium

# AFTER — classify returns Urgency or throws AiError; one mechanism handles all failures
urgency = try {
  ai.classify(email.body, as: Urgency)
} catch e: AiError {
  Urgency.medium
}
```

This rewrites the language's most-shown line. It is defensible (0.x alpha; the ROADMAP
states breaking changes are expected; the issue explicitly asks for it), but it is **the**
decision of this work and must be made deliberately, not as an implementation detail.

### 3a. `AiError` shape — reason enum, not a type hierarchy

The issue wants "reasons, not just `none`." We get reasons with **zero new type
machinery** — keep the flat `Error` union, fold the existing `AiSchemaError` into a single
`AiError` carrying a discriminated reason:

```keel
type AiErrorReason = unavailable | timeout | schema_mismatch | provider_error
# AiError { message: str, reason: AiErrorReason, got: str? }
#   - reason: why it failed (drives programmatic handling + tracing)
#   - got:    the raw model output when reason == schema_mismatch (else none)
```

`catch e: AiError` now catches **every** AI failure; `e.reason` distinguishes causes
without a class hierarchy. `AiSchemaError` is **removed** as a separate variant (its
`got` field is preserved on `AiError`). Rejected alternative: a nested sub-union
(`AiError = AiUnavailable | AiTimeout | …`) — that needs error-type-hierarchy support in
the catch matcher, which the flat union deliberately avoids. Reach for it only if a
concrete need appears.

## 4. Ergonomics: keep the common case terse

The most common AI pattern is "classify, default on any failure." Empirically, today:

- `x = try { … } catch … { … }` as a **sub-expression** → **parse error** (not valid).
- `try { … } catch … { … }` in **tail / implicit-return position** → **works**, yields the
  branch value.

So a single-statement task body stays a one-liner; only mid-block uses need restructuring.

**Decision (recommended): ship the core fix without new syntax.** Tail-position `try/catch`
already covers the dominant case. If real friction shows up, add `try`/`catch` as a general
**expression** (`x = try EXPR catch DEFAULT`) as a *separate, additive* enhancement — it
does not block #38 and should not be bundled into it.

## 5. Explicitly out of scope

- **`Result[T,E]`** — dropped, not reviving.
- **Separating `none` from the unit type** (TODO K-03) — marked **skip**; breaks every
  program for a purity gain. Not touched here.
- **The full AI primitives contract** (issue #42 / K-10: retries, token accounting,
  determinism, trace events) — #38 fixes only the *failure* facet. The `AiError`/reason
  shape defined here is the foundation #42 builds on.
- **Compile-time capability checking** (#39) — separate issue.

## 6. Implementation surface (once approved)

1. **Catalog** (`keel-catalog/specs/ai.rs`): change `ai.*` result types from `NullableStr`/
   `Nullable(…)` to their non-nullable forms (`Str`, `Enum(as:)`, `as:`-type).
2. **Runtime** (`keel-runtime` ai namespace): every failure path (no model configured,
   network error, timeout, unparseable / no-variant-match output) raises `AiError` with the
   correct `reason`; remove all `none`-on-failure returns. Add `AiErrorReason` enum + fold
   `AiSchemaError` into `AiError`.
3. **Error registry** (`SPEC §2.10` + runtime error kinds): replace `AiError`+`AiSchemaError`
   with the single reason-carrying `AiError`; update `RuntimeErrorKind`.
4. **Checker**: `ai.*` call-site result types become non-nullable; `?? default` on an `ai.*`
   call now type-errors (right side unreachable / left non-nullable) — surface a migration
   hint pointing at `try/catch`.
5. **Docs**: `SPEC §11.1`, §8.5, §2.10; `docs/src/guide/ai-primitives.md`, error-handling
   guide; rewrite the triage example + README hero to the `try/catch AiError` form.
6. **Tests + examples**: update `examples/email_agent.keel`, `examples/trading_bot`, and the
   AI/error integration tests; add a test asserting every `ai.*` failure path throws
   `AiError` with the expected `reason` (mock mode → `unavailable`).
7. **CHANGELOG** (breaking — migration table), **ROADMAP**, **TODO.md** (mark K-02 done).

## 7. Open decisions for sign-off

- **D1.** Approve the breaking `ai.*`-always-throws change (§3)? *(recommended: yes)*
- **D2.** `AiError` with a `reason` enum, folding away `AiSchemaError` (§3a)? *(recommended:
  yes)* — or keep `AiSchemaError` as a distinct variant for back-compat.
- **D3.** Ship core fix only and defer the `try`-expression sugar (§4)? *(recommended: yes)*
