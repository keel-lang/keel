# Testing

> **Alpha (v0.1).** Breaking changes expected.

Keel test blocks make agent-facing code deterministic without replacing your production program.

```keel
use std/testing

type Severity = low | medium | critical

task classify(text: str) -> Severity {
  Ai.classify(text, as: Severity) ?? Severity.low
}

test "mocked classify returns critical" {
  testing.mock(Ai.classify).returns(Severity.critical)
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
test "summary" {
  setup {
    expected: str = "short"
    actual: str = Ai.summarize("long article") ?? ""
  }

  assert actual == expected
}
```

## Mocks

Mocks replace prelude namespace methods inside one test:

```keel
use std/testing

test "summary fallback" {
  testing.mock(Ai.summarize).returns("short")
  assert Ai.summarize("long article") == "short"
}
```

For enum classification, return the enum variant directly:

```keel
test "classification" {
  testing.mock(Ai.classify).returns(Severity.critical)
  assert classify("payment outage") == Severity.critical
}
```

Mocks are scoped to a single test. If two tests mock the same method differently, each test sees only its own value.

Repeat a mock target to return a sequence of values. Once the sequence is exhausted, the last value repeats:

```keel
test "summaries" {
  testing.mock(Ai.summarize).returns("first")
  testing.mock(Ai.summarize).returns("second")

  assert Ai.summarize("a") == "first"
  assert Ai.summarize("b") == "second"
  assert Ai.summarize("c") == "second"
  assert Ai.summarize.called
  assert Ai.summarize.call_count == 3
  assert Ai.summarize.called_with("a")
}
```

Mocked methods expose test-local metadata:

```keel
test "draft" {
  testing.mock(Ai.draft).returns("Thanks")

  reply = Ai.draft("response to Ada", tone: "friendly") ?? ""

  assert reply == "Thanks"
  assert Ai.draft.called
  assert Ai.draft.call_count == 1
  assert Ai.draft.called_with("response to Ada", tone: "friendly")
}
```

`called_with(...)` returns `true` when any recorded mock call matches the supplied evaluated arguments. Positional arguments match from the start, and named arguments match by name.

`@tools` capability checks still apply. A mock changes the method result; it does not grant an agent access to a namespace that its `@tools` block disallows.

## Contextual Syntax

`test`, `setup`, and `assert` are not reserved keywords. They are recognized only in their testing positions, so existing identifiers with those names remain valid elsewhere.
