# keel build

<span class="badge badge-soon">Coming soon</span>

> Status: `keel build` is the future entry point for the native/LLVM AOT backend (see the project's `designs/llvm-compilation.md`). The CLI does not produce a binary yet — `--emit=kir` prints the mid-level IR for the subset of the language that lowers so far, as a preview of the pipeline under construction; every other form of `keel build` errors. Behind the scenes, that same subset now compiles, links, and runs as a native binary end to end (proven by a dedicated conformance suite comparing its output against the interpreter's), but this isn't reachable from the CLI yet — only from the internal `keel-codegen`/`keel-rt-ffi` crates. Use `keel run` to execute programs and `keel check` to type-check without running.

## `--emit=kir`

Type-checks a single-file program and prints its lowered mid-level IR (KIR) instead of compiling. Everything through M2 of `designs/llvm-compilation.md` lowers today: `int`/`float`/`bool`/`str` literals, arithmetic and comparison, `if`/`else`, `while`, `for`-over-ranges (and `for`-over-`list[T]`), `let`/assignment, task declarations (including default parameter values, whose expressions may themselves call another task), direct calls between tasks, calls into std namespaces (`io.show`, `log.*`, …), `return` (including an implicit tail-expression-as-return), named structs (literals, field access, spread-update), simple enums matched via `when` (both as a statement and as a value-producing expression — in `let`/`return` position and in nested positions such as a call argument or a binary-op operand, over any subject expression rather than only a bare variable), `if` used as a value-producing expression in those same positions (including `else if` chains), `list[T]`/`map[str, V]`/`set[T]` containers (including `map.insert` and `set.add`/`.contains`/`.len`), nullable types (`T?`, `??`, `?.`), string interpolation, and `raise`/`try`/`catch`. Anything else — agents, lambdas, generics, or a sub-expression that has to be evaluated ahead of its enclosing statement sitting in a position that isn't evaluated exactly once (a `while` condition, an `and`/`or` right operand, a `??` fallback, a parameter default), or a parameter default that *omits* another call's defaulted argument (pass it explicitly) — is rejected by name rather than silently dropped or approximated. Four constructs need that hoisting: a `when`-expression, an `if`-expression, a `when` whose subject is not a bare variable (the subject is bound to a temporary so it is evaluated once rather than once per arm comparison), and a struct literal or spread-update whose fields are written out of the target type's declared order *and* whose field expressions have side effects, since those are evaluated in source order (see [Types](../guide/types.md#structs)) and so must be bound to temporaries before the struct is assembled.

Two `if`-expression forms are rejected rather than compiled. An `if` used as a value with no `else` branch (`x = if c { 1 }`) has no value to produce on the false path — `SPEC.md` §8.1 calls this a compile error, and `keel check` now rejects it too (along with any branch that produces no value), so this diagnostic is normally reached only when lowering runs without a prior check. And an *unannotated* `if`-expression whose `then` branch exits via `return` (`x = if c { return 0 } else { 1 }`) has no tail value to infer the result type from; annotate it (`x: int = …`) or use it where the surrounding syntax already pins a type. Both restrictions match the `when`-expression's.

```keel
use std/io

task sum_upto(n: int) -> int {
  total = 0
  for i in 1..n {
    total += i
  }
  return total
}

io.show(sum_upto(5))
```

```bash
keel build sum.keel --emit=kir
```

```
fn sum_upto(n: int) -> int {
  let total: int = 0
  for i in 1..n {
    total = (total + i)
  }
  return total
}

fn <toplevel>() -> none {
  io.show(sum_upto(5))
}
```

Real programs use agents, which don't lower yet, so pointing `--emit=kir` at `examples/hello_world.keel` (or almost any other shipped example that declares an agent) currently errors by design:

```bash
keel build examples/hello_world.keel --emit=kir
```

```
Error:   × KIR lowering error at 419..1039: `agent declaration` is not supported by
  │ the scalar-subset KIR lowering (M0)
```

## Without `--emit`

```bash
keel build examples/hello_world.keel
```

```
Error:   × native codegen not yet implemented — `keel build` is the future LLVM AOT
  │ backend (designs/llvm-compilation.md); pass --emit=kir to inspect the mid-
  │ level IR, or use `keel run` to execute examples/hello_world.keel
```

`keel build` is present in the CLI because the verb is reserved for the native backend. Codegen exists now (see the status note above) but is not yet wired to this command — only the KIR lowering step above is reachable today.
