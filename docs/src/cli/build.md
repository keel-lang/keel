# keel build <span class="badge badge-soon">Coming soon</span>

> **Status:** `keel build` is deferred post-v0.1. The tree-walking interpreter handles every alpha workload, so shipping a bytecode compiler is punted until a concrete motivator (LLVM/WASM backend, embeddable runtime) lands. Tracked in [ROADMAP](../../ROADMAP.md).

`keel build` is present in the CLI but intentionally returns an error today. Use `keel run` to execute programs and `keel check` to type-check without running.

```bash
keel build <file.keel>
```

When the compiler is implemented, this command is expected to produce a `.keelc` bytecode artifact. No `.keelc` output is produced in v0.1.

## Example

```bash
keel build examples/hello_world.keel
# error: keel build is deferred post-v0.1 — use `keel run`
```

## Bytecode format

The planned `.keelc` file is expected to contain:
- **main chunk** — top-level agent code
- **function chunks** — compiled tasks
- **string pool** — deduplicated string constants
- **register count** — per function/chunk

The VM and bytecode format live in stub modules until the compiler is resumed.
