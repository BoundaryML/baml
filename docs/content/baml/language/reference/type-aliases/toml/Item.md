---
title: "toml.Item"
description: "Type alias toml.Item from the generated baml package reference."
---

TOML's four datetime kinds map losslessly onto the `baml.time` types
(see BEP-021): offset datetime → `ZonedDateTime` (fixed offset), local
datetime → `PlainDateTime`, local date → `PlainDate`, local time →
`PlainTime`.

```baml
type toml.Item = baml.toml.Table | baml.toml.Item[] | bool | int | float | string | baml.time.ZonedDateTime | baml.time.PlainDateTime | baml.time.PlainDate | baml.time.PlainTime
```

_Source: `<builtin>/baml/ns_toml/toml.baml:244`_
