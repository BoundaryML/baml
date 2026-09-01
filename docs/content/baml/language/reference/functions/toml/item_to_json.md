---
title: "toml.item_to_json"
description: "Function toml.item_to_json from the generated baml package reference."
---

Converts a TOML item to a JSON value.

This is lossy: TOML datetime values are converted to JSON strings
(`ZonedDateTime` as RFC 3339, the `Plain*` types as zoneless ISO 8601).

```baml
function toml.item_to_json(item: baml.toml.Item) -> baml.json.json throws baml.json.JsonSerializationError
```

_Source: `<builtin>/baml/ns_toml/toml.baml:2825`_
