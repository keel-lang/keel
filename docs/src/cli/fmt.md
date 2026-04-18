# keel fmt

> **Alpha (v0.1).** Breaking changes expected.

Auto-format a Keel file with consistent style.

```bash
keel fmt <file.keel>
```

## What it does

- 2-space indentation
- Consistent spacing around operators and `@attribute` values
- One declaration per section with blank line separators
- Single-line `when` arms for simple expressions
- Writes the formatted output back to the file

## Example

Before:

```keel
agent Bot{@role "helper"
state{count:int=0}
@on_start{Schedule.every(1.day, () => {Io.notify("hello")
self.count=self.count+1})}}
run(Bot)
```

After `keel fmt`:

```keel
agent Bot {
  @role "helper"

  state {
    count: int = 0
  }

  @on_start {
    Schedule.every(1.day, () => {
      Io.notify("hello")
      self.count = self.count + 1
    })
  }
}

run(Bot)
```
