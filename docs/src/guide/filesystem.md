# Filesystem — `File`

> **Alpha (v0.1).** Breaking changes expected.

The `File` namespace gives agents access to the local filesystem. It is auto-imported — no `use` required.

## Reading and writing

```keel
content = file.read("data/report.txt")    # str — raises FileError if file missing
file.write("output/result.txt", content)
```

`file.read` returns `str`. If the file does not exist it raises a `FileError` at runtime — use `file.exists` to guard reads when the path may be absent.

`file.write` creates parent directories automatically.

All `file.*` paths and `file.write` content must be `str` values. Dynamic values with
another runtime type raise a clear type error; they are not silently formatted as strings.

## Existence and listing

```keel
if file.exists("config.json") {
  cfg = file.read("config.json")
}

entries = file.list("output")   # list[str] — names only, not full paths
```

## Creating directories

```keel
file.mkdir("output/reports/2026")   # creates all missing parents
```

## Copying and moving

```keel
file.copy("template.txt", "output/report.txt")   # src unchanged
file.move("draft.txt", "published/final.txt")    # src removed; atomic on same filesystem
```

Both `copy` and `move` create missing parent directories on the destination side automatically.

## Removing files and directories

```keel
file.remove("tmp/scratch.txt")    # single file
file.remove("tmp/cache")          # directory — removed recursively (rm -rf semantics)
```

`file.remove` auto-detects whether the path is a file or a directory. Removing a non-existent path is a runtime error.

## Glob patterns

```keel
reports = file.glob("output/*.txt")        # files in output/ matching *.txt
all_rs  = file.glob("src/**/*.rs")         # recursive — all .rs files under src/
```

Returns a `list[str]` of matching paths. No matches → empty list. An invalid pattern is a runtime error.

Supported pattern syntax: `*` (any chars in one segment), `?` (single char), `**` (zero or more segments, recursive).

## Temporary files and directories

```keel
tmp     = file.mktemp()            # creates a temp file; returns its path as str
tmpdir  = file.mktemp(dir: true)   # creates a temp directory; returns its path
```

`file.mktemp` creates the file or directory immediately and returns the path. **Lifecycle is the caller's responsibility** — use `file.remove(path)` when done:

```keel
tmp = file.mktemp()
file.write(tmp, processed_content)
result = file.read(tmp)
file.remove(tmp)
```

## Quick reference

{{#catalog file}}

## Error handling

All `file.*` methods that fail for I/O reasons throw a `FileError`. Catch it by type name:

```keel
try {
    content = file.read("config.json")
} catch err: FileError {
    io.notify("Could not read config: {err.message}")
    content = "{}"
} catch err: Error {
    io.notify("Unexpected error: {err.message}")
}
```

`FileError` carries a `message: str` field. Its diagnostic code is `keel::runtime::FileError`.

---

For subprocess execution, see [Shell — subprocess bridge](./shell.md).
