---
title: "csv.CsvReader"
description: "Class csv.CsvReader from the generated baml package reference."
---

A streaming CSV reader. Create one with `baml.csv.open` or
`baml.csv.reader`.

`CsvReader` is a lazy iterator over `CsvRecord` values: a multi-gigabyte
file parses in constant memory, and every `baml.iter` adapter and default
method (`map`, `filter`, `collect`, ...) works on it. There is one
cursor; raw iteration, `rows<T>()`, and `headers()` all draw from the
same stream.

A thrown record error has already consumed the offending record: the
parser resynchronizes at the next record boundary and the next call to
`next()` continues. One corrupt cell costs exactly one record.

```baml
class csv.CsvReader
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

### _file

```baml
_file: baml.fs.File | null
```

No description is available yet.

### _on_skip

```baml
_on_skip: ((baml.csv.CsvError) -> null throws never) | null
```

No description is available yet.

### _owns_file

```baml
_owns_file: bool
```

No description is available yet.

## Methods

### _feed

```baml
function _feed(self: baml.csv.CsvReader, chunk: uint8array) -> null
```

No description is available yet.

### _feed_eof

```baml
function _feed_eof(self: baml.csv.CsvReader) -> null
```

No description is available yet.

### _mark_closed

```baml
function _mark_closed(self: baml.csv.CsvReader) -> null
```

No description is available yet.

### _mark_exhausted

```baml
function _mark_exhausted(self: baml.csv.CsvReader) -> null
```

No description is available yet.

### _poll

```baml
function _poll(self: baml.csv.CsvReader) -> baml.csv.CsvRecord | baml.csv.CsvSkip | baml.csv.CsvNeedData | baml.iter.Done throws baml.csv.CsvError
```

No description is available yet.

### _poll_headers

```baml
function _poll_headers(self: baml.csv.CsvReader) -> baml.csv.CsvHeaders | baml.csv.CsvNeedData throws baml.csv.CsvError
```

No description is available yet.

### _read_chunk

```baml
function _read_chunk(self: baml.csv.CsvReader) -> void throws baml.errors.Io
```

(internal) Pull the next chunk from the backing file, or mark EOF for
in-memory sources. IO errors are stream-scoped and fatal: the reader is
marked exhausted before the throw, so subsequent `next()` calls return
`baml.iter.Done`. A closed handle arrives as an IO error because
`baml.io.Read` has no separate closed-handle error channel.

### close

```baml
function close(self: baml.csv.CsvReader) -> null throws baml.errors.Io
```

Releases the reader. Idempotent. The underlying file handle is closed
only when the reader owns it (created by `open`); a handle passed to
`reader(file)` stays open — its lifecycle belongs to the caller.
Subsequent `next()`, `headers()`, and `rows<T>()` calls throw
`CsvError { kind: Closed }`; `skipped()`, `skipped_count()`, and
`position()` remain valid. As with `baml.fs.File`, closing is optional:
the GC reclaims unreferenced readers.

### headers

```baml
function headers(self: baml.csv.CsvReader) -> string[] | null throws baml.csv.CsvError | baml.errors.Io
```

The column names: the `headers` option when set, the file's header row
when `has_header = true` (consumed on first call), and `null` otherwise.

Parse errors in the header record (`Quote`, `Encoding`) surface from
this call (or from whichever call reads the header first), and they
exhaust the reader: without a trustworthy header every subsequent
name-to-column mapping would be silently wrong, so the stream ends
rather than mis-mapping a million rows.

### position

```baml
function position(self: baml.csv.CsvReader) -> baml.csv.CsvPosition
```

No description is available yet.

### rows

```baml
function rows<T>(self: baml.csv.CsvReader) -> baml.iter.Iterator<Error = baml.csv.CsvError | baml.errors.Io, Item = T> throws baml.csv.CsvError | baml.errors.Io
```

A typed iterator over the remaining records, decoding each into `T`.

Validates eagerly: reads the header if necessary and checks that every
non-optional field of `T` is satisfiable, throwing
`CsvError { kind: Header }` at the call site rather than on the
millionth row.

### skipped

```baml
function skipped(self: baml.csv.CsvReader) -> baml.csv.CsvError[]
```

No description is available yet.

### skipped_count

```baml
function skipped_count(self: baml.csv.CsvReader) -> int
```

No description is available yet.

_Source: `<builtin>/baml/ns_csv/csv.baml:9783`_
