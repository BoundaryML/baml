---
title: "time.ZonedDateTime"
description: "Class time.ZonedDateTime from the generated baml package reference."
---

A timezone-aware point in time: an absolute instant plus a timezone —
either a fixed offset (`-07:00`) or an IANA identifier
(`America/Los_Angeles`).

The timezone does not affect the absolute time; it affects the
interpretation of date/time components and string formatting. The
internal representation is an absolute time (like `Instant`) rather than
calendar components, so values are unambiguous across DST transitions.

Equivalent to `Temporal.ZonedDateTime` (TC39).

```baml
class time.ZonedDateTime
```

## Fields

### _nanoseconds

```baml
_nanoseconds: bigint
```

Internal representation: nanoseconds since the Unix epoch (an inlined
`Instant`).

### _offset_ns

```baml
_offset_ns: int | null
```

Internal: the fixed offset in nanoseconds, if the timezone is a fixed
offset. Invariant: exactly one of `_offset_ns` / `_iana` is non-null.

### _iana

```baml
_iana: string | null
```

Internal: the IANA identifier, if the timezone is one.

## Methods

### _to_string_impl

```baml
function _to_string_impl(self: baml.time.ZonedDateTime) -> string throws baml.errors.InvalidArgument | baml.time.UnknownTimezoneError | baml.errors.Io
```

Internal RFC 9557 / RFC 3339 formatter. Throws when the value cannot be
formatted; `baml.ToString.to_string` turns that into a panic, while
`to_json` surfaces it as a `JsonSerializationError`.

### day

```baml
function day(self: baml.time.ZonedDateTime) -> int throws baml.errors.InvalidArgument | baml.time.UnknownTimezoneError | baml.errors.Io
```

The day of the month, in `[1, 31]`, resolved through the timezone.

### from_components

```baml
function from_components(timezone: baml.time.TimeZoneOffset | string, year: int, month: int, day: int, hour: int, minute: int, second: int, millisecond: int, microsecond: int, nanosecond: int, disambiguation: baml.time.Disambiguation) -> baml.time.ZonedDateTime throws baml.errors.InvalidArgument | baml.time.UnknownTimezoneError | baml.time.AmbiguousTimeError | baml.errors.Io
```

Creates a `ZonedDateTime` from calendar/clock components read in
`timezone`. `month` and `day` are 1-based. Defaulted clock components
and `disambiguation` are passed by name. With an IANA timezone, DST
gaps/overlaps are resolved per `disambiguation` (see `Disambiguation`).

### from_instant

```baml
function from_instant(instant: baml.time.Instant, timezone: baml.time.TimeZoneOffset | string) -> baml.time.ZonedDateTime
```

Pairs an absolute time with a timezone.

### hour

```baml
function hour(self: baml.time.ZonedDateTime) -> int throws baml.errors.InvalidArgument | baml.time.UnknownTimezoneError | baml.errors.Io
```

The hour of the day, in `[0, 23]`, resolved through the timezone.

### max

```baml
function max(self: baml.time.ZonedDateTime, other: baml.time.ZonedDateTime) -> baml.time.ZonedDateTime
```

If `self` is before `other` (absolute-time comparison), returns
`other`. Otherwise, returns `self`.

### millisecond

```baml
function millisecond(self: baml.time.ZonedDateTime) -> int throws baml.errors.InvalidArgument | baml.time.UnknownTimezoneError | baml.errors.Io
```

The millisecond of the second, in `[0, 999]`, resolved through the
timezone.

### min

```baml
function min(self: baml.time.ZonedDateTime, other: baml.time.ZonedDateTime) -> baml.time.ZonedDateTime
```

If `self` is after `other` (absolute-time comparison), returns
`other`. Otherwise, returns `self`.

### minute

```baml
function minute(self: baml.time.ZonedDateTime) -> int throws baml.errors.InvalidArgument | baml.time.UnknownTimezoneError | baml.errors.Io
```

The minute of the hour, in `[0, 59]`, resolved through the timezone.

### month

```baml
function month(self: baml.time.ZonedDateTime) -> int throws baml.errors.InvalidArgument | baml.time.UnknownTimezoneError | baml.errors.Io
```

The calendar month, in `[1, 12]`, resolved through the timezone.

### now

```baml
function now() -> baml.time.ZonedDateTime throws baml.errors.Io
```

Creates a new `ZonedDateTime` with the current time in the system
timezone (an IANA identifier, e.g. `"America/Los_Angeles"`).
Mirrors `Temporal.Now.zonedDateTimeISO()`.

### now_in

```baml
function now_in(timezone: baml.time.TimeZoneOffset | string) -> baml.time.ZonedDateTime
```

Creates a new `ZonedDateTime` with the current time in the given
timezone.

### parse

```baml
function parse(s: string) -> baml.time.ZonedDateTime throws baml.errors.ParseError
```

No description is available yet.

### second

```baml
function second(self: baml.time.ZonedDateTime) -> int throws baml.errors.InvalidArgument | baml.time.UnknownTimezoneError | baml.errors.Io
```

The second of the minute, in `[0, 59]`, resolved through the timezone.

### timezone

```baml
function timezone(self: baml.time.ZonedDateTime) -> baml.time.TimeZoneOffset | string
```

The timezone: a `TimeZoneOffset` if fixed, or the IANA identifier.

### timezone_offset

```baml
function timezone_offset(self: baml.time.ZonedDateTime) -> baml.time.TimeZoneOffset throws baml.time.UnknownTimezoneError | baml.errors.Io
```

The resolved `TimeZoneOffset` for this `ZonedDateTime`. If the
timezone is an IANA identifier, it is resolved to a concrete offset
based on the absolute time (DST-aware), using the host's timezone
database.

### to_instant

```baml
function to_instant(self: baml.time.ZonedDateTime) -> baml.time.Instant
```

The absolute time, dropping the timezone.

### to_plain

```baml
function to_plain(self: baml.time.ZonedDateTime) -> baml.time.PlainDateTime throws baml.time.UnknownTimezoneError | baml.errors.Io
```

Drops the timezone, keeping the wall-clock reading.

### with_timezone

```baml
function with_timezone(self: baml.time.ZonedDateTime, timezone: baml.time.TimeZoneOffset | string) -> baml.time.ZonedDateTime
```

Same absolute time, different timezone.

### year

```baml
function year(self: baml.time.ZonedDateTime) -> int throws baml.errors.InvalidArgument | baml.time.UnknownTimezoneError | baml.errors.Io
```

The calendar year, resolved through the timezone.

_Source: `<builtin>/baml/ns_time/zoneddatetime.baml:512`_
