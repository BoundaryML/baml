---
title: "csv.CsvReader$stream"
description: "Class csv.CsvReader$stream from the generated baml package reference."
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
class csv.CsvReader$stream
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

### _file

```baml
_file: baml.fs.File$stream | null
```

No description is available yet.

### _on_skip

```baml
_on_skip: unknown | null
```

No description is available yet.

### _owns_file

```baml
_owns_file: bool | null
```

No description is available yet.

_Source: `<builtin>/baml/ns_csv/csv.baml:0`_
