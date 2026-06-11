# Modules & Imports

> **Alpha (v0.1).** Breaking changes expected.

Keel has one source file type: `.keel`. Every file is both **runnable** (an
entrypoint when you `keel run` or `keel test` it directly) and **importable**
(a module when another file `use`s it). Stdlib modules and your own files are
the same concept, imported the same way:

```keel
use std/io
use std/file
use "./validation.keel"

task load_config() -> str {
  file.read("config.json")
}

ok = validation.email("ada@example.com")
io.show("valid: {ok}")
```

## Import forms

| Form | Binds | Example access |
|---|---|---|
| `use std/file` | `file` | `file.read(...)` |
| `use std/file as f` | `f` | `f.read(...)` |
| `use "./validation.keel"` | `validation` (file stem) | `validation.email(...)` |
| `use "./validation.keel" as v` | `v` | `v.email(...)` |
| `use email, helper as h from "./validation.keel"` | `email`, `h` | `email(...)`, `h(...)` |
| `use parse from std/json` | `parse` | `parse(...)` |

The namespace is predictable: a std import binds the module's name, a file
import binds the file stem, and `as` overrides either. Imports never dump
names into scope by default — `use X from ...` is the explicit opt-in for
unqualified names.

Relative paths resolve from the importing file's directory and must end in
`.keel`. `std/*` modules ship inside the `keel` binary and never touch the
filesystem — a local `./std/` directory cannot shadow them.

## What a module exports

Every top-level declaration — `task`, `type`, `agent`, `interface`,
`extern` — is exported under the module's namespace. There is no `pub`
keyword yet; if it's declared at the top level, it's importable.

- **Tasks** are called qualified (`validation.email(...)`) or imported by
  name (`use email from "./validation.keel"`).
- **Agents** work with the built-in agent verbs:
  `run(watchers.Watcher)`, `send(watchers.Watcher, msg)`. Symbol imports
  work too: `use Watcher from "./watchers.keel"` then `run(Watcher)`.
- **Types and enums** are imported by name —
  `use Urgency from "./models.keel"` — and then used exactly like a local
  type, including `when` exhaustiveness. Types keep their declared identity,
  so they cannot be renamed with `as`.
- **Interfaces and impls** travel with the module: importing a module
  activates its `impl` blocks program-wide.

Modules gate **entry points**; values carry their methods everywhere. A task
that receives a `DbConnection` can call `.query()` without importing
`std/db` — only `db.connect(...)` needs the import. The same goes for
`dt.format(...)`, `id.to_str()`, and every other value method.

## Top-level statements: the implicit main

Top-level statements are the file's entrypoint. They run **only when the
file is executed directly** — never when it is imported:

```keel
# validation.keel
use std/io

task email(s: str) -> bool {
  s.contains("@") and s.contains(".")
}

# Implicit main: runs on `keel run validation.keel`, ignored on import.
sample = "ada@example.com"
io.show("email({sample}) = {email(sample)}")
```

This means a module can carry its own demo or smoke check with zero
boilerplate, and importing a file can never surprise you with side effects.

## Tests and modules

`keel test file.keel` runs **only that file's** `test` blocks. Imported
modules contribute their declarations — including test helpers, which are
ordinary tasks — but never their tests. `keel test <dir>` runs each file's
own tests. See [Testing](./testing.md).

A dedicated test file is just a module that imports what it tests:

```keel
# validation_test.keel
use "./validation.keel"
use subject_priority from "./validation.keel"

test "qualified module call" {
  assert validation.email("ada@example.com")
}

test "urgent subjects rank higher" {
  assert subject_priority("urgent: prod is down") == 2
}
```

The full version lives in
[`examples/inbox_modules/`](https://github.com/keel-lang/keel/tree/main/examples/inbox_modules).

## One global namespace (v0.1)

The runtime registers every module's declarations in one flat global table,
so a name must mean the same thing across the whole program:

- Two modules may not declare the same top-level name.
- Two files may not bind the same import name to different targets.
- A declaration may not reuse a name bound by an import in the same file.

Each of these is a compile error that names both files and suggests a rename
or an `as` alias. Module-private scoping is planned.

## Errors you might see

```text
`File` is not ambient — add `use std/file` and write `file.<method>(...)`
```
The PascalCase prelude (`File.read`, `ai.classify` written as `Ai.classify`,
…) was removed when the module system landed. Add the `use std/<name>`
import and lowercase the call.

```text
circular import: a.keel → b.keel → a.keel
```
Imports may not form cycles. Move the shared declarations into a third file
both can import.

```text
unknown std module `std/nope`
```
The error lists every available std module. See
[The Standard Library](./stdlib.md) for the full table.

## What stays built in

Agent lifecycle and messaging are part of the language, not the library:
`run`, `stop`, `send`, `delegate`, and `broadcast` are always in scope,
along with `min`, `max`, `typeof`, the built-in types (`str`, `int`,
`datetime`, `duration`, `Uuid`, …), and the built-in interfaces
(`Stringable`, `Comparable`, …).

`community/...` package paths are reserved for a future release.
