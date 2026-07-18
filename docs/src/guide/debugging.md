# Debugging

`keel dap` runs a Keel program under the [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/) (DAP) — the same protocol VS Code, Neovim, and most other editors use to drive a step debugger. It pauses the actual tree-walking interpreter that `keel run` and `keel test` already use; there is no separate "debug build" or execution path.

```bash
keel dap program.keel
```

A DAP client connects over stdio, not a terminal a human reads directly. See [`keel dap`](../cli/dap.md) for the CLI reference, and [`keel test --debug`](../cli/test.md) for debugging a single test.

## What you get

- **Source breakpoints**, honored across every module the program imports.
- **Step over / step in / step out**, with the usual semantics: step over does not descend into a called task, step in does, step out runs until the current call returns.
- **Call stack** (`stackTrace`) reflecting real task/handler calls, including calls made from inside an agent's `@on_start`, `@on_stop`, or `on <event>` handler.
- **Locals**, for the innermost paused frame.
- **Agent State** — a second scope that appears whenever the paused frame is running inside a live agent, showing that agent instance's declared `state` fields with their current values.
- **Expression evaluation** (hover/watch), reusing the program's own expression evaluator against the paused frame's live locals — a hover on `a + b` sees the same value, and the same errors, the paused statement itself would.

```keel
agent Counter {
  state {
    count: int = 0
  }

  task bump(n: int) -> int {
    result = n + 1        # a breakpoint here pauses inside `bump`
    return result
  }

  @on_start {
    self.count = self.bump(self.count)
  }
}

run(Counter)
```

Pausing on the `result = n + 1` line shows a two-frame stack (`bump`, called from `Counter.on_start`), a `Locals` scope with `n`, and an `Agent State` scope with `count` — still `0` at that point, since `self.count` isn't assigned until `bump` returns.

## Known limitations (D0)

This is the first debugger milestone. A few gaps are deliberate scope decisions, not bugs:

<span class="badge badge-soon">Coming soon</span> **Code running inside `Async.spawn` is not debuggable.** Breakpoints inside a spawned task are never hit; the spawned code simply runs to completion in the background while the debugger observes only the caller.

> **Status:** documented gap for D0. Debugging concurrent, spawned work requires tracking multiple simultaneously-paused execution contexts, which is deferred to a later milestone.

<span class="badge badge-soon">Coming soon</span> **Pausing one agent's handler stalls the whole program's event delivery.** Keel's interpreter serializes all agent event handling through one event loop; while that loop is paused inside one agent's handler, no other agent's queued events run either. `delegate`/`send` to a paused agent queues normally and is delivered once you resume — it does not error or drop.

> **Status:** documented gap for D0, tracked against the interpreter's single-event-loop design (see `SPEC.md §9.4`).

**Only the innermost paused frame's variables are inspectable.** `stackTrace` shows the full call chain, but `scopes`/`variables` on any frame other than the topmost one returns no bindings.

**A closure inherits the module of wherever it's called, not where it was declared.** If a closure is passed across a module boundary and called from the far side, breakpoints and stack frames inside it attribute to the caller's module. This only matters for closures shared across files; closures used within a single file are unaffected.

**`pause` (breaking in without a prior breakpoint) is not wired up.** A DAP client's "pause" action is accepted but has no effect; set a breakpoint instead.

**Parameterized tests aren't debuggable.** `keel test --debug` requires `--filter` to match exactly one, non-parameterized (`test "name" for case in cases { ... }`) test.

**Lines and columns are always reported 1-indexed.** The server doesn't read the client's `linesStartAt1`/`columnsStartAt1` capabilities from `initialize` — it assumes both are `true`. VS Code and most DAP clients default to 1-indexed already, so this doesn't affect them; a client that declares 0-indexed lines will see numbers off by one.

## Editor integration

[vscode-keel](https://github.com/keel-lang/vscode-keel) launch-configuration support (so VS Code's "Run and Debug" panel can start `keel dap` automatically) is <span class="badge badge-soon">Coming soon</span>.

> **Status:** not yet shipped. Until then, configure your editor's DAP client manually to run `keel dap <file>` (or `keel test --debug --filter <name> <file>`).
