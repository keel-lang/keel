# Csv — CSV serialization

> **Alpha (v0.1).** Breaking changes expected.

The `Csv` namespace parses and produces RFC 4180–compliant CSV text. No `@tools` annotation is required.

```keel
use std/csv
use std/file
use std/log

agent TradeLoader {
    @on_start {
        raw = file.read("trades.csv")

        # list[map[str, str]] — first row becomes header keys
        trades = csv.parse_records(raw)
        for trade in trades {
            log.info("{trade["symbol"]} @ {trade["price"] as float:.2f}")
        }

        # Build and write a filtered CSV
        rows = [["symbol", "price"]]
        for trade in trades {
            if trade["price"] as float > 1000.0 {
                rows.push([trade["symbol"], trade["price"]])
            }
        }
        file.write("filtered.csv", csv.stringify(rows))
        stop(self)
    }
}
run(TradeLoader)
```

## Functions

### `csv.parse(text: str) -> list[list[str]]`

Parse a CSV string into rows of strings. Every cell becomes a `str` regardless of the original content. The first row is treated as data, not headers — use `csv.parse_records` if you want header-keyed maps.

```keel
rows = csv.parse("name,score\nAlice,10\nBob,20")
# rows[0] == ["name", "score"]
# rows[1] == ["Alice", "10"]
```

Quoted fields (containing commas, quotes, or newlines) are handled automatically per RFC 4180.

Raises `CsvError` on malformed input (e.g. unclosed quotes).

---

### `csv.parse_records(text: str) -> list[map[str, str]]`

Parse a CSV string using the **first row as header keys**. Returns one map per data row. Absent cells (short rows) default to `""`.

```keel
trades = csv.parse_records("symbol,price\nBTC,67000\nETH,3500")
# trades[0] == {symbol: "BTC", price: "67000"}
# trades[1] == {symbol: "ETH", price: "3500"}

for t in trades {
    log.info("{t["symbol"]}: {t["price"] as float:.2f}")
}
```

If the input has only a header row (no data rows), returns `[]`.

---

### `csv.stringify(rows: list[list[str]]) -> str`

Convert a list of rows to a CSV string. Each inner list is one row; every cell must be a `str`. Cells containing commas, quotes, or newlines are automatically quoted per RFC 4180. Raises `CsvError` if a row element is not a list or a cell is not a `str`.

Include a header row as the first inner list:

```keel
rows = [
    ["symbol", "price", "volume"],
    ["BTC", "67000", "1234.5"],
    ["ETH", "3500", "5678.9"],
]
text = csv.stringify(rows)
file.write("output.csv", text)
```

`csv.stringify` only accepts `list[list[str]]`. To convert `list[map[str, str]]` (e.g. from `parse_records`), project the fields you need:

```keel
lines = trades.map(t => [t["symbol"], t["price"]])
text  = csv.stringify([["symbol", "price"]] + lines)
```

## Round-trip example

```keel
use std/csv
use std/io

agent CsvRoundtrip {
    @on_start {
        original = "name,score\nAlice,10\nBob,20"
        rows     = csv.parse(original)
        back     = csv.stringify(rows)
        # back contains the same data, possibly with CRLF line endings
        io.show(back)
        stop(self)
    }
}
run(CsvRoundtrip)
```
