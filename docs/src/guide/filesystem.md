# Filesystem — `File`

> **Alpha (v0.1).** Breaking changes expected.

The `File` namespace gives agents access to the local filesystem. It is auto-imported — no `use` required.

## Reading and writing

```keel
content = File.read("data/report.txt")    # str — raises FileError if file missing
File.write("output/result.txt", content)
```

`File.read` returns `str`. If the file does not exist it raises a `FileError` at runtime — use `File.exists` to guard reads when the path may be absent.

`File.write` creates parent directories automatically.

All `File.*` paths and `File.write` content must be `str` values. Dynamic values with
another runtime type raise a clear type error; they are not silently formatted as strings.

## Existence and listing

```keel
if File.exists("config.json") {
  cfg = File.read("config.json")
}

entries = File.list("output")   # list[str] — names only, not full paths
```

## Creating directories

```keel
File.mkdir("output/reports/2026")   # creates all missing parents
```

## Copying and moving

```keel
File.copy("template.txt", "output/report.txt")   # src unchanged
File.move("draft.txt", "published/final.txt")    # src removed; atomic on same filesystem
```

Both `copy` and `move` create missing parent directories on the destination side automatically.

## Removing files and directories

```keel
File.remove("tmp/scratch.txt")    # single file
File.remove("tmp/cache")          # directory — removed recursively (rm -rf semantics)
```

`File.remove` auto-detects whether the path is a file or a directory. Removing a non-existent path is a runtime error.

## Glob patterns

```keel
reports = File.glob("output/*.txt")        # files in output/ matching *.txt
all_rs  = File.glob("src/**/*.rs")         # recursive — all .rs files under src/
```

Returns a `list[str]` of matching paths. No matches → empty list. An invalid pattern is a runtime error.

Supported pattern syntax: `*` (any chars in one segment), `?` (single char), `**` (zero or more segments, recursive).

## Temporary files and directories

```keel
tmp     = File.mktemp()            # creates a temp file; returns its path as str
tmpdir  = File.mktemp(dir: true)   # creates a temp directory; returns its path
```

`File.mktemp` creates the file or directory immediately and returns the path. **Lifecycle is the caller's responsibility** — use `File.remove(path)` when done:

```keel
tmp = File.mktemp()
File.write(tmp, processed_content)
result = File.read(tmp)
File.remove(tmp)
```

## Quick reference

| Method | Returns | Notes |
|---|---|---|
| `File.read(path)` | `str` | raises `FileError` if file missing |
| `File.write(path, content)` | `none` | creates parent dirs |
| `File.exists(path)` | `bool` | true for files and directories |
| `File.list(dir)` | `list[str]` | entry names only, not paths |
| `File.mkdir(path)` | `none` | creates all missing parents |
| `File.copy(src, dst)` | `none` | creates dst parent dirs |
| `File.move(src, dst)` | `none` | atomic rename; creates dst parent dirs |
| `File.remove(path)` | `none` | recursive for directories |
| `File.glob(pattern)` | `list[str]` | empty list if no match |
| `File.mktemp()` | `str` | temp file path; caller removes it |
| `File.mktemp(dir: true)` | `str` | temp directory path; caller removes it |

---

For subprocess execution, see [Shell — subprocess bridge](./shell.md).
