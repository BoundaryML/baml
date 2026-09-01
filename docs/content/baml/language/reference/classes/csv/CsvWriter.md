---
title: "csv.CsvWriter"
description: "Class csv.CsvWriter from the generated baml package reference."
---

A streaming CSV writer. Create one with `baml.csv.create`,
`baml.csv.writer`, or `baml.csv.buffer`.

```baml
class csv.CsvWriter
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

### _owns_file

```baml
_owns_file: bool
```

No description is available yet.

## Methods

### _bytes_written

```baml
function _bytes_written(self: baml.csv.CsvWriter) -> int
```

No description is available yet.

### _emit

```baml
function _emit(self: baml.csv.CsvWriter, out: string) -> null throws baml.errors.Io
```

(internal) Write encoded text to the backing file, if any. Buffer
writers accumulate natively and receive `""` here.

### _encode_header

```baml
function _encode_header(self: baml.csv.CsvWriter, names: string[]) -> string throws baml.csv.CsvError
```

No description is available yet.

### _encode_record

```baml
function _encode_record(self: baml.csv.CsvWriter, record: baml.csv.CsvValue[]) -> string throws baml.csv.CsvError
```

No description is available yet.

### _mark_closed

```baml
function _mark_closed(self: baml.csv.CsvWriter) -> null
```

No description is available yet.

### close

```baml
function close(self: baml.csv.CsvWriter) -> null throws baml.errors.Io
```

Flushes and releases the writer. Idempotent. The underlying file
handle is closed only when the writer owns it (created by `create`); a
handle passed to `writer(file)` stays open — its lifecycle belongs to
the caller. Writes after close throw `CsvError { kind: Closed }`;
`records_written()` remains valid.

### flush

```baml
function flush(self: baml.csv.CsvWriter) -> null
```

Flushes pending output. Writes through `baml.fs` flush eagerly, so this
is an infallible no-op kept for symmetry with other writer APIs.

### records_written

```baml
function records_written(self: baml.csv.CsvWriter) -> int
```

No description is available yet.

### text

```baml
function text(self: baml.csv.CsvWriter) -> string throws baml.csv.CsvError
```

No description is available yet.

### write_header

```baml
function write_header(self: baml.csv.CsvWriter, names: string[]) -> null throws baml.csv.CsvError | baml.errors.Io
```

Writes an explicit header record. Throws `CsvError { kind: Header }`
after data has been written.

### write_record

```baml
function write_record(self: baml.csv.CsvWriter, record: baml.csv.CsvValue[]) -> null throws baml.csv.CsvError | baml.errors.Io
```

Writes one raw record. Never triggers an automatic header.

### write_row

```baml
function write_row<T>(self: baml.csv.CsvWriter, row: T) -> null throws baml.csv.CsvError | baml.errors.Io
```

Writes one typed row. Fields serialize in declaration order; if
`write_header` is enabled (default) and no header has been written yet,
the header derived from `T`'s field names (or `WriterOptions.headers`)
is emitted first.

### write_rows

```baml
function write_rows<T>(self: baml.csv.CsvWriter, rows: T[]) -> null throws baml.csv.CsvError | baml.errors.Io
```

Writes many typed rows, as one atomic batch: on a thrown error nothing
from the batch is emitted or counted. Otherwise equivalent to
`write_row<T>` in a loop.

_Source: `<builtin>/baml/ns_csv/csv.baml:17784`_
