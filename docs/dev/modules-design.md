# Module System Design

Decision log for the module system and stdlib migration
([#66](https://github.com/keel-lang/keel/issues/66)). SPEC §20 is the
normative surface; this records the *why* and the rejected alternatives.

## Goals

- One source file type. Every `.keel` file is both runnable and importable.
- Stdlib modules and local modules are one concept with one import syntax.
- Predictable namespaces: `use std/file` → `file`; `use "./validation.keel"`
  → `validation`; `as` renames.
- No import side effects; no names dumped into scope by default.
- Explicit dependencies that tooling (and `@tools` capability gating) can
  read off the top of a file.

## Decisions

### Full prelude migration, clean break

All ambient PascalCase namespaces (`Ai`, `File`, `Env`, …) became
`use std/<name>` modules in one release. Pre-1.0 was the cheapest time to
break; a deprecation window and codemod were considered and rejected as
machinery without users. Migration help is in the diagnostics instead:
removed names produce tombstones (`` `File` is not ambient — add `use
std/file` ``), never bare "undefined identifier". No `Io` ambience
exception — one rule beats saving one import line.

### Agent verbs are language, not library

`run`, `stop`, `send`, `delegate`, `broadcast` stay ambient and the
`Agent` namespace is gone. Agents are the language's core abstraction;
importing their verbs would feel like importing `if`. This also dodges the
`std/agent` keyword collision (`agent` is reserved).

### `Uuid` splits into type + module

`Uuid` the *type* stays built in (annotations, casts, value methods).
Constructors move to `std/uuid` (`uuid.v4()`, `uuid.parse()`, the
`uuid.DNS`-style namespace constants). The bare `uuid()` free function was
removed — it would collide with the module binding.

### Implicit main

Top-level statements execute only when their file is the entry file.
Imports load declarations exclusively. Rejected alternatives: erroring on
imported top-level statements (breaks "one file type" — a file couldn't be
both demo script and library) and Python-style run-once-on-import (the
surprising-side-effects model this design exists to avoid). Consequence:
top-level statements share one environment so sequential bindings work.

### Flat global namespace (this release)

The runtime registers every module's declarations in one flat table.
Modules are a *visibility* discipline enforced statically: qualified access
is checked per file, but a name must mean one thing graph-wide. Conflicts
(duplicate decl names across modules, inconsistent import bindings,
decl-vs-import collisions) are compile errors with rename/alias hints.

Why: runtime type identity is nominal **by bare name** (`Value::Struct("Point")`,
enum exhaustiveness, impl dispatch, `Ai.extract` schemas). Per-module type
identity would require qualifying type names through the entire value
system — too invasive for one change. Module-private scoping is the planned
upgrade; the conflict errors keep today's semantics honest.

Corollaries:
- Types/enums/interfaces are accessed via symbol import
  (`use Urgency from ...`), not through the module namespace
  (`models.Urgency` is an error pointing at the import form).
- Type imports cannot be aliased — values carry the declared type name.
- The checker enforces *type visibility*: naming a foreign type in an
  annotation without importing it is an error, even though the runtime
  table is flat.

### Resolution

Relative imports resolve from the importing file's directory; `.keel`
extension required; no search paths. `std/*` resolves only against the
compiled-in catalog — no `KEEL_PATH`, no disk shadowing (predictability and
supply-chain hygiene). Circular imports are a hard error listing the cycle
with an "extract a shared module" hint; cycles could be relaxed later
without breaking anything.

### Tests

`keel test file.keel` runs only the entry file's tests. Transitive test
discovery was rejected: importing a helper should not change your test
count. Test helpers are plain tasks in imported modules — no special
syntax.

### REPL

The REPL pre-imports the whole stdlib (`bind_all_namespaces`); typing `use
std/file` per session would be hostile. Files must import explicitly.

### Reserved / deferred

- `community/...` package paths parse but error: resolution, registry,
  versioning are future work.
- Nested std paths (`std/http/server`) parse but no nested module ships;
  namespaces stay flat.
- `pub` / explicit export lists: every top-level declaration is exported
  until real demand says otherwise.
- Module-level `const` declarations (today: zero-arg task workaround).
- Stdlib-in-Keel: the intended direction is a repo `stdlib/` of `.keel`
  sources embedded via `include_str!` and merged with the Rust intrinsic
  catalog per module, so users cannot tell which half a function lives in.
  Not in this change; tracked as follow-up.

## Known sharp edges

- The `json` symbol-hint string (`format: json`) coexists with the
  `std/json` module name. Importing `std/json` shadows the hint string in
  that file; pass `"json"` as a string literal there. The checker rejects
  `json.parse(...)` without the import with an "add `use std/json`" hint.
- Mock targets (`testing.mock(file.read)`) resolve through aliased
  bindings at runtime, but the static mock-target check recognizes
  canonical names only — prefer unaliased imports in tests.
- `delegate` with a module-qualified handler (`delegate(mod.Agent.handle,
  …)`) is unsupported; symbol-import the agent first.
