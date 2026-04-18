# keel check

> **Alpha (v0.1).** Breaking changes expected.

Type-check a Keel program without executing it.

```bash
keel check <file.keel>
```

## What it checks

- **Syntax** — valid Keel grammar
- **Types** — type inference and compatibility
- **Exhaustiveness** — `when` matches cover all enum variants
- **Arguments** — task call parameter count and types
- **Nullable safety** — `T?` vs `T` tracking
- **Scope** — `self` only inside agents, undefined variables

## Example output

Success:

```
✓ examples/email_agent.keel is valid
```

Error:

```
  × Type error
   ╭─[agent.keel:8:5]
 7 │   every 1.day {
 8 │     greet(42)
   ·     ────┬────
   ·         ╰── Argument 'name' of task 'greet': expected str, got int
 9 │   }
   ╰────
```
