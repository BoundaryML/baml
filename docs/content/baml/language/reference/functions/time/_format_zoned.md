---
title: "time._format_zoned"
description: "Function time._format_zoned from the generated baml package reference."
---

Internal: formats an absolute time as RFC 3339 at the given offset,
appending an RFC 9557 `[iana]` annotation when `iana` is non-null.

```baml
function time._format_zoned(epoch_ns: bigint, offset_ns: int, iana: string | null) -> string throws baml.errors.InvalidArgument
```

_Source: `<builtin>/baml/ns_time/zoneddatetime.baml:11107`_
