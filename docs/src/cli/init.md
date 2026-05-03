# keel init

> **Alpha (v0.1).** Breaking changes expected.

Scaffold a new Keel project.

```bash
keel init                  # initialize in the current directory
keel init <project-name>   # create a new subdirectory
keel init <path>           # create at an absolute or relative path
```

## What it creates

```
├── main.keel      # Starter agent
└── .gitignore
```

The generated `main.keel`:

```keel
# myproject — built with Keel

agent Myproject {
  @role "Describe what this agent does"

  @on_start {
    Io.show("Hello from Myproject!")
    stop(self)
  }
}

run(Myproject)
```

The agent name is derived from the project name in PascalCase: `my-email-bot` → `MyEmailBot`.

## Examples

```bash
# Initialize in the current directory
mkdir myproject && cd myproject
keel init
keel run main.keel

# Create a named subdirectory
keel init task-sorter
keel run task-sorter/main.keel

# Use an absolute path (basename is used as the project name)
keel init /tmp/mybot
keel run /tmp/mybot/main.keel
```

`keel init` refuses to overwrite an existing `main.keel` or directory.
