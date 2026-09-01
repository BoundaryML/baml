---
title: "csv.ReaderOptions"
description: "Class csv.ReaderOptions from the generated baml package reference."
---

Options for CSV readers. All fields are optional; `null` means default.

`delimiter`, `quote`, `escape`, and `comment` must be single ASCII bytes
and mutually distinct; violations throw `CsvError { kind: Options }` at
construction.

```baml
class csv.ReaderOptions
```

## Fields

### delimiter

```baml
delimiter: string | null
```

Field separator. Exactly one ASCII byte. Default `","` (`"\t"` for TSV).

### quote

```baml
quote: string | null
```

Quote character. Exactly one ASCII byte. Default `"\""`.

### quoting

```baml
quoting: bool | null
```

`false` treats quote bytes as ordinary data. Default `true`.

### escape

```baml
escape: string | null
```

Escape byte for quotes. `null` (default) means RFC 4180 `""` doubling;
setting it switches to escape-character dialects (`"\\"` for MySQL-style
dumps).

### has_header

```baml
has_header: bool | null
```

First record is the header. Default `true`.

### headers

```baml
headers: string[] | null
```

Supply (`has_header = false`) or replace (`has_header = true`; the
file's header row is consumed) column names.

### comment

```baml
comment: string | null
```

Records whose first byte (outside quotes) matches are skipped. Off by
default — `#` is valid data.

### trim

```baml
trim: "none" | "headers" | "fields" | "all" | null
```

Strip leading/trailing ASCII space and tab from header and/or unquoted
field values. Default `"none"`.

### skip_lines

```baml
skip_lines: int | null
```

Raw lines dropped before parsing begins (preamble junk, Excel `sep=;`
lines, a Markdown code fence). Default `0`.

### skip_blank_records

```baml
skip_blank_records: bool | null
```

Drop blank records. Default `true`.

### ragged

```baml
ragged: "strict" | "pad" | "truncate" | null
```

Ragged-record policy. `"strict"` (default): width mismatch throws
`FieldCount`. `"pad"`: short records padded with empty cells (long ones
still throw). `"truncate"`: long records cut to the expected width
(short ones still throw).

### null_values

```baml
null_values: string[] | null
```

Unquoted cell texts (after trim) treated as null cells. Default `[]`.

### encoding

```baml
encoding: "utf8" | "utf8-lossy" | null
```

`"utf8"` (default) throws `Encoding` on invalid bytes; `"utf8-lossy"`
substitutes U+FFFD.

### bom

```baml
bom: "strip" | "keep" | null
```

A leading UTF-8 BOM is stripped by default so it never corrupts the
first header name.

### on_error

```baml
on_error: "throw" | "skip" | null
```

Per-record error policy. `"throw"` (default): `next()` throws a
`CsvError` for a malformed record and the reader resumes at the next
record. `"skip"`: record-scoped errors are counted and skipped.

### on_skip

```baml
on_skip: ((baml.csv.CsvError) -> null throws never) | null
```

Observer invoked with each skipped record's `CsvError` when
`on_error = "skip"`. Constant-memory alternative to `skipped()`.

### max_skipped

```baml
max_skipped: int | null
```

How many skipped `CsvError` values `skipped()` retains. Default 1000.
`0` disables retention. `skipped_count()` stays exact regardless.

### limit

```baml
limit: int | null
```

Stop after this many data records. Default unlimited.

_Source: `<builtin>/baml/ns_csv/csv.baml:2547`_
