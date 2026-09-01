---
title: "time.PlainTime"
description: "Class time.PlainTime from the generated baml package reference."
---

A civil wall-clock time without a date or timezone: `07:32:00.5`.

Equivalent to `Temporal.PlainTime` (TC39).

```baml
class time.PlainTime
```

## Fields

### _nanoseconds

```baml
_nanoseconds: int
```

Internal representation: nanoseconds since midnight, in `[0, 24h)`.

## Methods

### _from_components

```baml
function _from_components(hour: int, minute: int, second: int, millisecond: int, microsecond: int, nanosecond: int) -> baml.time.PlainTime throws baml.errors.InvalidArgument
```

No description is available yet.

### _to_string_impl

```baml
function _to_string_impl(self: baml.time.PlainTime) -> string
```

No description is available yet.

### from_components

```baml
function from_components(hour: int, minute: int, second: int, millisecond: int, microsecond: int, nanosecond: int) -> baml.time.PlainTime throws baml.errors.InvalidArgument
```

Creates a `PlainTime` from clock components. Defaulted components
are passed by name: `PlainTime.from_components(7, minute = 32)`.

Throws `root.errors.InvalidArgument` if a component is out of range
(e.g. hour 24, minute 60).

### hour

```baml
function hour(self: baml.time.PlainTime) -> int
```

The hour of the day, in `[0, 23]`.

### millisecond

```baml
function millisecond(self: baml.time.PlainTime) -> int
```

The millisecond of the second, in `[0, 999]`.

### minute

```baml
function minute(self: baml.time.PlainTime) -> int
```

The minute of the hour, in `[0, 59]`.

### parse

```baml
function parse(s: string) -> baml.time.PlainTime throws baml.errors.ParseError
```

No description is available yet.

### second

```baml
function second(self: baml.time.PlainTime) -> int
```

The second of the minute, in `[0, 59]`.

### to_plain_datetime

```baml
function to_plain_datetime(self: baml.time.PlainTime, date: baml.time.PlainDate) -> baml.time.PlainDateTime
```

Combines with a date into a `PlainDateTime`.

_Source: `<builtin>/baml/ns_time/plaintime.baml:121`_
