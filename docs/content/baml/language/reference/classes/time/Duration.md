---
title: "time.Duration"
description: "Class time.Duration from the generated baml package reference."
---

Represents a non-localized timespan.

May be negative: while `Instant` represents absolute points in time, a `Duration` represents the difference between two `Instant`s.

```baml
class time.Duration
```

## Fields

### _nanoseconds

```baml
_nanoseconds: bigint
```

No description is available yet.

## Methods

### abs

```baml
function abs(self: baml.time.Duration) -> baml.time.Duration
```

Creates a new `Duration` representing the positive magnitude of `self`.

### from_hours

```baml
function from_hours(h: int | bigint) -> baml.time.Duration
```

Creates a new duration of `h` hours.

### from_microseconds

```baml
function from_microseconds(us: int | bigint) -> baml.time.Duration
```

Creates a new duration of `us` microseconds.

### from_milliseconds

```baml
function from_milliseconds(ms: int | bigint) -> baml.time.Duration
```

Creates a new duration of `ms` milliseconds.

### from_minutes

```baml
function from_minutes(m: int | bigint) -> baml.time.Duration
```

Creates a new duration of `m` minutes.

### from_nanoseconds

```baml
function from_nanoseconds(ns: int | bigint) -> baml.time.Duration
```

Creates a new duration of `ns` nanoseconds.

### from_seconds

```baml
function from_seconds(s: int | bigint) -> baml.time.Duration
```

Creates a new duration of `s` seconds.

### to_hours

```baml
function to_hours(self: baml.time.Duration) -> bigint
```

Returns the number of hours in `self`.
Lossy: rounds down

### to_microseconds

```baml
function to_microseconds(self: baml.time.Duration) -> bigint
```

Returns the number of microseconds in `self`.
Lossy: rounds down

### to_milliseconds

```baml
function to_milliseconds(self: baml.time.Duration) -> bigint
```

Returns the number of milliseconds in `self`.
Lossy: rounds down

### to_minutes

```baml
function to_minutes(self: baml.time.Duration) -> bigint
```

Returns the number of minutes in `self`.
Lossy: rounds down

### to_nanoseconds

```baml
function to_nanoseconds(self: baml.time.Duration) -> bigint
```

Returns the number of nanoseconds in `self`.

### to_seconds

```baml
function to_seconds(self: baml.time.Duration) -> bigint
```

Returns the number of seconds in `self`.
Lossy: rounds down

_Source: `<builtin>/baml/ns_time/duration.baml:181`_
