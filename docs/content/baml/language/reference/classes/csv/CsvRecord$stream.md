---
title: "csv.CsvRecord$stream"
description: "Class csv.CsvRecord$stream from the generated baml package reference."
---

One raw CSV record. Yielded by iterating a `CsvReader`.

Records have snapshot semantics: they remain valid after the reader
advances. Cell access is lazy — `get<int>("amount")` converts one cell
without touching the others.

```baml
class csv.CsvRecord$stream
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

_Source: `<builtin>/baml/ns_csv/csv.baml:0`_
