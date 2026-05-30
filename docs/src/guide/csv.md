# Csv — CSV serialization

> **Alpha (v0.1).** Breaking changes expected.

The `Csv` namespace parses and produces RFC 4180–compliant CSV text. No `@tools` annotation is required.

```keel
agent TradeLoader {
    @on_start {
        raw = File.read("trades.csv")

        # list[map[str, str]] — first row becomes header keys
        trades = Csv.parse_records(raw)
        for trade in trades {
            Log.info("{trade["symbol"]} @ {trade["price"] as float:.2f}")
        }

        # Build and write a filtered CSV
        rows = [["symbol", "price"]]
        for trade in trades {
            if trade["price"] as float > 1000.0 {
                rows.push([trade["symbol"], trade["price"]])
            }
        }
        File.write("filtered.csv", Csv.stringify(rows))
        stop(self)
    }
}
run(TradeLoader)
```

## Functions

### `Csv.parse(text: str) -> list[list[str]]`

Parse a CSV string into rows of strings. Every cell becomes a `str` regardless of the original content. The first row is treated as data, not headers — use `Csv.parse_records` if you want header-keyed maps.

```keel
rows = Csv.parse("name,score\nAlice,10\nBob,20")
# rows[0] == ["name", "score"]
# rows[1] == ["Alice", "10"]
```

Quoted fields (containing commas, quotes, or newlines) are handled automatically per RFC 4180.

Raises `CsvError` on malformed input (e.g. unclosed quotes).

---

### `Csv.parse_records(text: str) -> list[map[str, str]]`

Parse a CSV string using the **first row as header keys**. Returns one map per data row. Absent cells (short rows) default to `""`.

```keel
trades = Csv.parse_records("symbol,price\nBTC,67000\nETH,3500")
# trades[0] == {symbol: "BTC", price: "67000"}
# trades[1] == {symbol: "ETH", price: "3500"}

for t in trades {
    Log.info("{t["symbol"]}: {t["price"] as float:.2f}")
}
```

If the input has only a header row (no data rows), returns `[]`.

---

### `Csv.stringify(rows: list[list[str]]) -> str`

Convert a list of rows to a CSV string. Each inner list is one row; every cell must be a `str`. Cells containing commas, quotes, or newlines are automatically quoted per RFC 4180. Raises `CsvError` if a row element is not a list or a cell is not a `str`.

Include a header row as the first inner list:

```keel
rows = [
    ["symbol", "price", "volume"],
    ["BTC", "67000", "1234.5"],
    ["ETH", "3500", "5678.9"],
]
text = Csv.stringify(rows)
File.write("output.csv", text)
```

`Csv.stringify` only accepts `list[list[str]]`. To convert `list[map[str, str]]` (e.g. from `parse_records`), project the fields you need:

```keel
lines = trades.map(t => [t["symbol"], t["price"]])
text  = Csv.stringify([["symbol", "price"]] + lines)
```

## Round-trip example

```keel
agent CsvRoundtrip {
    @on_start {
        original = "name,score\nAlice,10\nBob,20"
        rows     = Csv.parse(original)
        back     = Csv.stringify(rows)
        # back contains the same data, possibly with CRLF line endings
        Io.show(back)
        stop(self)
    }
}
run(CsvRoundtrip)
```
