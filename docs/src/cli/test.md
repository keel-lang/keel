# keel test

Run the `test` blocks in a Keel program or directory.

```bash
keel test <path>
```

`keel test` parses and type-checks each tested file, then executes only top-level `test "name" { ... }` blocks. Normal top-level program statements such as `run(A)` are not executed by the test command.

When `<path>` is a directory, Keel recursively discovers `.keel` files with test blocks and skips files without tests.

Tests are discovered **per file**: `keel test file.keel` runs only that file's `test` blocks. Modules imported with `use` contribute their declarations — test helpers are ordinary tasks — but never their own tests. Point `keel test` at each file (or the directory) to run a whole project's suite.

To run a subset by name:

```bash
keel test <path> --filter classify
```

The filter is a case-sensitive substring match against the test name. If no tests match, the command fails.

To list tests without running them:

```bash
keel test <path> --list
```

`--list` can be combined with `--filter`. If no tests are found while listing, the command prints `0 tests found`.

To stop after the first failing test:

```bash
keel test <path> --fail-fast
```

To print only failing test lines and the final summary:

```bash
keel test <path> --quiet
```

To step through a single test under a debugger instead of running it directly:

```bash
keel test <path> --debug --filter "mocked classify returns critical"
```

`--debug` requires `--filter` to match exactly one test, and speaks the Debug Adapter Protocol over stdio instead of printing pass/fail output — see [`keel dap`](./dap.md) for the protocol details and current limitations.

## Example

```keel
use std/ai
use std/testing

type Severity = low | medium | critical

task classify(text: str) -> Severity {
  ai.classify(text, as: Severity) ?? Severity.low
}

test "mocked classify returns critical" {
  testing.mock(ai.classify).returns(Severity.critical)
  assert classify("payment outage") == Severity.critical, "expected critical"
}
```

```bash
keel test triage.keel
```

Success:

```text
PASS mocked classify returns critical (2ms)
1 test passed in 2ms
```

Failure:

```text
FAIL mocked classify returns critical (2ms)
  assertion failed
0 tests passed, 1 test failed in 2ms
0 passed, 1 failed in triage.keel
```

## Behavior

- `use std/testing` brings the `testing` namespace into scope for test helpers.
- `testing.mock(Namespace.method).returns(value)` overrides that namespace method for the current test only.
- Repeating the same mock target returns values in order, then repeats the final value.
- Mocked methods expose test-local metadata: `Namespace.method.called`, `Namespace.method.call_count`, and `Namespace.method.called_with(...)`.
- `setup { ... }` runs before assertions in the same test and can bind values for the test body.
- `test "name" for case in cases { ... }` runs one indexed case per item in the `cases` list.
- `assert expr` requires `expr` to type-check as `bool`; `false` fails the current test.
- `assert expr, "message"` uses a custom failure message. The message expression must type-check as `str`.
- Failed tests print the source location of the failing statement when available.
- Each test runs with its own mock set, so mocks do not leak between tests.
- Passing a directory recursively runs `.keel` files with test blocks; files without test blocks are skipped.
- `--filter <text>` runs only tests whose names contain `text`.
- `--list` prints matching test names without running them.
- `--fail-fast` stops after the first failing test.
- `--quiet` suppresses passing test result lines while still printing failures and the final summary.
- `--debug` runs the single test matched by `--filter` under the Debug Adapter Protocol instead of executing it directly; it fails fast if `--filter` matches zero or more than one test.
- A file with no test blocks prints `0 tests found` and exits successfully.
- Test result lines include elapsed time after the test name, and the final summary includes total suite time.
- `keel run` ignores test blocks.
- `test`, `setup`, and `assert` are contextual syntax words, not reserved keywords.
