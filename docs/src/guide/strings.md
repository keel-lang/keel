# String Interpolation

Keel strings support `{expr}` interpolation — variables and expressions inside strings are evaluated at runtime.

## Basic interpolation

```keel
name = "Keel"
notify user "Hello, {name}!"           # "Hello, Keel!"
notify user "Count: {items.count}"     # "Count: 3"
notify user "Sum: {a + b}"            # expression evaluation
```

## Dotted paths

```keel
notify user "From: {email.from}"
notify user "Status: {self.count}"
notify user "Key: {env.API_KEY}"
```

## Escape sequences

| Sequence | Result |
|----------|--------|
| `\n` | Newline |
| `\t` | Tab |
| `\r` | Carriage return |
| `\\` | Backslash |
| `\"` | Double quote |
| `\{` | Literal `{` (prevents interpolation) |
| `\}` | Literal `}` |

```keel
notify user "Line 1\nLine 2"
notify user "Price: \{not interpolated\}"
```

## String methods

| Method | Returns | Example |
|--------|---------|---------|
| `.length` | `int` | `"hello".length` → `5` |
| `.is_empty` | `bool` | `"".is_empty` → `true` |
| `.contains(s)` | `bool` | `"hello".contains("ell")` → `true` |
| `.starts_with(s)` | `bool` | `"hello".starts_with("hel")` → `true` |
| `.ends_with(s)` | `bool` | `"hello".ends_with("lo")` → `true` |
| `.trim()` | `str` | `" hi ".trim()` → `"hi"` |
| `.upper()` | `str` | `"hello".upper()` → `"HELLO"` |
| `.lower()` | `str` | `"HELLO".lower()` → `"hello"` |
| `.split(sep)` | `list[str]` | `"a,b,c".split(",")` → `["a","b","c"]` |
| `.replace(old, new)` | `str` | `"hello".replace("l","r")` → `"herro"` |
| `.to_int()` | `int?` | `"42".to_int()` → `42` |
| `.to_float()` | `float?` | `"3.14".to_float()` → `3.14` |
