---
title: "csv.writer"
description: "Function csv.writer from the generated baml package reference."
---

A streaming writer to an open file handle. Emits from the file's current
cursor and never truncates; for fresh output files, use `create`.

```baml
function csv.writer(file: baml.fs.File, options: baml.csv.WriterOptions | null) -> baml.csv.CsvWriter throws baml.csv.CsvError
```

_Source: `<builtin>/baml/ns_csv/csv.baml:26047`_
