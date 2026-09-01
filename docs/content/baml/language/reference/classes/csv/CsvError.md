---
title: "csv.CsvError"
description: "Class csv.CsvError from the generated baml package reference."
---

A structured CSV error with positional diagnostics.

`kind` is for portable handling; `line` / `record` / `field` / `column`
make "row 41,283 has 7 fields, expected 8" diagnostics cheap.

```baml
class csv.CsvError
```

## Fields

### kind

```baml
kind: baml.csv.CsvErrorKind
```

No description is available yet.

### message

```baml
message: string
```

No description is available yet.

### line

```baml
line: int | null
```

1-based line in the source where the offending record starts.

### record

```baml
record: int | null
```

0-based data-record index (header excluded).

### field

```baml
field: int | null
```

0-based field index within the record.

### column

```baml
column: string | null
```

Header name, when known.

### expected

```baml
expected: int | null
```

`FieldCount`: expected width.

### found

```baml
found: int | null
```

`FieldCount`: actual width.

_Source: `<builtin>/baml/ns_csv/csv.baml:1616`_
