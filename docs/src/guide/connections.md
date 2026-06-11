# Stdlib: `Email` & `Http`

> **Alpha (v0.1).** Breaking changes expected.

External connections live in stdlib namespaces. `Email` handles IMAP/SMTP. `Http` handles HTTP. (See also [`Db` for SQLite access](./database.md).) The interface boundary is planned so these backends can become swappable; v0.1 wires the default `Email` and `Http` transports only.

## `Email`

Default implementation uses `imap` (fetch) + `lettre` (send). v0.1 reads credentials from environment variables:

```bash
export IMAP_HOST=imap.gmail.com
export SMTP_HOST=smtp.gmail.com          # optional — defaults to IMAP host with `imap.` → `smtp.`
export EMAIL_USER=you@example.com
export EMAIL_PASS=app-password
```

If those aren't set, `email.fetch` returns `[]` and `email.send` is a no-op (with a stderr warning), so programs keep running.

### Fetch messages

```keel
emails = email.fetch(unread: true)   # up to 20 most recent unread from INBOX
```

Each returned map has `from`, `subject`, `body`, `unread`, and `uid` keys.
The `uid` is the IMAP UID of the message and is required by `email.archive`.

### Send messages

```keel
email.send(reply, to: email.from)
email.send(reply, to: address, subject: "Re: hello")
```

Positional body can be a `str` or a `map` with `body` (and optional `subject`). `to:` can be an address string or a map with `from`.

### Archive

```keel
for email in email.fetch(unread: true) {
  email.archive(email)
}
```

`email.archive` performs an IMAP UID MOVE on the message, falling back
to COPY + `\Deleted` + EXPUNGE for servers without the MOVE extension.
The destination folder defaults to `Archive`; override with the
`IMAP_ARCHIVE_FOLDER` env var:

```bash
export IMAP_ARCHIVE_FOLDER="[Gmail]/All Mail"
```

The argument must be a message map with a positive `uid` field — the
shape returned by `email.fetch`. If credentials are not configured the
call is a silent no-op so programs keep running.

## `Http`

Default implementation wraps `reqwest`.

### GET

```keel
response = http.get("https://api.example.com/data")
# response: HttpResponse?

if response?.is_ok {
  users = response?.json_as[list[User]]() ?? []
}
```

### POST

```keel
response = http.post("https://api.example.com/v2/events",
  json: {kind: "email_processed", count: 42},
  headers: {Authorization: "Bearer {env.require("API_KEY")}"}
)
```

### Full request

```keel
response = http.request(
  method: POST,
  url: "https://api.example.com/v2/classify",
  headers: {
    Authorization: "Bearer {env.require("API_KEY")}",
    "Content-Type": "application/json"
  },
  body: {text: email.body},
  timeout: 10.seconds
)
```

**Returns:** `HttpResponse?` — see [Types](./types.md) for the shape.

### `http.serve` — inbound HTTP (webhooks)

Start an HTTP listener on a port. Each incoming request invokes the handler closure:

```keel
http.serve(8080, (request) => {
  method = request["method"]   # "GET", "POST", …
  path   = request["path"]     # "/webhook/events"
  body   = request["body"]     # raw body string

  if method == "POST" {
    io.show("Received: {body}")
    { status: 200, body: "OK" }
  } else {
    { status: 405, body: "Method Not Allowed" }
  }
})
```

- `request` — map with `method`, `path`, `body` (all strings)
- Return a map with `status` (integer, 100–999) and `body` (string)
- The server runs in a background task; `http.serve` returns immediately
- The event loop stays alive as long as at least one server is active, even with no running agents
- **When the event queue is full**, incoming HTTP requests receive a `503 Service Unavailable` response automatically. No user code runs for that request.

> **Handlers run outside any agent context.** An `http.serve` handler
> is a top-level closure — it fires on the event loop with no
> `current_agent` set. That has two consequences:
>
> - **`self.<field>` raises a runtime error** inside a handler. Agent
>   state is only reachable from within an agent's tasks / `on`
>   handlers / attribute blocks.
> - **`ai.*` calls are not agent-aware.** No `@role`, no `@rules`, and
>   no `@model` injection — calls fall back to the default model
>   (`KEEL_OLLAMA_MODEL`) with a bare system prompt. Results are still
>   returned, just without the agent's identity layered in.
>
> To use agent state or an agent's `@role` / `@model` from a handler,
> route the request into a live agent:
>
> ```keel
> http.serve(8080, (request) => {
>   send(Triage, request, event: "http_request")
>   { status: 202, body: "accepted" }
> })
> ```
>
> The matching `on http_request(req) { ... }` handler on `Triage`
> runs *with* `self.`, `@role`, `@rules`, and `@model` all wired up.

## `Db`

`db.connect` opens a SQLite database and returns a `DbConnection` value. All SQL is
executed through that value. Requires `@tools [db]`.

```keel
db = db.connect("sqlite://interactions.db")

rows = db.query(
  "SELECT * FROM interactions WHERE contact = ? AND created_at > ?",
  [email.from, 30.days.ago]
)
# rows: list[map[str, dynamic]]

db.exec("UPDATE status SET seen = true WHERE id = ?", [ticket.id])
# returns int — number of rows affected
```

> **v0.1 scope.** SQLite only (`sqlite://path`, `sqlite:///abs/path`, `sqlite://:memory:`).
> Postgres and MySQL support are planned for v0.2.

## Swapping the backend <span class="badge badge-soon">Coming soon</span>

The planned interface flow for custom transports is:

```keel
# In your startup
email.install(MyProprietaryEmailTransport)
http.install(MyRateLimitedClient)
```

> **Status:** `email.install` / `http.install` are reserved but not registered in v0.1 — the default transports are the only ones wired.

See [The Prelude & Interfaces](./stdlib.md) for how interface dispatch works.

## Why a library, not `connect` + `fetch` keywords

Dedicated `connect X via Y { ... }` or `fetch X where Y` grammar wouldn't compose well: `connect` is really a struct literal, and `fetch` generalizes badly across connection types (email's `unread` is not SQL's `where`). Per-connector libraries give better autocomplete, clearer types, and zero language changes when a new connector ships.
