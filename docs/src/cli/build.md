# keel build

<span class="badge badge-soon">Coming soon</span>

> Status: `keel build` is the future entry point for the native/LLVM AOT backend (see the project's `designs/llvm-compilation.md`). The CLI does not produce a binary yet — `--emit=kir` prints the mid-level IR for the subset of the language that lowers so far, as a preview of the pipeline under construction; every other form of `keel build` errors. Behind the scenes, that same subset now compiles, links, and runs as a native binary end to end (proven by a dedicated conformance suite comparing its output against the interpreter's), but this isn't reachable from the CLI yet — only from the internal `keel-codegen`/`keel-rt-ffi` crates. Use `keel run` to execute programs and `keel check` to type-check without running.

## `--emit=kir`

Type-checks a single-file program and prints its lowered mid-level IR (KIR) instead of compiling. Everything through M2 of `designs/llvm-compilation.md` lowers today: `int`/`float`/`bool`/`str` literals, arithmetic and comparison, `if`/`else`, `while`, `for`-over-ranges (and `for`-over-`list[T]`), `let`/assignment, task declarations (including default parameter values), direct calls between tasks, calls into std namespaces (`io.show`, `log.*`, …), `return` (including an implicit tail-expression-as-return), named structs (literals, field access, spread-update), simple enums matched via `when` (both as a statement and as a value-producing expression in `let`/`return` position), `list[T]`/`map[str, V]`/`set[T]` containers, nullable types (`T?`, `??`, `?.`), string interpolation, and `raise`/`try`/`catch`. Anything else — agents, lambdas, generics, a `when`-expression in a call-argument or other nested position, map/set mutation — is rejected by name rather than silently dropped or approximated.

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
