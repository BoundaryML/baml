---
title: "csv.CsvErrorKind"
description: "Enum csv.CsvErrorKind from the generated baml package reference."
---

Classifies a `CsvError` for portable handling.

```baml
enum csv.CsvErrorKind
```

## Variants

### Options

Invalid configuration, or an operation the handle does not support.

### Quote

Unterminated or stray quote in a record.

### FieldCount

Record width differs from the expected width (see `ReaderOptions.ragged`).

### Encoding

Invalid UTF-8 in a record (`encoding = "utf8"`).

### Header

Missing, duplicate, or unusable header for the requested operation.

### Decode

A cell could not convert to the requested type.

### Encode

A value not representable under the writer options.

### NotFound

`decode_one` on an input with zero data records.

### TooManyRows

`decode_one` / `decode_optional` on an input with extra records.

### Closed

Operation on a closed reader or writer.

_Source: `<builtin>/baml/ns_csv/csv.baml:623`_
