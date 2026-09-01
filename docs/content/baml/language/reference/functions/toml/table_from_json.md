---
title: "toml.table_from_json"
description: "Function toml.table_from_json from the generated baml package reference."
---

Decodes a JSON object into a `Table`, skipping null values. Returns a
concrete `Table` so arm-reachability over the recursive `json` map pattern
is analyzed without the abstract `Self` of `Table`'s `baml.FromJson` impl.

```baml
function toml.table_from_json(j: baml.json.json) -> baml.toml.Table throws baml.json.JsonDecodeError
```

_Source: `<builtin>/baml/ns_toml/toml.baml:1977`_
