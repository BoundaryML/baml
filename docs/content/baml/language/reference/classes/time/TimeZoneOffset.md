---
title: "time.TimeZoneOffset"
description: "Class time.TimeZoneOffset from the generated baml package reference."
---

A fixed offset from UTC, with a permitted range of ±24 hours (real
timezones currently range from -12 to +14 hours).

Note that unlike IANA identifiers, a `TimeZoneOffset` does not change
based on daylight savings. Sometimes this is desirable, but other times
it is not, so `ZonedDateTime` permits either.

```baml
class time.TimeZoneOffset
```

## Fields

### _nanoseconds

```baml
_nanoseconds: int
```

Internal representation: the offset in nanoseconds. Invariant: within
±24 hours.

## Methods

### from_duration

```baml
function from_duration(duration: baml.time.Duration) -> baml.time.TimeZoneOffset throws baml.errors.InvalidArgument
```

No description is available yet.

### from_timezone

```baml
function from_timezone(timezone: string, at: baml.time.Instant) -> baml.time.TimeZoneOffset throws baml.time.UnknownTimezoneError | baml.errors.Io
```

Resolves an IANA timezone identifier (e.g. `"America/Los_Angeles"`)
to its concrete offset at the absolute time `at` (DST-aware).

Resolution uses the host's timezone database.

### hours

```baml
function hours(self: baml.time.TimeZoneOffset) -> int
```

The hour component of the offset. Rounded toward zero.

### local

```baml
function local() -> baml.time.TimeZoneOffset throws baml.time.UnknownTimezoneError | baml.errors.Io
```

Returns the local timezone offset right now.
Note that even in the same location, this may vary over time based on
daylight savings.

### minutes

```baml
function minutes(self: baml.time.TimeZoneOffset) -> int
```

The minute component of the offset, modulo one hour. Rounded toward zero.

### new

```baml
function new(hours: int, minutes: int) -> baml.time.TimeZoneOffset throws baml.errors.InvalidArgument
```

Creates a `TimeZoneOffset` from hours and minutes east of UTC.
Both components must carry the same sign (e.g. `new(-7, 0)`,
`new(5, 30)`, `new(-9, -30)`).

Throws `root.errors.InvalidArgument` if the signs differ or the
result exceeds ±24 hours.

### to_duration

```baml
function to_duration(self: baml.time.TimeZoneOffset) -> baml.time.Duration
```

No description is available yet.

### utc

```baml
function utc() -> baml.time.TimeZoneOffset
```

The UTC (zero) offset.

_Source: `<builtin>/baml/ns_time/timezone.baml:980`_
