---
title: "csv.open"
description: "Function csv.open from the generated baml package reference."
---

Opens the file at `path` via `baml.fs` and streams it.

```baml
function csv.open(path: string, options: baml.csv.ReaderOptions | null) -> baml.csv.CsvReader throws baml.csv.CsvError | baml.errors.Io
```

_Source: `<builtin>/baml/ns_csv/csv.baml:22376`_
