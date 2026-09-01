---
title: "csv.reader"
description: "Function csv.reader from the generated baml package reference."
---

A streaming reader over in-memory text/bytes or an open file handle.

A `string` or `uint8array` source is CSV content, parsed in place. A
`baml.fs.File` source is streamed from its current cursor to EOF; from
construction until the reader is exhausted or closed, the reader owns
that cursor.

```baml
function csv.reader(source: string | uint8array | baml.fs.File, options: baml.csv.ReaderOptions | null) -> baml.csv.CsvReader throws baml.csv.CsvError
```

_Source: `<builtin>/baml/ns_csv/csv.baml:22144`_
