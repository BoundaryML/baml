---
title: "toml.Item$stream"
description: "Type alias toml.Item$stream from the generated baml package reference."
---

TOML's four datetime kinds map losslessly onto the `baml.time` types
(see BEP-021): offset datetime → `ZonedDateTime` (fixed offset), local
datetime → `PlainDateTime`, local date → `PlainDate`, local time →
`PlainTime`.

```baml
type toml.Item$stream = baml.toml.Table$stream | baml.toml.Item$stream[] | bool | int | float | string | baml.time.ZonedDateTime$stream | baml.time.PlainDateTime$stream | baml.time.PlainDate$stream | baml.time.PlainTime$stream
```

_Source: `<builtin>/baml/ns_toml/toml.baml:0`_
