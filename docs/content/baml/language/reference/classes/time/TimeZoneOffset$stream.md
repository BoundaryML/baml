---
title: "time.TimeZoneOffset$stream"
description: "Class time.TimeZoneOffset$stream from the generated baml package reference."
---

A fixed offset from UTC, with a permitted range of ±24 hours (real
timezones currently range from -12 to +14 hours).

Note that unlike IANA identifiers, a `TimeZoneOffset` does not change
based on daylight savings. Sometimes this is desirable, but other times
it is not, so `ZonedDateTime` permits either.

```baml
class time.TimeZoneOffset$stream
```

## Fields

### _nanoseconds

```baml
_nanoseconds: int | null
```

Internal representation: the offset in nanoseconds. Invariant: within
±24 hours.

_Source: `<builtin>/baml/ns_time/timezone.baml:0`_
