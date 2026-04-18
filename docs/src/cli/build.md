# keel build <span class="badge badge-soon">Coming soon</span>

> **Status:** `keel build` is deferred post-v0.1. The tree-walking interpreter handles every alpha workload, so shipping a bytecode compiler is punted until a concrete motivator (LLVM/WASM backend, embeddable runtime) lands. Tracked in [ROADMAP](../../ROADMAP.md).

Compile a Keel program to bytecode.

```bash
keel build <file.keel>
```

Produces a `.keelc` file (JSON-serialized bytecode) that can be cached for faster loading.

## Example

```bash
keel build examples/minimal.keel
# ✓ Compiled examples/minimal.keel → examples/minimal.keelc (28 ops, 2 functions)
```

## Bytecode format

The `.keelc` file contains:
- **main chunk** — top-level agent code
- **function chunks** — compiled tasks
- **string pool** — deduplicated string constants
- **register count** — per function/chunk

The bytecode is a register-based instruction set with 40+ opcodes covering arithmetic, comparison, control flow, function calls, data structures, and human interaction.
