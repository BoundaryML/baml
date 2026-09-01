---
title: "json.path"
description: "Function json.path from the generated baml package reference."
---

Navigate a jq-style selector into a JSON value and coerce the leaf to `T`.
Selectors start with `.` or `[`. Dot and quoted-bracket steps address object
fields; integer bracket steps address arrays.

```baml
function json.path<T>(j: baml.json.json, selector: string) -> T throws baml.json.JsonPathError
```

_Source: `<builtin>/baml/ns_json/json.baml:8040`_
