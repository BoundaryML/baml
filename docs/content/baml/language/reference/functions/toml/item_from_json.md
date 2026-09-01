---
title: "toml.item_from_json"
description: "Function toml.item_from_json from the generated baml package reference."
---

Errors if the JSON value is `null`.
Usually `Table.from_json` is a better entrypoint, as it will skip null values without erroring.

```baml
function toml.item_from_json(json: baml.json.json) -> baml.toml.Item throws baml.json.JsonDecodeError
```

_Source: `<builtin>/baml/ns_toml/toml.baml:3403`_
