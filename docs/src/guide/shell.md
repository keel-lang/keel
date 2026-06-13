# Shell — subprocess bridge

The `Shell` namespace lets agents invoke external commands and capture their output. It must be declared in `@tools` before use — an agent that calls `shell.run` without listing `Shell` in `@tools` raises `CapabilityError` at runtime.

```keel
use std/io
use std/shell

agent Builder {
    @tools [shell, io]

    @on_start {
        r = shell.run("cargo test --quiet 2>&1")
        if r.exit_code != 0 {
            raise "tests failed:\n{r.stdout}"
        }
        io.show("all tests passed")
    }
}
run(Builder)
```

## `shell.run`

```
shell.run(cmd: str, stdin: str? = none, cwd: str? = none)
    -> { stdout: str, stderr: str, exit_code: int }
```

`cmd` is passed to `/bin/sh -c`, so pipes, redirects, and shell builtins all work.

| Argument | Notes |
|---|---|
| `cmd` | Required. The shell command to run. |
| `stdin:` | Optional text piped to the process's standard input. |
| `cwd:` | Optional working directory for the subprocess. Defaults to the interpreter's working directory. |

**Return value** — always a map with three keys:

| Key | Type | Notes |
|---|---|---|
| `stdout` | `str` | Standard output captured from the process. |
| `stderr` | `str` | Standard error captured from the process. |
| `exit_code` | `int` | `0` on success; non-zero on failure; `-1` if the OS cannot report one. |

A non-zero exit code is **not** automatically an error — check `exit_code` and raise yourself if needed. A spawn failure (e.g. `/bin/sh` not in `PATH`) does raise.

## Environment isolation

The subprocess runs with a clean environment. Only the following variables are forwarded from the keel process:

| Variable | Why forwarded |
|---|---|
| `PATH` | Command resolution |
| `HOME` | Many tools read `~` |
| `SHELL` | Scripts that re-exec themselves |
| `TMPDIR` | Temp file location |
| `USER` | Identity for tools that need it |
| `LANG` | Locale / character encoding |

All other variables — including secrets, API keys, database URLs, and any other credentials present in the keel process environment — are **not** visible to the shell command. To read the keel process environment from Keel code, use `env.*`.

## Capability gating

`@tools` restricts an agent to the listed namespaces. An agent that declares `@tools [io]` but not `Shell` will get a `CapabilityError` on any `shell.run` call. An agent with no `@tools` declaration is unrestricted.

Currently this is process-level gating only — there is no OS-level sandbox.

## Security note

`cmd` is passed directly to `/bin/sh -c`. Never interpolate untrusted user input into `cmd` without sanitisation — shell injection is possible.
