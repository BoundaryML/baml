---
title: "time.PlainDate"
description: "Class time.PlainDate from the generated baml package reference."
---

A civil date without a time or timezone, on the proleptic Gregorian
calendar: `1979-05-27`.

Equivalent to `Temporal.PlainDate` (TC39).

```baml
class time.PlainDate
```

## Fields

### _days

```baml
_days: int
```

Internal representation: days since 1970-01-01 on the proleptic
Gregorian calendar.

## Methods

### _to_plain_datetime

```baml
function _to_plain_datetime(self: baml.time.PlainDate, time: baml.time.PlainTime | null) -> baml.time.PlainDateTime
```

No description is available yet.

### _to_string_impl

```baml
function _to_string_impl(self: baml.time.PlainDate) -> string throws baml.errors.InvalidArgument
```

Internal ISO 8601 formatter. Throws `InvalidArgument` when the year is
out of range; `baml.ToString.to_string` turns that into a panic, while
`to_json` surfaces it as a `JsonSerializationError`.

### day

```baml
function day(self: baml.time.PlainDate) -> int throws baml.errors.InvalidArgument
```

The day of the month, in `[1, 31]`.

### from_components

```baml
function from_components(year: int, month: int, day: int) -> baml.time.PlainDate throws baml.errors.InvalidArgument
```

No description is available yet.

### month

```baml
function month(self: baml.time.PlainDate) -> int throws baml.errors.InvalidArgument
```

The calendar month, in `[1, 12]`.

### parse

```baml
function parse(s: string) -> baml.time.PlainDate throws baml.errors.ParseError
```

No description is available yet.

### to_plain_datetime

```baml
function to_plain_datetime(self: baml.time.PlainDate, time: baml.time.PlainTime | null) -> baml.time.PlainDateTime
```

Combines with a time-of-day into a `PlainDateTime`. Defaults to
midnight when `time` is omitted (pass by name to override:
`d.to_plain_datetime(time = t)`).

### year

```baml
function year(self: baml.time.PlainDate) -> int throws baml.errors.InvalidArgument
```

The calendar year.

_Source: `<builtin>/baml/ns_time/plaindate.baml:151`_
