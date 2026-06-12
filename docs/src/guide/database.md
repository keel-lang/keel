# Db — SQLite database

> **Alpha (v0.1).** Breaking changes expected.

The `Db` namespace provides access to SQLite databases via `db.connect()`. Each connection can execute queries and commands. No `@tools` annotation is required.

```keel
use std/db
use std/log

agent DataPipeline {
    @tools [db]
    @on_start {
        db = db.connect("sqlite://trades.db")

        db.exec("CREATE TABLE IF NOT EXISTS trades (id TEXT, symbol TEXT, price REAL)")
        db.exec("INSERT INTO trades VALUES (?, ?, ?)", ["t1", "BTCUSDT", 67000.0])

        rows = db.query("SELECT symbol, price FROM trades WHERE symbol = ?", ["BTCUSDT"])
        for row in rows {
            log.info("{row["symbol"]} @ {row["price"] as float:.2f}")
        }

        count = db.exec("DELETE FROM trades WHERE symbol = ?", ["BTCUSDT"])
        log.info("Deleted {count} row(s)")

        stop(self)
    }
}
run(DataPipeline)
```

## Connection URLs

Use the `sqlite://` scheme. All three forms are valid:

- `sqlite://path/to/db.db` — file-relative path
- `sqlite:///abs/path/to/db.db` — absolute path
- `sqlite://:memory:` — in-memory database (not persisted)

SQLite is bundled into the Keel binary — no system library required. Other schemes (`postgres://`, `mysql://`) raise a clear error: "only sqlite:// is supported in v0.1".

## Functions

### `db.connect(url: str) -> DbConnection`

Open or create a SQLite database. Returns a connection value with `.query()` and `.exec()` methods.

```keel
db = db.connect("sqlite://data/app.db")
```

---

### `DbConnection.query(sql: str, params?: list[dynamic]) -> list[map[str, dynamic]]`

Execute a SELECT query and return all matching rows. Each row is a `map[str, dynamic]` — field names are column names (lowercased or as-is per the query), values are their dynamic types.

```keel
rows = db.query("SELECT name, score FROM users")
for row in rows {
    log.info("{row["name"]}: {row["score"]}")
}

# Parameterized query — use ? placeholders
age = 21
results = db.query("SELECT * FROM users WHERE age > ?", [age])
```

Column values are returned as:
- `int` for INTEGER
- `float` for REAL
- `str` for TEXT
- `none` for NULL

Access fields with map syntax: `row["name"]` or `row["age"] as int`.

---

### `DbConnection.exec(sql: str, params?: list[dynamic]) -> int`

Execute an INSERT, UPDATE, or DELETE statement. Returns the number of rows affected. For CREATE TABLE and other DDL statements, returns 0.

```keel
# Create table
db.exec("CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, name TEXT, age INT)")

# Insert
count = db.exec("INSERT INTO users VALUES (?, ?, ?)", ["u1", "Alice", 30])

# Update
count = db.exec("UPDATE users SET age = ? WHERE name = ?", [31, "Alice"])

# Delete
count = db.exec("DELETE FROM users WHERE age < ?", [18])
log.info("Deleted {count} user(s)")
```

---

## Parameterized queries

Always use parameterized queries (`?` placeholders) when inserting user-provided values. This prevents SQL injection:

```keel
# Safe — value is parameterized
db.exec("INSERT INTO users (name, email) VALUES (?, ?)", [user_input_name, user_input_email])

# Unsafe — string interpolation allows injection
# db.exec("INSERT INTO users (name) VALUES ('{user_input}')")  # DON'T DO THIS
```

The `params` list is optional; omit it for queries with no placeholders:

```keel
db.query("SELECT COUNT(*) as count FROM users")
```

## Transactions and error handling

SQLite operates in autocommit mode by default — each `exec()` is its own transaction. Use `BEGIN` and `COMMIT` for multi-statement transactions:

```keel
db.exec("BEGIN")
try {
    db.exec("INSERT INTO accounts (user, balance) VALUES (?, ?)", [user_id, 100.0])
    db.exec("UPDATE ledger SET total = total + ? WHERE id = ?", [100.0, ledger_id])
    db.exec("COMMIT")
}
catch e {
    db.exec("ROLLBACK")
    raise "Transaction failed: {e}"
}
```

Malformed SQL, constraint violations, and I/O errors raise exceptions and halt execution.

## In-memory databases

Use `sqlite://:memory:` for temporary data that doesn't persist:

```keel
use std/db
use std/io

agent Scratchpad {
    @tools [db, io]
    @on_start {
        scratch = db.connect("sqlite://:memory:")
        scratch.exec("CREATE TABLE temp (id INT, value TEXT)")
        scratch.exec("INSERT INTO temp VALUES (1, 'hello')")
        results = scratch.query("SELECT * FROM temp")
        io.show(results[0])
        stop(self)
    }
}
run(Scratchpad)
```

## Performance notes

- Each `.query()` and `.exec()` call is a separate SQLite statement execution. For bulk inserts, use a transaction:

  ```keel
  db.exec("BEGIN")
  for item in items {
      db.exec("INSERT INTO data VALUES (?)", [item])
  }
  db.exec("COMMIT")
  ```

- SQLite is single-writer; concurrent writes from multiple agents may timeout. For production workloads, consider Postgres (planned for v0.2).
