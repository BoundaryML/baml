---
title: "csv.create"
description: "Function csv.create from the generated baml package reference."
---

Creates or truncates the file at `path` (auto-creating parent
directories, like `baml.fs.write`) and stream-writes to it.

```baml
function csv.create(path: string, options: baml.csv.WriterOptions | null) -> baml.csv.CsvWriter throws baml.csv.CsvError | baml.errors.Io
```

_Source: `<builtin>/baml/ns_csv/csv.baml:26313`_
