---
title: "time.system_timezone"
description: "Function time.system_timezone from the generated baml package reference."
---

Returns the system's IANA timezone identifier, for example
`"America/Los_Angeles"`. Mirrors `Temporal.Now.timeZoneId()`.

An IO function so hosts can swap the system-state source out.
Throws if the host cannot determine its timezone.

```baml
function time.system_timezone() -> string throws baml.errors.Io
```

_Source: `<builtin>/baml/ns_time/timezone.baml:4178`_
