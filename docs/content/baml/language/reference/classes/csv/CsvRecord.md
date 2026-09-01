---
title: "csv.CsvRecord"
description: "Class csv.CsvRecord from the generated baml package reference."
---

One raw CSV record. Yielded by iterating a `CsvReader`.

Records have snapshot semantics: they remain valid after the reader
advances. Cell access is lazy — `get<int>("amount")` converts one cell
without touching the others.

```baml
class csv.CsvRecord
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

## Methods

### decode

```baml
function decode<T>(self: baml.csv.CsvRecord) -> T throws baml.csv.CsvError
```

Applies the full typed-decode rules to this one record — useful for
routing heterogeneous rows. Always throws on failure, regardless of
the reader's `on_error` policy.

### fields

```baml
function fields(self: baml.csv.CsvRecord) -> string[]
```

No description is available yet.

### get

```baml
function get<T>(self: baml.csv.CsvRecord, column: string) -> T | null throws baml.csv.CsvError
```

Converts the cell under header `column` to `T`.

Returns `null` for a column name absent from the headers, a missing
cell, or a null cell. Throws `CsvError { kind: Decode }` when a cell
exists but cannot convert to `T`, and `CsvError { kind: Header }` when
the name is duplicated in the header or when name-based access is used
with no headers at all.

### get_at

```baml
function get_at<T>(self: baml.csv.CsvRecord, index: int) -> T | null throws baml.csv.CsvError
```

Converts the cell at `index` to `T`. Returns `null` for a missing or
null cell; throws `CsvError { kind: Decode }` on conversion failure.

### length

```baml
function length(self: baml.csv.CsvRecord) -> int
```

No description is available yet.

### position

```baml
function position(self: baml.csv.CsvRecord) -> baml.csv.CsvPosition
```

No description is available yet.

### to_map

```baml
function to_map(self: baml.csv.CsvRecord) -> map<string, string> throws baml.csv.CsvError
```

No description is available yet.

_Source: `<builtin>/baml/ns_csv/csv.baml:7314`_
