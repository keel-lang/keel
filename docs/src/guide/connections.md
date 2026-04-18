# Stdlib: `Email` & `Http`

> **Alpha (v0.1).** Breaking changes expected.

External connections live in stdlib namespaces. `Email` handles IMAP/SMTP. `Http` handles HTTP. `Db` handles SQL. Each one dispatches through an interface so the backend is swappable.

## `Email`

Default implementation uses `imap` (fetch) + `lettre` (send).

### Configure a connection

```keel
mailbox = Email.imap(
  host: Env.require("IMAP_HOST"),
  user: Env.require("EMAIL_USER"),
  pass: Env.require("EMAIL_PASS")
)
```

### Fetch messages

```keel
emails = Email.fetch(unread: true)                     # default mailbox
emails = Email.fetch(from: mailbox, unread: true)      # specific
recent = Email.fetch(from: mailbox, since: 7.days.ago)
```

`Email.fetch` returns `list[EmailInfo]`.

### Send messages

```keel
Email.send(reply, to: email.from)
Email.send(report, to: Env.require("DIGEST_TO"), subject: "Weekly digest")
```

### Archive

```keel
Email.archive(email)
```

Archiving is a connection-level operation — what "archive" means is defined by the transport (IMAP folder move, API call, etc.).

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
