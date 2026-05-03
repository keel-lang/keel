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

Error with source span (v0.1.9+):

```
  × Type error
   ╭─[agent.keel:8:5]
 7 │   @on_start {
 8 │     greet(42)
   ·     ────┬────
   ·         ╰── task `greet` takes 1 argument(s), got 0 — expected: name
 9 │   }
   ╰────
```

Every error from `keel check` includes a line:column pointer and an underlined source excerpt. Arity errors list the expected parameter names as a hint.
