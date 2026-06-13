# keel lint

Check a Keel program for style and best-practice issues beyond what `keel check` covers.

```bash
keel lint <file.keel>
keel lint --fix <file.keel>
```

## What it checks

| Rule | Description |
|------|-------------|
| **Unused variable** | A local binding is assigned but never read. Prefix the name with `_` to suppress. |
| **Uncalled task** | A `task` is declared but never called anywhere in the program. |
| **ai.* outside agent** | `ai.classify`, `ai.summarize`, etc. called from a plain task or top-level statement — these calls lack an `@role` / `@model` context and may produce unexpected results. |
| **State written, never read** | An agent `state` field is assigned (`self.x = …`) but `self.x` is never read. |

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | No warnings |
| `1` | One or more warnings found |

## `--fix`

Applies safe, single-line automatic fixes. Currently fixes:

- **Unused variable** — removes the assignment statement entirely.

Other rules emit warnings only; no fix is applied.

```bash
keel lint --fix agent.keel
```

## Suppressing warnings

Prefix a variable name with `_` to tell the linter the value is intentionally unused:

```keel
use std/io

agent Processor {
  @tools [io]
  @on_start {
    _unused = compute_something()   # no warning
    result = compute_other()
    io.show(result)
  }
}
```

## Example output

```
  ⚠ Unused variable
   ╭─[agent.keel:4:5]
 3 │   @on_start {
 4 │     temp = "debug value"
   ·     ──┬─
   ·       ╰── assigned here but never read — prefix with _ to suppress
 5 │     io.show("hello")
   ╰────
  help: remove this line or rename to _temp
```

## Relationship to `keel check`

`keel check` enforces **type correctness** — programs that fail `check` cannot run.  
`keel lint` enforces **style and best practices** — programs that fail `lint` may run, but likely contain dead code or misuse patterns.

Run `check` first; lint warnings are only meaningful on a type-correct program.

```bash
keel check file.keel && keel lint file.keel
```
