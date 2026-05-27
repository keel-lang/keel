# Json

> **Alpha (v0.1).** Breaking changes expected.

The `Json` namespace serializes and deserializes JSON. No `@tools` annotation required.

```keel
text = Json.stringify({ symbol: "BTC", price: 67000 })
data = Json.parse(text) as dynamic
Io.show("{data.symbol}")   # BTC
```

---

## `Json.stringify(value) -> str`

Converts a Keel value to a JSON string.

```keel
Json.stringify("hello")              # "\"hello\""
Json.stringify(42)                   # "42"
Json.stringify(true)                 # "true"
Json.stringify(none)                 # "null"
Json.stringify([1, 2, 3])            # "[1,2,3]"
Json.stringify({ a: 1, b: "x" })    # "{\"a\":1,\"b\":\"x\"}"
```

If a type implements the [`Serializable`](./interfaces.md) interface, `Json.stringify` calls `to_json()` instead of the default serializer:

```keel
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
Json.stringify(o)   # {"id":"ord-1","amount":99.50}
```

---

## `Json.parse(text)`

Parses a JSON string. The type checker does not infer a precise return type — treat the result as untyped and narrow with `as T` before using it. The JSON-to-Keel type mapping at runtime is:

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
body = Http.get("https://api.example.com/ticker")?.body ?? ""
data = Json.parse(body) as dynamic

price  = data.price as str             # str field
volume = data.volume as int            # int field
rows   = data.candles as list[dynamic] # array field

for row in rows {
  close = (row as list[dynamic])[4] as str
}
```

> **Tip:** narrow immediately after parsing. Keeping values as `dynamic` throughout a program defeats the type checker and suppresses autocomplete.
