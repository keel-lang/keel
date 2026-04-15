# keel init

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
  role "Describe what this agent does"
  model "claude-sonnet"

  every 1.hour {
    notify user "Hello from my-project!"
  }
}

run MyProject
```

The agent name is automatically derived from the project name in PascalCase: `my-email-bot` → `MyEmailBot`.

## Example

```bash
keel init task-sorter
# ✓ Created project 'task-sorter'
#   task-sorter/main.keel
#
#   Run it:  keel run task-sorter/main.keel
```
