---
id: BEP-009
title: "Filesystem API"
shepherds: Antonio Sarosi <sarosiantonio@gmail.com>
status: Proposed
created: 2026-04-15
---

# BEP-009: Filesystem API

## Summary

Extend BAML's `baml.fs` namespace with write operations, random-access I/O, and
Bun-style naming. This is a breaking change that renames existing methods.

## Motivation

BAML's current filesystem support is read-only: `baml.fs.open(path)` returns a
`File` with `read_string()` and `read_bytes()`. Users cannot write files, perform
random-access I/O, or use familiar naming from Bun's API.

A key driver for this proposal is porting a Rust database implementation to BAML
for our benchmark suite. This requires seek and read-write mode to perform
block-level I/O (e.g. B-tree page reads and writes at arbitrary offsets). Bun's
filesystem API does not offer seek or open modes, so we extend the Bun-style
naming with Node.js-style open modes (`"r"`, `"r+"`) and `File.seek()` to cover
this use case.

This proposal adds write operations (`baml.fs.write()`), random-access I/O
(`File.seek()`, `File.write()`) via Node.js-style open modes (`"r"`, `"r+"`),
and renames existing methods to match Bun's conventions.

## Proposed Design

### API Surface

| Operation | Signature | Description |
|-----------|-----------|-------------|
| `baml.fs.file(path, mode)` | `(string, "r" \| "r+") -> File throws Io` | Open file. `"r"` read-only, `"r+"` read-write |
| `File.text()` | `(self) -> string throws Io` | Read entire file as UTF-8 string |
| `File.bytes()` | `(self) -> uint8array throws Io` | Read entire file as bytes |
| `File.close()` | `(self) -> null throws Io` | Close file (no-op, GC handles cleanup) |
| `File.seek(offset)` | `(self, int) -> null throws Io` | Seek to byte offset from start |
| `File.write(data)` | `(self, string \| uint8array) -> int throws Io` | Write at current position, return bytes written |
| `baml.fs.write(path, data)` | `(string, string \| uint8array) -> int throws Io` | One-shot write to file, return bytes written |

### Syntax

```baml
class File {
  _handle $rust_type

  function text(self) -> string throws root.errors.Io { $rust_io_function }
  function bytes(self) -> uint8array throws root.errors.Io { $rust_io_function }
  function close(self) -> null throws root.errors.Io { $rust_io_function }
  function seek(self, offset: int) -> null throws root.errors.Io { $rust_io_function }
  function write(self, data: string | uint8array) -> int throws root.errors.Io { $rust_io_function }
}

function file(path: string, mode: "r" | "r+") -> File throws root.errors.Io { $rust_io_function }
function write(path: string, data: string | uint8array) -> int throws root.errors.Io { $rust_io_function }
```

### Semantics

- **`file(path, mode)`**: `"r"` opens read-only (error if missing), `"r+"` opens
  read-write (error if missing). Other modes are rejected at compile time via
  the `"r" | "r+"` literal type.
- **`write(path, data)`**: Always creates or truncates. Auto-creates parent
  directories. Returns bytes written. `data` is `string | uint8array`.
- **`File.seek(offset)`**: Sets cursor to absolute byte offset from start.
- **`File.write(data)`**: Writes at current cursor position. Errors on files
  opened with `"r"`. `data` is `string | uint8array`.
- **Error handling**: All operations throw `root.errors.Io`.

### Backwards Compatibility

**Breaking changes:**
- `baml.fs.open(path)` renamed to `baml.fs.file(path, mode)` (added mode parameter)
- `File.read_string()` renamed to `File.text()`
- `File.read_bytes()` renamed to `File.bytes()`

## Alternatives Considered

- **`slice()` instead of `seek()`**: Bun uses `slice(begin, end)` for subrange reads.
  We chose `seek()` because it supports both reads and writes at arbitrary offsets,
  enabling random-access patterns like B-tree block I/O.
- **Write as File method**: Bun's `Bun.write()` is a standalone function. We follow
  this for one-shot writes but also add `File.write()` for random-access use cases.
- **Separate `write` / `write_bytes` functions**: Instead of two functions, we use
  a single `write` with a `string | uint8array` union parameter, matching Bun's
  approach of accepting multiple data types in one function.

## Open Questions

- **`baml.fs.*` vs `baml.*` namespace**: Bun places file operations directly on the
  top-level object (`Bun.file()`, `Bun.write()`) with no `fs` sub-namespace. Should
  we follow suit and use `baml.file()` / `baml.write()` instead of `baml.fs.file()` /
  `baml.fs.write()`? Flattening is closer to Bun's API and shorter to type, but a
  sub-namespace keeps `baml.*` cleaner as more builtins are added (env, net, etc.).
