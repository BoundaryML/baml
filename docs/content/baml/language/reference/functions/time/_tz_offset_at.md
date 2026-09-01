---
title: "time._tz_offset_at"
description: "Function time._tz_offset_at from the generated baml package reference."
---

Internal: resolves an IANA identifier to its offset (in nanoseconds) at
the absolute time `at_ns` (nanoseconds since the Unix epoch), using the
host's timezone database. Returns `null` if the identifier is unknown.

```baml
function time._tz_offset_at(timezone: string, at_ns: bigint) -> int | null throws baml.errors.Io
```

_Source: `<builtin>/baml/ns_time/timezone.baml:4491`_
