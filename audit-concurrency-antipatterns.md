# Rust Audit — Concurrency & Anti-Patterns

> Skills used: `m07-concurrency` (async/concurrency), `m15-anti-pattern` (general Rust anti-patterns)
> Date: 2026-05-16
> Fact-checked: every cited file and line verified by reading source directly

---

## Critical

### 1. `Async.spawn` — orphaned event channel in spawned interpreter

**File:** `src/runtime/namespaces/asynchronous.rs` lines 14–57
**Confidence:** 95%

`Async.spawn` creates a new `Interpreter` via `Interpreter::with_runtime(runtime)`. Inside `state.rs`, `with_runtime` unconditionally calls `tokio::sync::mpsc::unbounded_channel()` and stores the fresh `event_tx` in the spawned interpreter. The main event loop polls `event_rx` on the *original* interpreter — the spawned interpreter's `event_tx` points to a receiver that nobody polls.

**Consequence:** any call inside a spawned closure to `Schedule.every`, `Schedule.after`, `Schedule.at`, `Schedule.cron`, `Agent.send`, `Agent.broadcast`, or `Http.serve` silently enqueues events into a dead channel. Those events are dropped. The spawned task returns `Ok` with no indication that event registration failed.

**Fix:** either pass the parent's `event_tx` clone into the spawned interpreter, or document that spawned closures cannot use scheduler/agent/HTTP primitives and add a runtime guard that raises an error when those namespaces are accessed from a spawned context.

---

## High

### 2. `Memory.*` — blocking filesystem I/O on the async executor

**Files:** `src/runtime/namespaces/memory.rs`, `src/runtime/context.rs` lines 290–338
**Confidence:** 95%

`Memory.remember`, `Memory.recall`, and `Memory.forget` are registered as async closures but call `interp.runtime.persistent_memory.*` synchronously. Those methods dispatch to `NativePersistentMemoryStore::with_locked`, which runs all of the following on the calling thread with no `spawn_blocking` wrapper:

- `std::fs::create_dir_all`
- `std::fs::OpenOptions::new().open` (creates/opens lock file)
- `lock_file.lock_exclusive()` — `fs2::FileExt` flock, **blocks indefinitely on contention**
- `std::fs::read_to_string`
- `std::fs::write` + `std::fs::rename` + `fsync`

The `lock_exclusive` call is the most dangerous: it stalls the tokio executor thread for the duration of any lock contention. Under the single-threaded tokio runtime used by Keel's REPL, this stalls the entire event loop.

**Fix:**
```rust
let result = tokio::task::spawn_blocking(move || {
    runtime.persistent_memory.remember(&key, &value)
}).await??;
```

### 3. `File.*` — blocking filesystem I/O on the async executor

**File:** `src/runtime/namespaces/file.rs` lines 7–49
**Confidence:** 95%

`File.read`, `File.write`, `File.exists`, and `File.list` are registered as async closures but call blocking `std::fs::*` operations directly — no `spawn_blocking`. For large files or slow storage (NFS, network mounts, spinning disk) these block the executor thread.

**Fix:** wrap each handler body in `tokio::task::spawn_blocking`. The `NativeFileSystem` struct holds only a base-path `PathBuf` (`Clone + Send + 'static`), so it moves into the closure cleanly.

---

## Medium

### 4. `Http.*` — `reqwest::Client::new()` created on every HTTP call

**File:** `src/runtime/namespaces/http.rs` ~line 161
**Confidence:** 85%

The internal `http_send` helper constructs a new `reqwest::Client` on every `Http.get / .post / .put / .patch / .delete` call. `reqwest::Client` owns a connection pool; creating one per call discards all pooled connections, forces new TCP+TLS handshakes for every request, and wastes memory.

**Fix:** store a single client in `RuntimeContext` or use a lazy static:
```rust
static HTTP_CLIENT: once_cell::sync::Lazy<reqwest::Client> =
    once_cell::sync::Lazy::new(reqwest::Client::new);
```

### 5. `Stmt::SelfAssign` — TOCTOU double lock

**File:** `src/interpreter/stmt.rs` lines 37–48
**Confidence:** 82%

The readonly check and the subsequent write each acquire the agent `parking_lot::Mutex` separately — there is a window between the two acquisitions:

```rust
let is_readonly = agent.lock().def.state_fields.iter().any(...);
// lock released here — another task could run
agent.lock().state.insert(field.clone(), v);
```

**Fix:** acquire the lock once and do both the check and the insert in a single guard scope:
```rust
let mut agent_guard = agent.lock();
if agent_guard.def.state_fields.iter().any(|f| f.name == *field && f.readonly) {
    return Err(runtime_error(format!("cannot assign to `self.{field}`: field is declared readonly")));
}
agent_guard.state.insert(field.clone(), v);
```

---

## Low / Informational

### 6. `agent.rs` — nested lock ordering (`live_agents` → `instance`)

**File:** `src/runtime/namespaces/agent.rs` lines 94–97
**Confidence:** 80%

The lock nesting is at lines 94–97 (not a wider range). `live_agents` is held while acquiring each per-instance lock: The ordering is consistent today so there is no current deadlock risk, but any future code path that acquires an `instance` lock first and then `live_agents` will deadlock. Snapshot the Arc handles first to release `live_agents` early:

```rust
let instances: Vec<_> = interp.live_agents.lock().values().cloned().collect();
for instance in instances {
    let def = instance.lock().def.clone();
    ...
}
```

### 7. `llm.rs` — `truncate()` allocates `String` in the non-truncating path

**File:** `src/runtime/llm.rs` lines 632–638
**Confidence:** 80%

```rust
fn truncate(s: &str, max: usize) -> String {
    if s.len() > max { format!("{}...", &s[..max]) }
    else { s.to_string() }  // allocates even when s fits
}
```

All call sites pass `max` of 80 or 200, so the else branch only fires when the input is already short — the wasted allocation is small in practice. Still, `Cow<str>` is the correct signature for this kind of helper:
```rust
fn truncate(s: &str, max: usize) -> Cow<str> {
    if s.len() > max { Cow::Owned(format!("{}...", &s[..max])) }
    else { Cow::Borrowed(s) }
}
```

### 8. `human.rs` — O(n²) column deduplication in `show_table`

**File:** `src/runtime/human.rs`
**Confidence:** 80%

Column headers are deduplicated with a linear scan (`columns.contains(key)`), which is O(n²) for wide tables. Use an `IndexSet` (from `indexmap`) for O(1) membership check while preserving insertion order. Low severity given typical column counts.

---

## Summary

| # | Severity | File | Issue |
|---|---|---|---|
| 1 | Critical | `runtime/namespaces/asynchronous.rs` | Spawned interpreter event channel never polled |
| 2 | High | `runtime/namespaces/memory.rs`, `runtime/context.rs` | Blocking fs I/O (+ flock) on async executor |
| 3 | High | `runtime/namespaces/file.rs` | Blocking fs I/O on async executor |
| 4 | Medium | `runtime/namespaces/http.rs` | New `reqwest::Client` per HTTP call |
| 5 | Medium | `interpreter/stmt.rs` | TOCTOU double lock on agent state |
| 6 | Low | `runtime/namespaces/agent.rs` | Nested lock ordering not documented |
| 7 | Low | `runtime/llm.rs` | Unnecessary `String` alloc in `truncate()` |
| 8 | Low | `runtime/human.rs` | O(n²) column deduplication |

**Confirmed clean:** `runtime/namespaces/email.rs` (proper `spawn_blocking`), `runtime/namespaces/io.rs` (proper `spawn_blocking`), `lsp.rs` (guards dropped before every `.await`), `runtime/namespaces/cache.rs` (guards dropped synchronously), all `parking_lot::Mutex` guard lifetimes in the interpreter (no guard held across `.await`).
