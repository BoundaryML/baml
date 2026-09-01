---
title: "time._tz_to_instant"
description: "Function time._tz_to_instant from the generated baml package reference."
---

Internal: locates the civil ("wall-clock") reading `civil_ns` (nanoseconds
since 1970-01-01T00:00:00, interpreted as if UTC) in the IANA timezone
`timezone`, resolving DST gaps/overlaps per `disambiguation` (which must
be `"compatible"`, `"earlier"`, or `"later"` — `"reject"` is implemented
by callers as: resolve both `"earlier"` and `"later"` and require them to
agree). Returns the resolved absolute time as nanoseconds since the Unix
epoch, or `null` if the identifier is unknown to the host's timezone
database.

```baml
function time._tz_to_instant(timezone: string, civil_ns: bigint, disambiguation: string) -> bigint | null throws baml.errors.Io
```

_Source: `<builtin>/baml/ns_time/timezone.baml:5156`_
