# Json

The `Json` namespace serializes and deserializes JSON. No `@tools` annotation required.

```keel
text = json.stringify({ symbol: "BTC", price: 67000 })
data = json.parse(text) as dynamic
io.show("{data.symbol}")   # BTC
```

---

## `json.stringify(value) -> str`

Converts a Keel value to a JSON string.

```keel
json.stringify("hello")              # "\"hello\""
json.stringify(42)                   # "42"
json.stringify(true)                 # "true"
json.stringify(none)                 # "null"
json.stringify([1, 2, 3])            # "[1,2,3]"
json.stringify({ a: 1, b: "x" })    # "{\"a\":1,\"b\":\"x\"}"
```

If a type implements the [`Serializable`](./interfaces.md) interface, `json.stringify` calls `to_json()` instead of the default serializer:

```keel
use std/json

type Order {
  id: str
  amount: float
}

impl Serializable for Order {
  task to_json(self) -> str {
    "{\"id\":\"{self.id}\",\"amount\":{self.amount:.2f}}"
  }
}

o: Order = { id: "ord-1", amount: 99.5 }
json.stringify(o)   # {"id":"ord-1","amount":99.50}
```

---

## `json.parse(text)`

Parses a JSON string. The type checker does not infer a precise return type — treat the result as untyped and narrow with `as T` or annotate as `dynamic` before using it. The JSON-to-Keel type mapping at runtime is:

| JSON type | Keel runtime value |
|---|---|
| object `{}` | map — field access `parsed.name` works at runtime |
| array `[]` | `list[dynamic]` — index with `parsed[i]`, iterate with `for` |
| number (integer) | `int` |
| number (float) | `float` |
| string | `str` |
| boolean | `bool` |
| null | `none` |

Named-field access (`parsed.price`) resolves at runtime. A missing key raises rather than returning `none` — use `??` or check existence explicitly.

Narrow to a concrete type with `as T` as early as possible:

```keel
body = http.get("https://api.example.com/ticker")?.body ?? ""
data = json.parse(body) as dynamic

price  = data.price as str             # str field
volume = data.volume as int            # int field
rows   = data.candles as list[dynamic] # array field

for row in rows {
  close = (row as list[dynamic])[4] as str
}
```

> **Strict mode:** `keel check --strict` flags unannotated `json.parse` bindings because the
> return type cannot be inferred statically. Add `data: dynamic = json.parse(...)` (explicit
> `dynamic` annotation, always clean) or cast immediately to a concrete type with `as T`.

> **Tip:** narrow immediately after parsing. Keeping values as `dynamic` throughout a program defeats the type checker and suppresses autocomplete.
