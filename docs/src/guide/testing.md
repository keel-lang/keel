# Testing

Keel test blocks make agent-facing code deterministic without replacing your production program.

```keel
use std/ai
use std/testing

type Severity = low | medium | critical

task classify(text: str) -> Severity {
  ai.classify(text, as: Severity) ?? Severity.low
}

test "mocked classify returns critical" {
  testing.mock(ai.classify).returns(Severity.critical)
  assert classify("payment outage") == Severity.critical
}
```

Run tests with:

```bash
keel test triage.keel
```

Run one group by name with:

```bash
keel test triage.keel --filter classify
```

List tests without running them with:

```bash
keel test triage.keel --list
```

## Test Blocks

Test blocks live at the top level beside `type`, `task`, and `agent` declarations:

```keel
test "name" {
  assert true
}
```

`keel test` type-checks each tested file, registers declarations, and runs each test block. Top-level statements such as `run(MyAgent)` are skipped unless a test calls them explicitly. `keel run` ignores test blocks.

Pass a directory to recursively run `.keel` files with test blocks. Files without test blocks are skipped.

`--filter <text>` runs only tests whose names contain `text`. Matching is case-sensitive, and the command fails if no tests match.

`--list` prints matching test names without running them. It can be combined with `--filter`. If a file has no test blocks, `keel test` prints `0 tests found` and exits successfully.

`--fail-fast` stops after the first failing test. `--quiet` suppresses passing test result lines while still printing failures and the final summary.

Each executed test line includes elapsed time after the test name. The final summary includes total suite time, and failures print the source location when available before returning a failing exit status.

## Assertions

```keel
assert expr
```

The expression must be `bool`. If it evaluates to `false`, the test fails.

```keel
test "math" {
  assert 2 + 2 == 4
}
```

Use a custom failure message with a second `str` expression:

```keel
test "math" {
  assert 2 + 2 == 5, "expected arithmetic to balance"
}
```

## Parameterized Tests

Use `for name in cases` after the test name to run one test case for each item in a list:

```keel
test "validate status" for case in [
  { score: 95, expected: Status.valid },
  { score: 150, expected: Status.needs_review }
] {
  assert validate_score(case.score) == case.expected
}
```

The runner prints each case with an index, such as `validate status [0]`. The case binding is available in `setup` and in the test body.

## Setup

Use `setup` to prepare values that the test body can assert against:

```keel
use std/ai

test "summary" {
  setup {
    expected: str = "short"
    actual: str = ai.summarize("long article") ?? ""
  }

  assert actual == expected
}
```

## Mocks

Mocks replace stdlib module methods inside one test:

```keel
use std/ai
use std/testing

test "summary fallback" {
  testing.mock(ai.summarize).returns("short")
  assert ai.summarize("long article") == "short"
}
```

For enum classification, return the enum variant directly:

```keel
use std/ai
use std/testing

test "classification" {
  testing.mock(ai.classify).returns(Severity.critical)
  assert classify("payment outage") == Severity.critical
}
```

Mocks are scoped to a single test. If two tests mock the same method differently, each test sees only its own value.

Repeat a mock target to return a sequence of values. Once the sequence is exhausted, the last value repeats:

```keel
use std/ai
use std/testing

test "summaries" {
  testing.mock(ai.summarize).returns("first")
  testing.mock(ai.summarize).returns("second")

  assert ai.summarize("a") == "first"
  assert ai.summarize("b") == "second"
  assert ai.summarize("c") == "second"
  assert ai.summarize.called
  assert ai.summarize.call_count == 3
  assert ai.summarize.called_with("a")
}
```

Mocked methods expose test-local metadata:

```keel
use std/ai
use std/testing

test "draft" {
  testing.mock(ai.draft).returns("Thanks")

  reply = ai.draft("response to Ada", tone: "friendly") ?? ""

  assert reply == "Thanks"
  assert ai.draft.called
  assert ai.draft.call_count == 1
  assert ai.draft.called_with("response to Ada", tone: "friendly")
}
```

`called_with(...)` returns `true` when any recorded mock call matches the supplied evaluated arguments. Positional arguments match from the start, and named arguments match by name.

`@tools` capability checks still apply. A mock changes the method result; it does not grant an agent access to a namespace that its `@tools` block disallows.

## Contextual Syntax

`test`, `setup`, and `assert` are not reserved keywords. They are recognized only in their testing positions, so existing identifiers with those names remain valid elsewhere.
