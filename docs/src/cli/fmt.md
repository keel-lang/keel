# keel fmt

Auto-format an Keel file with consistent style.

```bash
keel fmt <file.keel>
```

## What it does

- 2-space indentation
- Consistent spacing around operators
- One declaration per section with blank line separators
- Single-line `when` arms for simple expressions
- Writes the formatted output back to the file

## Example

Before:

```keel
agent Bot{
role "helper"
model "claude-haiku"
state{count:int=0}
every 1.day{notify user "hello"
self.count=self.count+1}}
run Bot
```

After `keel fmt`:

```keel
agent Bot {
  role "helper"

  model "claude-haiku"

  state {
    count: int = 0
  }

  every 1.days {
    notify user "hello"
    self.count = self.count + 1
  }
}

run Bot
```
