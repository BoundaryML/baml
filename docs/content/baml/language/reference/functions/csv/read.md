---
title: "csv.read"
description: "Function csv.read from the generated baml package reference."
---

Eager typed read of a whole file: `open(path).rows<T>().collect()`.

If the file might not fit in memory, `open` it and stream instead — the
options are the same; the streaming spelling is a verb swap away.

```baml
function csv.read<T>(path: string, options: baml.csv.ReaderOptions | null) -> T[] throws baml.csv.CsvError | baml.errors.Io
```

_Source: `<builtin>/baml/ns_csv/csv.baml:22901`_
