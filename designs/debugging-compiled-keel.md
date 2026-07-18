# Design: Debugging Compiled Keel Programs

Status: proposal (pre-implementation)
Companion to: `designs/llvm-compilation.md` — assumes its pipeline (KIR → LLVM via
inkwell, result-style error convention, `libkeel_rt` runtime, thread-per-handler
scheduler). Milestone references (M1–M5) are that document's.

Goal: a compiled Keel binary must be debuggable **at the Keel level** — Keel file/line
breakpoints, Keel variable names and values, Keel stack traces on errors — never
requiring the user to understand LLVM IR or the runtime's Rust internals.

---

## 0. Code organization

Where the pieces in this document live, relative to the compilation plan's layout:

```
crates/
  keel-codegen/src/debug.rs        # DIBuilder wiring: DICompileUnit/DIFile/DISubprogram,
                                   #   DILocation per instruction, DILocalVariable, DIType map
  keel-kir/src/span_table.rs       # SpanId interning + line/col index (shared: codegen,
                                   #   span registry, DAP — the ONE line-index impl)
  keel-rt-ffi/src/
    errors.rs                      # KeelError -> miette report (same renderer as interpreter)
    spans.rs                       # runtime reader of the baked .keel_spans registry
    trace.rs                       # shadow call stack + event breadcrumbs, emitted as
                                   #   tracer-hook events (§3.3 / §5)
  keel-dap/                        # NEW crate — Track A interpreter-backed DAP adapter
    src/
      main_loop.rs                 # DAP protocol over stdio (launch, breakpoints, threads)
      hooks.rs                     # DebugHook impl: pause/step/breakpoint state machine
      variables.rs                 # Environment + agent-state -> DAP variable tree
      eval.rs                      # paused-frame expression evaluation via eval_expr
  keel-runtime/src/interpreter/
    debug_hook.rs                  # NEW: DebugHook trait + statement-boundary call sites
                                   #   (no-op default; the only interpreter change)
tools/
  lldb/keel_formatters.py          # pretty printers + frame recognizer (§2), embedded
                                   #   into the keel binary for `keel debug` to install
src/cli/                           # `keel dap` and `keel debug` subcommands
```

`keel-dap` depends only on `keel-runtime` + `keel-syntax` (no LLVM), so Track A ships
independently of the build backend and works for every current `keel run` user.

---

## 1. Source-level debug info (DWARF)

### 1.1 Emission

Use inkwell's `DebugInfoBuilder` (LLVM `DIBuilder`) during codegen:

- **`DICompileUnit`** per program; producer string `keel <version>`. Language tag:
  DWARF has no Keel code — emit `DW_LANG_C11` as the compatibility baseline so lldb/gdb
  apply no language-specific expression semantics, and set `DW_AT_producer` to identify
  Keel tooling. (Registering a `DW_LANG_` vendor constant is cosmetic; not worth it.)
- **`DIFile`** per `.keel` source file in the `ModuleGraph`.
- **`DISubprogram`** per compiled task, agent handler, impl method, and lambda.
  `DW_AT_name` carries the human name (`triage`, `Inbox.on email`, `Inbox.check`);
  the linkage name carries the mangled symbol (§1.3).
- **`DILocation`** on every KIR instruction, derived from AST spans. Prerequisite:
  spans today are byte offsets (`Span { start, end }`) — KIR lowering must attach a
  **line/column table** per file (one pass over the source; the LSP's
  `position.rs` already has this logic to reuse).
- **`DILocalVariable` + `llvm.dbg.declare`** for every named binding and parameter.
  Keel's "assignment always declares, shadowing allowed" semantics map naturally:
  each shadowing binding is a fresh `DILocalVariable` in a fresh lexical block scope,
  so `frame variable` shows the binding visible at the current line.
- **Type info (`DIType`)** mapped from KIR types:

| Keel type | DWARF encoding |
|---|---|
| `int` | base type, `DW_ATE_signed`, 64-bit |
| `float`, `duration` | base type, `DW_ATE_float`, 64-bit (name `duration` kept distinct) |
| `bool` | `DW_ATE_boolean` |
| `str`, containers, `dynamic`, handles | pointer to named opaque struct (`keel.str`, `keel.list`, …) — real structure recovered by pretty printers (§2) |
| named struct | `DW_TAG_structure_type` with true member layout |
| tuple | `DW_TAG_structure_type` with members `_0`, `_1`, … |
| simple enum | `DW_TAG_enumeration_type` (variant names visible) |
| rich enum | struct of `{tag, payload*}`; printer renders the active variant |
| `T?` | scalar pair `{present, value}` or nullable pointer; printer renders `none`/value |

### 1.2 Optimization levels

- Default `keel build`: `-O0`-equivalent codegen + full DWARF. Debuggability is the
  default, matching the rest of Keel's DX posture.
- `keel build --release`: O2 + line tables only (`DW_AT_stmt_list` kept, variables
  best-effort). Never strip line info entirely — runtime error reports depend on it
  (§3). `--release --strip` for the truly minimal binary.
- DWARF lives in the binary (Linux) / `.dSYM` via `dsymutil` (macOS); the linker
  driver in `keel-codegen` handles the platform difference.

### 1.3 Symbol mangling

Deterministic, demangle-friendly scheme:

```
_K<module>$<container>$<name>[$<disambiguator>]
  _Kmain$triage              # top-level task
  _Kmain$Inbox$on_email      # agent handler
  _Kmain$Score$describe      # impl method
  _Kmain$triage$lambda_0     # lambda, indexed within parent
```

Monomorphized generics append an instantiation hash: `_Kmain$first$i64`. A tiny
`keel-demangle` helper (exposed as `keel debug demangle` and as a Rust fn for the
runtime's own reporter) converts these back to display names. DWARF `DW_AT_name`
already carries the pretty name, so debuggers rarely need the demangler.

---

## 2. Debugger UX: lldb/gdb out of the box, made pleasant

With §1 in place, stock lldb works immediately: `b file.keel:12`, `step`, `bt`,
`frame variable`. Two additions make it Keel-native:

1. **Pretty printers** — a Python formatter script (lldb type summaries/synthetic
   children; gdb equivalents later) for `keel.str`, `keel.list`, `keel.map`,
   `keel.set`, `keel.box` (dynamic — shows tag + payload), rich enums, `T?`.
   Shipped inside the `keel` binary and installed via `command script import`.
2. **`keel debug <binary>` launcher** — the `rust-lldb` pattern: finds lldb, loads the
   formatters, sets `KEEL_LLM`/env passthrough, launches. Zero-setup path:
   `keel build app.keel && keel debug ./app`.

Runtime-internal frames (tokio, scheduler, shims) are noise in `bt`. The formatter
script registers a frame-recognizer that tags `libkeel_rt` frames as hidden-by-default
(lldb `frame recognizer` API), so backtraces show Keel frames first.

---

## 3. Mapping compiled errors back to Keel source

The interpreter's typed runtime errors (`RuntimeErrorKind` + miette spans) set the UX
bar. Compiled programs must match it without a debugger attached.

### 3.1 Span registry baked into the binary

KIR lowering assigns every potentially-raising site a `u32 span_id`. The compiler
emits a compact static table (`.keel_spans` section / linked const array):

```
span_id → (file_id, line, col)          file_id → path
```

plus, in non-release builds, the source text itself (or its path + content hash, with
graceful degradation when the file moved). `KeelError` (the payload of the result
convention) carries `kind`, message fields, and `span_id`.

### 3.2 Error reports

An uncaught error reaching the runtime boundary renders exactly like today's
interpreter output: miette-style report with the source snippet resolved from the span
registry — `keel-rt-ffi` links the same reporting code the interpreter uses. Same
message text, same `RuntimeErrorKind` taxonomy; the conformance harness (M0) diffs
error output too, which enforces this.

### 3.3 Keel-level stack traces

Native unwinding is unreliable here (result-convention calls get tail-call-optimized;
handler stacks are runtime-managed), so don't depend on it. Instead:

- **Shadow call stack**: each compiled task prologue pushes `(function_id, span_id of
  current statement — updated at call sites)` onto a per-context stack; epilogue pops.
  Cost: two stores per call, acceptable at `-O0`/default. In `--release`, error
  *propagation* (`?`, implicit re-raise) appends frames to the `KeelError` itself as it
  bubbles — trace-on-error costs nothing on the happy path.
- Rendered trace shows Keel names via the span/function registries:

```
Error: FileNotFound: no such file "inbox.json"
   ┌─ examples/triage.keel:14:12   file.read(path)
  at read_inbox        examples/triage.keel:14
  at Inbox.on start    examples/triage.keel:31
  event: start → agent Inbox      (dispatched from run(Inbox), triage.keel:40)
```

- **Cross-agent/async causality**: mailbox events and `async.spawn` already flow
  through the runtime — attach the sender's `(function_id, span_id)` breadcrumb to
  each `Event`/spawned task, and append an `event:` line per hop (capped, e.g. last 8
  hops). This gives compiled Keel *better* traces than raw native code, because the
  runtime sees the logical structure.

---

## 4. Interactive debugging strategy (REPL / step-debugger)

Two tracks, cheapest-first; both converge on DAP so editor UX is uniform.

### 4.1 Track A — interpreter-backed DAP debugger (`keel run --debug`) [first]

The tree-walking interpreter already executes the AST with spans in hand; a debugger
over it is mostly plumbing, and it works **today**, before any codegen exists:

- New `keel dap` mode implementing the Debug Adapter Protocol over stdio (crate:
  `dap` or hand-rolled — the protocol is small). The interpreter gains a `DebugHook`
  trait called at statement boundaries (`exec_block` loop) and on env mutation:
  breakpoint check, pause, step-over/in/out via call-depth tracking, variable
  enumeration from `Environment` + agent `state`, expression evaluation by reusing
  `eval_expr` in the paused frame's env.
- Async caveat: hooks are `async` and pause via a watch channel, so a paused agent
  handler suspends exactly like a suspending shim — other agents keep running,
  matching SPEC §9.4 semantics. (A "freeze world" toggle can come later.)
- The existing REPL stays as-is (interpreter). Inside a DAP pause, the debug-console
  evaluate request effectively *is* a REPL scoped to the paused frame.

### 4.2 Track B — native DAP for compiled binaries [after M5]

Do not write a native debugger. Wrap the existing machinery:

- **`lldb-dap`** (ships with LLVM) speaks DAP natively and consumes our DWARF; the
  launcher work is a VS Code launch configuration plus auto-loading the pretty
  printers from §2. This is the entire phase-1 native story.
- Later polish, only if usage demands it: a thin `keel dap --native` proxy in front of
  lldb-dap that demangles names, hides runtime frames, and renders agent mailbox state
  (via a runtime introspection hook: a debugger-callable `keel_rt_debug_snapshot()`
  returning JSON of live agents/state/queues).

### 4.3 Editor integration

`vscode-keel` (separate repo) gains two launch configurations: `keel: run (debug)` →
Track A adapter; `keel: launch compiled` → lldb-dap with formatter preamble. The LSP
is untouched — DAP and LSP are separate channels. Breakpoint validity (breakable
lines) can later be served from the HIR via the LSP for gutter accuracy.

---

## 5. Integration with existing Keel tooling

| Existing surface | Interaction |
|---|---|
| `KEEL_TRACE` / `--trace` (LLM call narration) | Unchanged — same runtime code runs in compiled binaries |
| Tracer hook (SPEC §22, runtime service 6) | The shadow-stack/event-breadcrumb machinery (§3.3) is implemented *as* tracer events, so `log.*` and traces share one spine |
| `keel test` + `testing.mock` | Stays on the interpreter (per compilation design §1.3); Track A debugger works under `keel test --debug` for free since it's the same interpreter |
| `keel check` | Unchanged gate: `keel build` refuses on check errors, so codegen never sees `Ty::Error` |
| miette diagnostics | Reused verbatim for compiled runtime error reports (§3.2) |
| REPL | Interpreter-only, permanently |

---

## 6. Milestones (aligned to the compilation plan)

**D0 (parallel to M0–M1, no LLVM dependency):** Track A interpreter DAP debugger —
breakpoints, step, variables, paused-frame eval; VS Code launch config. Immediately
useful to every current Keel user, and it pins down the debugging UX the native path
must match.

**D1 (with M1–M2):** Line-table DWARF (`DILocation` everywhere) + span registry +
compiled error reports matching interpreter output byte-for-byte in the conformance
harness. Shadow stack + Keel stack traces on uncaught errors.

**D2 (with M3–M4):** Full variable DWARF (`DILocalVariable`, `DIType` for all KIR
types), pretty printers, `keel debug` launcher, frame recognizer, mangling +
demangler. Event-breadcrumb causal traces across agents.

**D3 (with M5):** lldb-dap launch config in vscode-keel, `dsymutil`/`--release`
debug-level matrix, docs (`docs/src/guide/debugging.md`, `docs/src/cli/`).

---

## 7. Risks / open questions

| Item | Notes |
|---|---|
| lldb formatter maintenance across lldb versions | Keep printers dependency-free Python; CI smoke test: script-load + one summary per type against a fixture binary |
| Shadow-stack overhead in default builds | Measure at M2; if >5% on compute-heavy examples, switch default to propagation-time trace building (release strategy) everywhere |
| Byte-offset spans vs line/col | One shared line-index utility (reuse `lsp/position.rs` logic) used by KIR, span registry, and DAP — do not implement it three times |
| Paused-handler deadlock (Track A) | A breakpoint inside a handler that another agent `delegate`s to and awaits: document that delegation to a paused agent queues (matches SPEC §9.4); add watchdog notice in the DAP client after N seconds |
| Debug info for monomorphized generics | Each instantiation is its own `DISubprogram` with the pretty name `first[int]`; verify lldb breakpoint-by-name matches all instantiations (`-n` regex fallback in launcher) |
| Windows | Out of scope with the compilation plan's platform set (PDB/CodeView is a separate project) |
