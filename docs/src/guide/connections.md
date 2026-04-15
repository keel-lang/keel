# Connections

Connections establish authenticated links to external services.

## Email (IMAP/SMTP)

```keel
connect email via imap {
  host: env.IMAP_HOST,
  user: env.EMAIL_USER,
  pass: env.EMAIL_PASS
}
```

Set the environment variables:

```bash
export IMAP_HOST=imap.gmail.com
export EMAIL_USER=you@gmail.com
export EMAIL_PASS=your-app-password
```

### Fetch emails

```keel
emails = fetch email where unread
```

Returns a `list` of email maps with fields: `from`, `subject`, `body`, `unread`.

### Send emails

```keel
send reply to email
```

Sends via SMTP. The SMTP host is derived from the IMAP host (`imap.` → `smtp.`), or set explicitly:

```keel
connect email via imap {
  host: env.IMAP_HOST,
  smtp_host: env.SMTP_HOST,
  user: env.EMAIL_USER,
  pass: env.EMAIL_PASS
}
```

## HTTP fetch

Fetch URLs directly:

```keel
response = fetch "https://api.example.com/data"
# response.status   — int (200, 404, etc.)
# response.body     — str (response body)
# response.headers  — map[str, str]
# response.is_ok    — bool (status 200-299)
```

## Archive

Move an item to its archive (connection-specific behavior):

```keel
archive email    # moves to archive folder in IMAP
```

## Environment variables

Access environment variables with `env.`:

```keel
api_key = env.API_KEY
host = env.IMAP_HOST
debug = env.DEBUG
```
