# keel dap

Run a Keel program under the [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/) (DAP), so an editor can set breakpoints, step, and inspect variables in a live interpreter session.

```bash
keel dap <file>
```

`keel dap` does not print program output to a terminal for a human to read. It speaks DAP over stdio — newline-free, `Content-Length`-framed JSON messages — to whichever client launched it. Point your editor's debug configuration at the `keel` binary with this subcommand; you will not typically run it by hand.

Editor integration for [vscode-keel](https://github.com/keel-lang/vscode-keel) — a `launch.json` config that lets VS Code's "Run and Debug" panel invoke `keel dap` automatically — is <span class="badge badge-soon">Coming soon</span>.

> **Status:** not yet shipped. Until then, configure your DAP client to run `keel dap <file>` (or `keel test --debug --filter <name> <file>`) directly.

## Behavior

- Parses and type-checks `<file>` first — the same gate `keel run` uses. A type error is reported and the session ends before any DAP handshake happens.
- Source breakpoints are honored across every module the program imports, not just the entry file.
- `next` (step over), `stepIn`, and `stepOut` behave like a conventional debugger: `next` does not descend into a called task; `stepIn` does; `stepOut` runs until the current call returns.
- The `scopes` response includes a `Locals` scope always, and an `Agent State` scope whenever the paused frame is running inside a live agent (an `@on_start`/`@on_stop` block or an `on <event>` handler) — its variables are that agent instance's declared `state` fields.
- `evaluate` (hover/watch expressions) reuses the program's own expression evaluator against the paused frame's live locals, so it produces the same values and errors the paused statement would.
- Only the innermost paused frame's `Locals`/`Agent State` are populated; outer frames appear in `stackTrace` but their `variables` requests return no bindings. See `docs/src/guide/debugging.md` for the full list of D0 gaps.

## `keel test --debug`

To debug a single test instead of a whole program, use `keel test`'s `--debug` flag together with `--filter`:

```bash
keel test --debug --filter "square works" suite.keel
```

`--filter` must match exactly one test — `--debug` fails fast with an error if it matches zero or more than one, since a DAP session can only debug a single running program. Parameterized tests (`test "name" for case in cases { ... }`) are not yet debuggable under `--debug`.

## Example

```keel
task add(a: int, b: int) -> int {
  sum = a + b     # breakpoint here
  return sum
}

x = 1
y = 2
z = add(x, y)
```

```bash
keel dap add.keel
```

A DAP client that sets a breakpoint on the `sum = a + b` line, runs `configurationDone`, and waits will receive a `stopped` event once the program reaches that statement; `stackTrace`/`scopes`/`variables` then show the `add` frame with `a = 1` and `b = 2` in scope.
