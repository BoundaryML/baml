---
title: "time.PlainDateTime"
description: "Class time.PlainDateTime from the generated baml package reference."
---

A civil ("wall-clock") date and time without a timezone, on the proleptic
Gregorian calendar: `1979-05-27T07:32:00`. It cannot be located on the
global timeline without supplying a timezone via `to_zoned`.

Equivalent to `Temporal.PlainDateTime` (TC39). The `Plain` prefix marks
anything without a timezone, following TC39 Temporal.

```baml
class time.PlainDateTime
```

## Fields

### _nanoseconds

```baml
_nanoseconds: bigint
```

Internal representation: the wall-clock reading encoded as nanoseconds
since 1970-01-01T00:00:00, interpreting the reading as if it were UTC.
This is a civil quantity, not an absolute time.

## Methods

### _from_components

```baml
function _from_components(year: int, month: int, day: int, hour: int, minute: int, second: int, millisecond: int, microsecond: int, nanosecond: int) -> baml.time.PlainDateTime throws baml.errors.InvalidArgument
```

No description is available yet.

### _to_string_impl

```baml
function _to_string_impl(self: baml.time.PlainDateTime) -> string throws baml.errors.InvalidArgument
```

Internal ISO 8601 formatter. Throws `InvalidArgument` when the year is
outside ±9999; `baml.ToString.to_string` turns that into a panic, while
`to_json` surfaces it as a `JsonSerializationError`.

### day

```baml
function day(self: baml.time.PlainDateTime) -> int throws baml.errors.InvalidArgument
```

The day of the month, in `[1, 31]`.

### from_components

```baml
function from_components(year: int, month: int, day: int, hour: int, minute: int, second: int, millisecond: int, microsecond: int, nanosecond: int) -> baml.time.PlainDateTime throws baml.errors.InvalidArgument
```

Creates a `PlainDateTime` from calendar/clock components. `month` and
`day` are 1-based. Defaulted clock components are passed by name:
`PlainDateTime.from_components(1979, 5, 27, hour = 7)`.

Throws `root.errors.InvalidArgument` if a component is out of range
(e.g. month 13, Feb 30) or the year is outside ±9999.

### hour

```baml
function hour(self: baml.time.PlainDateTime) -> int throws baml.errors.InvalidArgument
```

The hour of the day, in `[0, 23]`.

### max

```baml
function max(self: baml.time.PlainDateTime, other: baml.time.PlainDateTime) -> baml.time.PlainDateTime
```

If `self` is before `other` (civil comparison), returns `other`.
Otherwise, returns `self`.

### millisecond

```baml
function millisecond(self: baml.time.PlainDateTime) -> int throws baml.errors.InvalidArgument
```

The millisecond of the second, in `[0, 999]`.

### min

```baml
function min(self: baml.time.PlainDateTime, other: baml.time.PlainDateTime) -> baml.time.PlainDateTime
```

If `self` is after `other` (civil comparison), returns `other`.
Otherwise, returns `self`.

### minute

```baml
function minute(self: baml.time.PlainDateTime) -> int throws baml.errors.InvalidArgument
```

The minute of the hour, in `[0, 59]`.

### month

```baml
function month(self: baml.time.PlainDateTime) -> int throws baml.errors.InvalidArgument
```

The calendar month, in `[1, 12]`.

### parse

```baml
function parse(s: string) -> baml.time.PlainDateTime throws baml.errors.ParseError
```

No description is available yet.

### second

```baml
function second(self: baml.time.PlainDateTime) -> int throws baml.errors.InvalidArgument
```

The second of the minute, in `[0, 59]`.

### to_plain_date

```baml
function to_plain_date(self: baml.time.PlainDateTime) -> baml.time.PlainDate throws baml.errors.InvalidArgument
```

No description is available yet.

### to_plain_time

```baml
function to_plain_time(self: baml.time.PlainDateTime) -> baml.time.PlainTime
```

No description is available yet.

### to_zoned

```baml
function to_zoned(self: baml.time.PlainDateTime, timezone: baml.time.TimeZoneOffset | string, disambiguation: baml.time.Disambiguation) -> baml.time.ZonedDateTime throws baml.time.UnknownTimezoneError | baml.time.AmbiguousTimeError | baml.errors.Io
```

Locates this wall-clock reading in a timezone, producing an absolute
time. With a `TimeZoneOffset` the conversion is exact. With an IANA
identifier, DST gaps/overlaps are resolved per `disambiguation`
(passed by name: `dt.to_zoned(tz, disambiguation = "earlier")`).

### year

```baml
function year(self: baml.time.PlainDateTime) -> int throws baml.errors.InvalidArgument
```

The calendar year.

_Source: `<builtin>/baml/ns_time/plaindatetime.baml:356`_
