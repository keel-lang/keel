# keel build

<span class="badge badge-soon">Coming soon</span>

> Status: `keel build` is the future entry point for the native/LLVM AOT backend (see the project's `designs/llvm-compilation.md`). It does not produce a binary yet. `--emit=kir` prints the mid-level IR for the scalar subset of the language, as a preview of the pipeline under construction; every other form of `keel build` errors. Use `keel run` to execute programs and `keel check` to type-check without running.

## `--emit=kir`

Type-checks a single-file program and prints its lowered mid-level IR (KIR) instead of compiling. Only the scalar subset lowers today: `int`/`float`/`bool`/`str` literals, arithmetic and comparison, `if`/`else`, `while`, `let`/assignment, task declarations with scalar parameters, direct calls between tasks, and `return`. Anything else — agents, structs, enums, containers, string interpolation, `for`, `when`, lambdas, generics — is rejected by name rather than silently dropped or approximated.

```keel
task add(a: int, b: int) -> int {
  return a + b
}

x = add(2, 3)
```

```bash
keel build add.keel --emit=kir
```

```
fn add(a: int, b: int) -> int {
  return (a + b)
}

fn <toplevel>() -> none {
  let x: int = add(2, 3)
}
```

Real programs use namespaces, agents, and structured data — none of which lower yet, so pointing `--emit=kir` at `examples/hello_world.keel` (or almost any other shipped example) currently errors by design:

```bash
keel build examples/hello_world.keel --emit=kir
```

```
Error:   × KIR lowering error at 396..406: `use declaration` is not supported by the
  │ scalar-subset KIR lowering (M0)
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

`keel build` is present in the CLI because the verb is reserved for the native backend, but no codegen exists yet — only the KIR lowering step above.
