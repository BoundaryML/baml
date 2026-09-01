---
title: "csv.decode_optional"
description: "Function csv.decode_optional from the generated baml package reference."
---

Typed parse of an input with zero or one data record. Extra records
throw `CsvError { kind: TooManyRows }`.

```baml
function csv.decode_optional<T>(source: string | uint8array, options: baml.csv.ReaderOptions | null) -> T | null throws baml.csv.CsvError
```

_Source: `<builtin>/baml/ns_csv/csv.baml:24354`_
