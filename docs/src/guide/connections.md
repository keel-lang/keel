# Stdlib: `Email` & `Http`

> **Alpha (v0.1).** Breaking changes expected.

External connections live in stdlib namespaces. `Email` handles IMAP/SMTP. `Http` handles HTTP. `Db` handles SQL. Each one dispatches through an interface so the backend is swappable.

## `Email`

Default implementation uses `imap` (fetch) + `lettre` (send). v0.1 reads credentials from environment variables:

```bash
export IMAP_HOST=imap.gmail.com
export SMTP_HOST=smtp.gmail.com          # optional — defaults to IMAP host with `imap.` → `smtp.`
export EMAIL_USER=you@example.com
export EMAIL_PASS=app-password
```

If those aren't set, `Email.fetch` returns `[]` and `Email.send` is a no-op (with a stderr warning), so programs keep running.

### Fetch messages

```keel
emails = Email.fetch(unread: true)   # up to 20 most recent unread from INBOX
```

Each returned map has `from`, `subject`, `body`, `unread` keys.

### Send messages

```keel
Email.send(reply, to: email.from)
Email.send(reply, to: address, subject: "Re: hello")
```

Positional body can be a `str` or a `map` with `body` (and optional `subject`). `to:` can be an address string or a map with `from`.

### Archive

```keel
Email.archive(email)
```

v0.1: no-op placeholder. Future release will implement IMAP folder move.

## `Http`

Default implementation wraps `reqwest`.

### GET

```keel
response = Http.get("https://api.example.com/data")
# response: HttpResponse?

if response?.is_ok {
  users = response?.json_as[list[User]]() ?? []
}
```

### POST

```keel
response = Http.post("https://api.example.com/v2/events",
  json: {kind: "email_processed", count: 42},
  headers: {Authorization: "Bearer {Env.require("API_KEY")}"}
)
```

### Full request

```keel
response = Http.request(
  method: POST,
  url: "https://api.example.com/v2/classify",
  headers: {
    Authorization: "Bearer {Env.require("API_KEY")}",
    "Content-Type": "application/json"
  },
  body: {text: email.body},
  timeout: 10.seconds
)
```

**Returns:** `HttpResponse?` — see [Types](./types.md) for the shape.

## `Db`

```keel
db = Db.connect("postgres://localhost/mydb")

rows = Db.query(db,
  "SELECT * FROM interactions WHERE contact = ? AND created_at > ?",
  params: [email.from, 30.days.ago]
)
# rows: list[dynamic]

Db.exec(db, "UPDATE status SET seen = true WHERE id = ?", params: [ticket.id])
```

## Swapping the backend

Each namespace dispatches through an interface. To plug in a custom transport:

```keel
# In your startup
Email.install(MyProprietaryEmailTransport)
Http.install(MyRateLimitedClient)
```

See [The Prelude & Interfaces](./prelude.md) for how interface dispatch works.

## Why a library, not `connect` + `fetch` keywords

Dedicated `connect X via Y { ... }` or `fetch X where Y` grammar wouldn't compose well: `connect` is really a struct literal, and `fetch` generalizes badly across connection types (email's `unread` is not SQL's `where`). Per-connector libraries give better autocomplete, clearer types, and zero language changes when a new connector ships.
