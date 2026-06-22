# The Standard Library

Keel's standard library is a set of modules imported with `use std/<name>`:

```keel
use std/ai
use std/io
use std/file

text = file.read("inbox.txt")
mood = ai.classify(text, as: Sentiment)
io.show("mood: {mood}")
```

Stdlib modules and your own files are the same concept — see
[Modules & Imports](./modules.md). Nothing from the stdlib is ambient; if a
file calls `file.read(...)`, it says `use std/file` at the top. The only
always-in-scope names are the agent verbs (`run`, `stop`, `send`,
`delegate`, `broadcast`), generic utilities (`min`, `max`, `typeof`), the
built-in types, and the built-in interfaces.

> **Migrating from the ambient prelude?** Earlier alphas auto-imported
> PascalCase namespaces (`File.read`, `Ai.classify`). Add the matching
> `use std/<name>` import and lowercase the call: `file.read`,
> `ai.classify`. The compiler error tells you exactly which import to add.

## Why Modules

- **Small core.** The compiler doesn't know about `classify`, `fetch`, or `every`. Those are library function calls. Parser, lexer, and type checker stay free of domain-specific special cases.
- **Explicit dependencies.** A file's imports are its capability surface — auditable at a glance, and `@tools [...]` gates them per agent.
- **Keyword feel.** `ai.classify(...)` is still ceremony-free; the import is one line and autocomplete does the rest.
- **Interface boundary.** std modules are designed around interfaces so custom LLM providers, memory stores, schedulers, or HTTP clients can be installed in a later runtime.

## The Modules

Status legend: ✅ shipping · 🟡 partial · ⏳ <span class="badge badge-soon">Coming soon</span>

| Module | Status | Purpose |
|---|---|---|
| `std/ai` | 🟡 | LLM operations: `classify`, `extract`, `summarize`, `draft`, `translate`, `decide`, `prompt` · `embed` ⏳ |
| `std/io` | ✅ | Human interaction: `ask`, `confirm`, `notify`, `show` |
| `std/schedule` | ✅ | Time-based scheduling: `every`, `after`, `at`, `cron`, `sleep` |
| `std/email` | 🟡 | IMAP/SMTP: `fetch`, `send`, `archive` |
| `std/http` | ✅ | HTTP client + server: `get`, `post`, `request`, `serve` |
| `std/env` | ✅ | Environment: `get(name)`, `require(name)` |
| `std/log` | ✅ | Structured logging: `info`, `warn`, `error`, `debug`, plus `set_level`, `level`. Threshold default is `info`; raise via `--log-level debug`, `KEEL_LOG_LEVEL=debug`, or `log.set_level("debug")` at runtime. |
| `std/memory` | ✅ | Per-agent key-value store: `remember`, `recall`, `forget`. Scope set by `@memory session\|persistent\|none` (default: session). Persistent mode writes `~/.keel/memory/<stem>_<hash12>/<agent>.json`. |
| `std/control` | ✅ | `retry`, `with_timeout`, `with_deadline` |
| `std/async` | ✅ | Structured concurrency: `spawn`, `join_all`, `select`, `sleep` |
| `std/cache` | 🟡 | In-memory process-scoped cache: `set`, `get`, `delete`, `clear` |
| `String value methods` | ✅ | Regex and formatting directly on string values: matches, extract, find_all, sub, truncate, pad |
| `std/file` | ✅ | Filesystem: `read`, `write`, `exists`, `list`, `mkdir`, `remove`, `copy`, `move`, `glob`, `mktemp` |
| `std/json` | 🟡 | JSON: `parse`, `stringify` |
| `std/search` | 🟡 | Web search providers: `web(query)` — registered; raises "planned for v0.2" error |
| `std/db` | ✅ | SQLite: `connect(url)` → `DbConnection`; `db.query(sql, params?)` → `list[map[str,dynamic]]`; `db.exec(sql, params?)` → `int` |
| `std/time` | 🟡 | Factories: `now()`, `now(tz: name)`, `parse(str)`, `parse(str, tz: name)`, `epoch_ms()` → `int`. Methods on value: `dt.parts()` → map, `dt.format(as: pattern)` → `str?`. Duration literals: `500.ms` … `1.week`. Operators: `dt ± dur → dt`, `dt - dt → duration`, `<`/`>` comparison. Naive strings rejected — use RFC 3339 or `tz:`. |
| `std/random` | ✅ | Pseudo-random generation: `float()`, `int(min:, max:)`, `bool()`. Use `Crypto` for security-sensitive randomness. |
| `std/uuid` | ✅ | UUID values: `v4`, `v7`, deterministic `v5`, `parse`, `uuid()` alias, and value methods `version`, `format`, `to_str`. |
| `std/crypto` | ✅ | Cryptographic primitives: fixed safe SHA-2 hash/HMAC methods, `token`, `random_bytes`. |
| `std/math` | ✅ | Transcendentals: `sqrt`, `pow`, `exp`, `log` (natural), `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`. Constants: `PI()`, `E()`. All return `float`. Domain errors raise. |
| `std/shell` | ✅ | Subprocess bridge: execute shell commands via `/bin/sh -c` and capture stdout/stderr. Requires `@tools [shell]`. |
| `std/csv` | ✅ | CSV serialization: `parse`, `parse_records`, `stringify` — RFC 4180 compliant |
| `std/testing` | ✅ | Test doubles: `mock(...)` — see [Testing](./testing.md). |

Agent lifecycle and messaging — `run`, `stop`, `send`, `delegate`,
`broadcast` — are **built into the language**, not a module. Agents are
Keel's core abstraction; their verbs are always in scope.

**Modules gate entry points; values carry their methods.** Calling
`conn.query(...)` on a `DbConnection`, `dt.format(...)` on a `datetime`, or
`id.to_str()` on a `Uuid` needs no import — only the entry point
(`db.connect`, `time.now`, `uuid.v4`) requires its `use std/<name>` line.

## Free Functions

A small set of functions live directly in the global scope — no namespace prefix needed.

| Function | Signature | Returns | Notes |
|---|---|---|---|
| `run(agent)` | `(agent) -> none` | `none` | Start an agent |
| `stop(agent)` | `(agent) -> none` | `none` | Stop an agent |
| `send(agent, msg)` | `(agent, dynamic) -> none` | `none` | Post to an agent's mailbox |
| `delegate(Agent.handler, data)` | `(handler, dynamic) -> none` | `none` | Post a named handler event |
| `broadcast(team, msg)` | `(str, dynamic) -> none` | `none` | Fan out to a `@team` |
| `typeof(value)` | `(dynamic) -> str` | `str` | Runtime type name |
| `min(...)` | `(...items: T, by: (T -> any)? = none) -> T?` | `T?` | Minimum; `none` on empty |
| `max(...)` | `(...items: T, by: (T -> any)? = none) -> T?` | `T?` | Maximum; `none` on empty |

`min` and `max` accept any number of positional arguments, an optional `by:` key-selector, and return `none` when called with no items.

```keel
min(3, 1, 4)                           # 1
max("banana", "apple", "cherry")       # cherry

scores = [4, 9, 2, 7]
min(scores)                            # 2 — single list auto-spread
max(...scores, 99)                     # 99 — explicit spread

people = [{name: "Alice", age: 30}, {name: "Bob", age: 25}]
min(people, by: p => p.age)            # {name: "Bob", age: 25}
max(people, by: p => p.age)            # {name: "Alice", age: 30}
```

## Random

`Random` produces non-cryptographic pseudo-random values for simulation, sampling, games, and tests where security is not involved. Use `Crypto` for tokens, secrets, signatures, or key material.

{{#catalog random}}

```keel
roll = random.int(min: 1, max: 6)
sample = random.float()
enabled = random.bool()
```

## Uuid

`Uuid` is a distinct value type, not a `str`. It displays and interpolates as a lowercase hyphenated UUID, and it can be converted explicitly with `.to_str()`.

{{#catalog uuid}}

Namespace constants `uuid.DNS`, `uuid.URL`, `uuid.OID`, and `uuid.X500` are available for `uuid.v5`.

| Method | Returns | Notes |
|---|---|---|
| `.version()` | `int` | UUID version number |
| `.format(as:)` | `str` | `"hyphenated"`, `"simple"`, or `"urn"` |
| `.to_str()` | `str` | Lowercase hyphenated string |

```keel
id: Uuid = uuid.v4()
trace = uuid.v7()
site = uuid.v5(ns: uuid.DNS, name: "keel-lang.dev")
parsed = uuid.parse("f47ac10b-58cc-4372-a567-0e02b2c3d479")
simple = id.format(as: "simple")
```

## Crypto

`Crypto` provides security-grade primitives backed by the operating system CSPRNG. It is distinct from `Random`; use `Crypto` for tokens, secrets, signatures, digests, and other security-sensitive work.

{{#catalog crypto}}

```keel
digest = crypto.sha256("hello")
wide = crypto.sha384("hello")
sig = crypto.hmac_sha256("message", key: secret)
token = crypto.token(bytes: 32)
bytes = crypto.random_bytes(16)
```

`Crypto` intentionally exposes fixed safe SHA-2 methods only. Legacy hashes such as MD5 and SHA-1, and string-selected hash algorithms, are not exposed.

## Db

`Db` provides SQLite-backed durable storage. `db.connect` returns a connection value; `.query` and `.exec` are called directly on that value.

{{#catalog db}}

`db.query(sql)` and `db.exec(sql)` are called on a `DbConnection` value returned by `db.connect`. Both accept an optional second argument `params: list` for `?` placeholder binding.

**Connection URLs** use the `sqlite://` scheme:

| URL | Opens |
|---|---|
| `sqlite://trades.db` | Relative path |
| `sqlite:///tmp/trades.db` | Absolute path |
| `sqlite://:memory:` | In-memory (tests, scratch) |

Other schemes (`postgres://`, `mysql://`) raise a clear error — "only sqlite:// is supported in v0.1".

**Row return type.** Each row is a `map[str, dynamic]`. Access fields by column name and narrow with `as T`:

```keel
db = db.connect("sqlite://trades.db")

db.exec("CREATE TABLE IF NOT EXISTS trades (id TEXT, symbol TEXT, price REAL, qty REAL)")
db.exec("INSERT INTO trades VALUES (?, ?, ?, ?)", ["t1", "BTCUSDT", 67000.0, 0.01])

rows = db.query("SELECT symbol, price, qty FROM trades WHERE symbol = ?", ["BTCUSDT"])
for row in rows {
    symbol = row["symbol"] as str
    price  = row["price"]  as float
    qty    = row["qty"]    as float
    log.info("{symbol} — {qty} @ {price:.2f}")
}
```

**Multiple databases** in the same program — each `db.connect` call returns an independent connection value:

```keel
trades    = db.connect("sqlite://trades.db")
analytics = db.connect("sqlite://analytics.db")

rows = trades.query("SELECT * FROM fills")
analytics.exec("INSERT INTO daily_pnl VALUES (?)", [pnl])
```

**Parameterized queries.** Use `?` placeholders and pass a `list` as the second argument. Supported param types: `str`, `int`, `float`, `bool` (stored as `0`/`1`), `none` (NULL).

> **v0.1 scope.** SQLite only. Postgres and MySQL support, along with a pluggable multi-backend registry, are planned for v0.2. Requires `@tools [db]`.

## Json

`json.parse` and `json.stringify` bridge between Keel values and JSON strings.

{{#catalog json}}

**`json.parse` return-type semantics.** The result is `dynamic` — the type is not known statically. At runtime, JSON types map to Keel values as follows:

| JSON | Keel runtime value |
|---|---|
| object `{}` | field-accessible — `parsed.fieldName` works, raises on missing key |
| array `[]` | `list[dynamic]` — indexable and iterable |
| integer | `int` |
| float | `float` |
| string | `str` |
| boolean | `bool` |
| null | `none` |

Narrow with `as T` as early as possible:

```keel
body = http.get("https://api.example.com/ticker")?.body ?? ""
data = json.parse(body) as dynamic

price  = (data.price as str).to_float() ?? 0.0
volume = data.volume as int
rows   = data.candles as list[dynamic]
```

**`json.stringify`** serialises Keel structs, maps, lists, and primitives to JSON. If the value implements `Serializable`, its `to_json()` method is called instead of the default serialiser.

## Cache

`Cache` is a process-scoped in-memory key-value store with optional TTL. It is shared across all agents in the same process — a convenient way to pass state between agents without serialising to disk.

{{#catalog cache}}

**`cache.get` return-type semantics.** The return type is `dynamic?`. The stored type is preserved exactly — a value written as `str` is read back as `str`, a value written as `int` is read back as `int`. Use `as T` to recover a concrete type, and `??` to supply a default:

```keel
cache.set("trading:halted", "true")
halted_raw = cache.get("trading:halted")      # dynamic?
halted = if halted_raw == none { false } else { (halted_raw as str) == "true" }

cache.set("count", 0)
n = (cache.get("count") ?? 0) as int

cache.set("rate", 0.5, ttl: 30.seconds)
rate = (cache.get("rate") ?? 0.0) as float
```

## Log

`Log` writes structured messages at four severity levels. Output goes to stderr; messages below the current threshold are suppressed.

{{#catalog log}}

The threshold can also be set before launch with `--log-level debug` (CLI flag) or `KEEL_LOG_LEVEL=debug` (env var). `log.set_level` changes it at runtime.

```keel
log.debug("cache miss for key {key}")
log.info("order filled: {order_id}")
log.warn("rate limit approaching: {remaining} calls left")
log.error("payment failed: {code}")

log.set_level("debug")
current = log.level()   # "debug"
```

## Search

`Search` provides web and news search. Both methods return a list of `SearchResult` values.

{{#catalog search}}

> `std/search` is registered but raises a "not yet implemented" error at runtime. Wire a custom backend via the `SearchProvider` interface when it lands.

## Async

`Async` provides structured concurrency — run multiple tasks in parallel and collect their results.

{{#catalog async}}

```keel
task1 = async.spawn(() => { fetch_price("BTC") })
task2 = async.spawn(() => { fetch_price("ETH") })

# Wait for both
prices = async.join_all([task1, task2])

# Or race — whichever resolves first
winner = async.select([task1, task2])
```

`async.select` cancels any handles that haven't completed yet once a winner is found.

> **v0.1 scope.** Anything marked ⏳ is reserved in the grammar but not yet wired. 🟡 means partial: something works, but not everything. `Search` is registered and raises a clear "planned for v0.2" error; `ai.embed` returns an empty list. The status column in the table above is authoritative.

## Interfaces

An **interface** declares a set of method signatures. Any type with matching methods structurally satisfies the interface — no explicit `implements`.

```text
interface LlmProvider {
  task complete(messages: list[Message], opts: LlmOpts) -> LlmResponse?
  task embed(text: str) -> list[float]?
}

interface VectorStore {
  task put(key: str, value: map[str, str], embedding: list[float])
  task query(embedding: list[float], limit: int) -> list[str]
}

interface Tracer {
  task on_event(event: str)
}
```

Every effectful stdlib module dispatches through one or more interfaces:

| Module | Interface(s) |
|---|---|
| `std/ai` | `LlmProvider` |
| `std/memory` | `VectorStore`, `Embedder` |
| `std/http` | `HttpClient` |
| `std/email` | `EmailTransport` |
| `std/search` | `SearchProvider` |
| `std/log` | `Tracer` |

## Swapping Implementations

Per-call model selection works today via the `using:` keyword:

```keel
use std/ai

urgency = ai.classify(body, as: Urgency, using: "smart")
```

`using:` resolves via `KEEL_MODEL_<ALIAS>` env vars and Ollama tags. Ollama is the only wired backend; `ai.install(...)` and `@provider` are reserved and have no runtime effect yet.

## Shadowing Module Bindings

Module bindings are identifiers, not keywords. A local binding can shadow
one inside a body — the idiomatic connection pattern relies on it:

```keel
use std/db

conn = db.connect("sqlite://trades.db")   # clearer: keep `db` for the module
```

The linter warns when a binding shadows a module import. Use deliberately.

## Modules, Not Keywords

Operations like `classify`, `draft`, `every`, `fetch`, `ask`, and `confirm`
are stdlib functions on the `ai`, `io`, `email`, `schedule`, and `http`
modules — not reserved words. The core language stays small (~27 keywords),
and the stdlib is a normal Rust crate that anyone can extend or replace.
