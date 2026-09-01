---
title: "csv.write"
description: "Function csv.write from the generated baml package reference."
---

One-shot typed file write; returns bytes written, mirroring
`baml.fs.write` (creates or truncates).

```baml
function csv.write<T>(path: string, rows: T[], options: baml.csv.WriterOptions | null) -> int throws baml.csv.CsvError | baml.errors.Io
```

_Source: `<builtin>/baml/ns_csv/csv.baml:26906`_
