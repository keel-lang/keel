# Post-v0.1.17 Code Audit

Covers commits `7586956..6645ab7` (nine commits, three from external contributors).
Every finding below was verified by reading the referenced source lines.

---

## Summary table

| # | Category | Severity | File | Line(s) | Status |
|---|----------|----------|------|---------|--------|
| B1 | Bug | **Critical** | `src/interpreter/binary.rs` | 20 | **Fixed** |
| B2 | Bug | **Critical** | `src/interpreter/expr.rs` | 283, 288, 309 | **Fixed** |
| B3 | Bug | **Critical** | `src/runtime/namespaces/asynchronous.rs` | 27–35 | **Fixed** |
| B4 | Bug | Medium | `src/pipeline.rs` | 276–283 | **Fixed** |
| B5 | Bug | Minor | `src/interpreter/entry.rs` | 130 | **Fixed** |
| S1 | Spec gap | High | `SPEC.md` § grammar + `src/interpreter/call.rs` | 1283 / 40 | **Fixed** |
| S2 | Spec gap | Medium | `SPEC.md` §3.2 + `src/runtime/namespaces/agent.rs` | 315 / 23 | **Fixed** |
| S3 | Spec gap | Medium | `SPEC.md` §4.2 + `src/interpreter/call.rs` | 403–406 / 143–174 | **Fixed** |
| D1 | Doc error | Low | `src/lsp.rs` | 224 | **Fixed** |
| P1 | Pending | — | `src/interpreter/entry.rs`, `state.rs` | — | Uncommitted (pre-existing) |

---

## Bugs

### B1 — Modulo by zero panics at runtime

**File:** `src/interpreter/binary.rs:20`
**Severity:** Critical

`Div` has an explicit zero-guard that returns a `RuntimeError`. `Mod` has no guard and panics with a Rust `attempt to calculate the remainder with a divisor of zero` when `b == 0`:

```rust
// binary.rs lines 14–20
(Div, Value::Integer(a), Value::Integer(b)) => {
    if *b == 0 {
        return Err(runtime_error("Division by zero"));
    }
    Ok(Value::Integer(a / b))
}
(Mod, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a % b)),  // ← no guard
```

**Fix:**
```rust
(Mod, Value::Integer(a), Value::Integer(b)) => {
    if *b == 0 {
        return Err(runtime_error("Modulo by zero"));
    }
    Ok(Value::Integer(a % b))
}
```

---

### B2 — `return` inside an if-expression or when-expression is silently swallowed

**File:** `src/interpreter/expr.rs:283, 288, 309`
**Severity:** Critical

The parser's `inner_block` uses `inner_stmt` which includes `return_stmt` (parser.rs lines 329 / 951). This makes `return` syntactically valid inside an `Expr::IfExpr` or `Expr::WhenExpr` body. The evaluator matches the resulting `StmtOutcome::Return(v)` identically to `StmtOutcome::Value(v)` and resolves to `Ok(v)`, absorbing the return signal:

```rust
// expr.rs ~283
StmtOutcome::Value(v) | StmtOutcome::Return(v) => Ok(v),  // Return consumed, not propagated
```

`Stmt::If` and `Stmt::For` in `stmt.rs` handle `StmtOutcome::Return` correctly by propagating it. Only the expression-position `IfExpr` and `WhenExpr` variants swallow it.

**Impact:** Code like this silently falls through:
```keel
task compute() -> int {
  result = if condition { return -1 } else { 0 }
  # execution continues here even though `return -1` was reached
  42
}
```

**Fix:** `eval_expr` returns `Result<Value>`, so `Return` cannot be propagated directly. Options:
- Introduce a sentinel `Value::Return(Box<Value>)` variant that only `call_task` and `call_closure` unwrap (analogous to how Python raises `StopIteration` for generators).
- Refactor `eval_expr` to return a richer outcome type, like `ExprOutcome { Value(Value), Return(Value) }`.

The second option is cleaner but touches more call sites. Either requires a follow-up test in `tests/integration/language.rs`.

---

### B3 — `Async.spawn` runs the closure in a bare interpreter with no user symbols

**File:** `src/runtime/namespaces/asynchronous.rs:27–35`
**Severity:** Critical

`Async.spawn` creates a fresh `Interpreter::with_runtime(runtime)` for the spawned Tokio task. Only `current_agent` and `program_name` are copied from the parent:

```rust
let handle = tokio::spawn(async move {
    let mut _local_interp = crate::interpreter::Interpreter::with_runtime(runtime);
    _local_interp.current_agent = current_agent;
    _local_interp.program_name = program_name;
    // globals, agents, enum_types, struct_types, closures are all empty
    _local_interp
        .call_closure(&params_clone, &body_clone, vec![])
        .await
        .map_err(|err| err.to_string())
});
```

`Interpreter::with_runtime` installs only the prelude namespaces. Any closure body that references a user-defined task, enum type, struct type, or another closure will fail at runtime with an "Undefined: `name`" error. That error is only surfaced later when `Async.join_all` is called.

**Impact:** `Async.spawn` is currently limited to closures that call only prelude namespaces (`Io`, `Ai`, `Http`, etc.). Any spawn of user code silently fails.

**Fix:** Snapshot the interpreter's symbol state before spawning:
```rust
let globals = interp.globals.clone();
let agents  = interp.agents.clone();
// etc.

let handle = tokio::spawn(async move {
    let mut local = Interpreter::with_runtime(runtime);
    local.globals      = globals;
    local.agents       = agents;
    local.current_agent = current_agent;
    local.program_name  = program_name;
    local.call_closure(&params_clone, &body_clone, vec![]).await
         .map_err(|e| e.to_string())
});
```

Because `Value::Task` and `Value::Closure` are `Arc`-backed or clone-cheap, this does not deep-copy the program. Note that a spawned task mutating `current_agent` state would still race — the design implication is that spawned closures should be stateless or communicate only through `Agent.send` / `Memory.*`.

---

### B4 — `apply_lint_fixes` can panic on overlapping fixable-warning spans

**File:** `src/pipeline.rs:276–283`
**Severity:** Medium

After sorting ranges in reverse start-position order, `ranges.dedup()` only removes adjacent *identical* byte pairs. Two fixable warnings whose line spans overlap (but are not byte-for-byte identical) both survive into the loop. The first `replace_range` shortens the string; the second tries to index the now-shorter string at the original byte offsets, causing a panic:

```rust
ranges.sort_by(|a, b| b.0.cmp(&a.0));
ranges.dedup();                         // ← removes identical pairs only

let mut result = source.to_string();
for (start, end) in ranges {
    result.replace_range(start..end, "");  // ← panics if first pass shifted indices
}
```

The current lint rules (unused-let, unused-task, ai-outside-agent, state-write-never-read) tend to produce non-overlapping spans, but the code makes no guarantee. Future rules will hit this.

**Fix:** Merge overlapping ranges after sorting:
```rust
ranges.sort_by(|a, b| b.0.cmp(&a.0));
let mut merged: Vec<(usize, usize)> = vec![];
for (s, e) in ranges {
    if let Some(last) = merged.last_mut() {
        if s < last.1 { last.1 = last.1.max(e); continue; }
    }
    merged.push((s, e));
}
let mut result = source.to_string();
for (start, end) in merged {
    result.replace_range(start..end, "");
}
```

---

### B5 — HTTP handler errors are silently discarded

**File:** `src/interpreter/entry.rs:130`
**Severity:** Minor

In the `Event::FireClosureWithArgs` path (used by `Http.serve` route handlers), runtime errors from the user's closure are swallowed:

```rust
let resp_val = result.unwrap_or(Value::None);
```

When a handler panics or returns an interpreter error, the caller sees a 500 response with no body explaining what went wrong. The error is gone.

**Fix:**
```rust
let resp_val = result.unwrap_or_else(|err| {
    eprintln!("[keel] HTTP handler error: {err}");
    Value::None
});
```

---

## Spec inconsistencies

### S1 — Named arguments silently fall back to positional for user-defined tasks

**Spec claim:** `SPEC.md` §18 grammar, line 1283:
```
Arg <- (IDENT ":")? Expr   # named args supported
```

**Reality:** `src/interpreter/call.rs:40`:
```rust
// Bind params by position (named args not wired for user tasks yet).
for (i, p) in decl.params.iter().enumerate() {
    let v = args.get(i).map(|a| a.value.clone()).unwrap_or(Value::None);
```

Named args are parsed and the `name` field is populated in `CallArgValue`, but `call_task` ignores it and binds strictly by position. A call like `send_email(to: addr, subject: title)` silently reorders arguments positionally if the parameter order differs. No error is raised.

Note: prelude namespace methods use `positional()` / named-arg helpers from `src/runtime/namespace.rs` and work correctly — this gap applies only to user-defined tasks.

**Fix (short-term):** Add a sentence to SPEC.md §18 noting that named-arg syntax is parsed but positional-only for user tasks in v0.1, with a `<span class="badge badge-soon">Coming soon</span>` marker.

**Fix (long-term):** In `call_task`, match by `CallArgValue::name` when present:
```rust
for p in &decl.params {
    let v = args.iter()
        .find(|a| a.name.as_deref() == Some(&p.name))
        .or_else(|| args.get(positional_idx))
        .map(|a| a.value.clone())
        .unwrap_or(Value::None);
    bind_value(&p.name, v, &mut env)?;
    positional_idx += 1;
}
```

---

### S2 — `Agent.send` missing from the SPEC prelude table

**Spec claim:** `SPEC.md:315`:
```
| `Agent` | Agent lifecycle | `run`, `stop`, `delegate`, `broadcast` ... |
```

**Reality:** `src/runtime/namespaces/agent.rs:23–43` implements `Agent.send(target, message)`. It is the primary async mailbox API, referenced in `SPEC.md:651` itself and in `SPEC.md:364` in an example. It is simply absent from the prelude table, making the table misleading.

**Fix:** Add `send(target, message)` to the `Agent` row in the prelude table.

---

### S3 — `@limits` fields `max_cost_per_request` and `require_confirmation` are unimplemented

**Spec claim:** `SPEC.md:400–407`:
```keel
@limits {
    timeout: 30s
    max_tokens: 4096
    max_cost_per_request: 0.50
    require_confirmation: [Email.send, Db.exec]
}
```

**Reality:** `src/interpreter/call.rs:133–174` (`agent_limits`) reads only `timeout`, `max_tokens`, and `max_cost`. The keys `max_cost_per_request` and `require_confirmation` fall through the `_ => {}` arm and are silently ignored. Users writing them get no error and no enforcement.

**Fix:** Either implement both fields or remove them from SPEC.md §4.2 with a `<span class="badge badge-soon">Coming soon</span>` note. The spec also names the field `max_cost` in some places and `max_cost_per_request` in others — pick one.

---

## Documentation errors

### D1 — `"now"` listed as a reserved keyword in LSP completions

**File:** `src/lsp.rs:224`

`"now"` appears in the LSP keyword completion list. Per SPEC.md §10 and AGENTS.md §"Reserved Keywords (v0.1)", `now` is **not** a reserved keyword. It is a prelude identifier exposed as `Time.now()`. IDE users will see it suggested as a completion alongside actual keywords (`agent`, `task`, `return`, etc.), which is misleading.

**Fix:** Remove `"now"` from the keywords vector at `lsp.rs:224`.

---

## Pending uncommitted changes

### P1 — Method renames in `entry.rs` and `state.rs` are unstaged

`git diff HEAD` shows two method renames that are not committed:

- `state.rs`: `fire_closure` → `call_scheduled_closure`
- `state.rs`: `dispatch_event` → `call_event_handler`
- `entry.rs`: call sites updated to match

The changes are correct and consistent. They should be committed as a standalone naming-cleanup commit before the next feature branch.

---

## Observations (no action needed)

- **Module split quality (7cde39a):** The interpreter split into `agent`, `binary`, `binding`, `call`, `decl`, `entry`, `expr`, `methods`, `state`, `stmt` is clean. No production `unwrap()` calls were found in any of these modules.
- **Test coverage (33d2c72, ecf0a55, 51a95a2):** The new integration test files (`tests/integration/`) are well-structured. The `util.rs` helper avoids duplication. Coverage for namespace tests (http, cache, async, memory, schedule) uses unit-level namespace instantiation correctly — the `unwrap()` calls there are all inside `#[cfg(test)]` blocks.
- **`multi_agent_inbox.keel` rewrite:** The example was restructured from a three-agent system to a single-agent + top-level-tasks pattern. `docs/src/examples/multi-agent.md` was updated to match. The doc correctly explains when to use agents vs top-level tasks. No inconsistency.
- **`rustfmt.toml` addition:** Five new formatting rules added. All are standard and non-controversial.

---

## Recommended fix order

1. **B1** (modulo zero) — one-liner, ship immediately.
2. **D1** (LSP "now" keyword) — one-liner, ship immediately.
3. **S2** (Agent.send in spec) — doc-only, ship immediately.
4. **S3** (unimplemented @limits fields) — remove from spec or badge-soon, ship before next release.
5. **P1** (uncommitted renames) — commit before next feature branch.
6. **B5** (HTTP error logging) — one-liner, low risk.
7. **B4** (overlapping lint ranges) — fix before adding new lint rules.
8. **S1** (named args gap) — add badge-soon note to spec short-term; implement in a dedicated PR.
9. **B2** (return in if-expr) — requires design work; open a tracking issue.
10. **B3** (Async.spawn barren interpreter) — requires design work; open a tracking issue.
