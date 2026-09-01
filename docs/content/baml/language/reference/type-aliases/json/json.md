---
title: "json.json"
description: "Type alias json.json from the generated baml package reference."
---

The BAML JSON value type — a union of all JSON-representable values.

```baml
type json.json = null | bool | int | float | string | baml.json.json[] | map<string, baml.json.json>
```

_Source: `<builtin>/baml/ns_json/json.baml:75`_
