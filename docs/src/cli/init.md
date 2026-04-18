# keel init

> **Alpha (v0.1).** Breaking changes expected.

Scaffold a new Keel project.

```bash
keel init <project-name>
```

## What it creates

```
my-project/
├── main.keel      # Starter agent
└── .gitignore
```

The generated `main.keel`:

```keel
# my-project — built with Keel

agent MyProject {
  @role "Describe what this agent does"
  @model "claude-sonnet"

  @on_start {
    Schedule.every(1.hour, () => {
      Io.notify("Hello from my-project!")
    })
  }
}

run(MyProject)
```

The agent name is derived from the project name in PascalCase: `my-email-bot` → `MyEmailBot`.

## Example

```bash
keel init task-sorter
# ✓ Created project 'task-sorter'
#   task-sorter/main.keel
#
#   Run it:  keel run task-sorter/main.keel
```
