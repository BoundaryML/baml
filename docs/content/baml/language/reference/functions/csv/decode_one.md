---
title: "csv.decode_one"
description: "Function csv.decode_one from the generated baml package reference."
---

Typed parse of an input with exactly one data record. Zero records throw
`CsvError { kind: NotFound }`; extra records throw
`CsvError { kind: TooManyRows }`.

Fits LLM output that was requested as exactly one row — with
`has_header: false` for a bare row (positional decode), or default
options when the model was asked to emit a header plus one data row.

```baml
function csv.decode_one<T>(source: string | uint8array, options: baml.csv.ReaderOptions | null) -> T throws baml.csv.CsvError
```

_Source: `<builtin>/baml/ns_csv/csv.baml:25094`_
