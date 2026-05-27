# keel check

> **Alpha (v0.1).** Breaking changes expected.

Type-check a Keel program without executing it.

```bash
keel check <file.keel>
keel check --strict <file.keel>
```

## What it checks

- **Syntax** — valid Keel grammar
- **Types** — type inference and compatibility
- **Exhaustiveness** — `when` matches cover all enum variants
- **Arguments** — task call parameter count and types
- **Nullable safety** — `T?` vs `T` tracking
- **Scope** — `self` only inside agents, undefined variables

## `--strict` mode

The checker's default mode accepts bindings whose type cannot be inferred (for example, the result of `Json.parse` or `Ai.extract` without an explicit cast). It silently allows these rather than reporting an error.

`--strict` changes that: any binding whose inferred type is `Unknown` becomes a type error. Use it when you want higher confidence that the checker is actually verifying your code.

There are two kinds of unresolved types, and `--strict` treats them differently:

| Situation | Normal mode | `--strict` |
|---|---|---|
| `data: dynamic = Json.parse(...)` | ✓ silent | ✓ **silent** — programmer chose `dynamic` deliberately |
| `data = Json.parse(...)` (unannotated) | ✓ silent | ✗ **error** — no annotation, type cannot be inferred |
| `data = Ai.extract(...)` (unannotated) | ✓ silent | ✗ **error** |

In other words: an explicit `dynamic` annotation is always accepted. `--strict` only fires when the checker cannot infer _any_ type, not when the programmer deliberately chose `dynamic`.

```bash
# Passes in normal mode, fails in strict
agent A {
  @on_start {
    data = Json.parse(raw_input)   # unannotated — strict rejects this
    Io.show("{data}")
  }
}
```

Fix with a `dynamic` annotation (accept the dynamic type) or a concrete cast:

```keel
# Option 1: annotate as dynamic — clean even in --strict
data: dynamic = Json.parse(raw_input)

# Option 2: narrow to a concrete type immediately
type Payload { key: str, value: str }

data = Json.parse(raw_input) as Payload
```

## Example output

Success:

```
✓ examples/email_agent.keel is valid
```

Error with source span:

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
