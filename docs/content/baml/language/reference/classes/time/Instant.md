---
title: "time.Instant"
description: "Class time.Instant from the generated baml package reference."
---

Represents an absolute point in time.

```baml
class time.Instant
```

## Fields

### _nanoseconds

```baml
_nanoseconds: bigint
```

Internal representation: the number of nanoseconds since the UNIX epoch.

## Methods

### _to_string_impl

```baml
function _to_string_impl(self: baml.time.Instant) -> string throws baml.errors.InvalidArgument
```

Internal RFC 3339 formatter. Throws `InvalidArgument` when the year is
outside the 4-digit RFC 3339 range; `baml.ToString.to_string` turns that
into a panic, while `to_json` surfaces it as a `JsonSerializationError`.

### abs_diff

```baml
function abs_diff(self: baml.time.Instant, other: baml.time.Instant) -> baml.time.Duration
```

Returns the absolute value of the difference between `self` and `other`.
This operation is commutative: swapping `self` and `other` does not change the result.

### elapsed

```baml
function elapsed(self: baml.time.Instant) -> baml.time.Duration
```

Creates a new `Duration` representing the time elapsed since `self`.
If `time` is in the future, the duration will be negative.

Uses `Instant.now()` to get the current time. See it for caveats.

Also note that since this operation has limited accuracy
due to measurement overhead, it is not recommended to use this
for high-precision measurements.

#### Panics
If the system clock is not available, this function will panic.

#### Examples
```baml
let start = Instant.now();
let response = baml.http.fetch("https://example.com");
let duration = start.elapsed();
```

### epoch

```baml
function epoch() -> baml.time.Instant
```

Returns the Unix epoch (1970-01-01T00:00:00Z).

### from_timestamp_microseconds

```baml
function from_timestamp_microseconds(us: bigint) -> baml.time.Instant
```

Creates a new `Instant` from a number of microseconds since the UNIX epoch.

### from_timestamp_milliseconds

```baml
function from_timestamp_milliseconds(ms: bigint) -> baml.time.Instant
```

Creates a new `Instant` from a number of milliseconds since the UNIX epoch.

### from_timestamp_nanoseconds

```baml
function from_timestamp_nanoseconds(ns: bigint) -> baml.time.Instant
```

Creates a new `Instant` from a number of nanoseconds since the UNIX epoch.

### from_timestamp_seconds

```baml
function from_timestamp_seconds(s: bigint) -> baml.time.Instant
```

Creates a new `Instant` from a number of seconds since the UNIX epoch.

### max

```baml
function max(self: baml.time.Instant, other: baml.time.Instant) -> baml.time.Instant
```

If `self` is before `other`, returns `other`. Otherwise, returns `self`.

### min

```baml
function min(self: baml.time.Instant, other: baml.time.Instant) -> baml.time.Instant
```

If `self` is after `other`, returns `other`. Otherwise, returns `self`.

### now

```baml
function now() -> baml.time.Instant
```

Creates a new `Instant` representing the current point in time.

Note that this uses wall-clock time and is not guaranteed to be monotonic
(e.g. [NTP](https://en.wikipedia.org/wiki/Network_Time_Protocol) adjustments may cause time to jump backwards).

#### Panics
If the system clock is not available, this function will panic.

### parse

```baml
function parse(s: string) -> baml.time.Instant throws baml.errors.ParseError
```

No description is available yet.

### to_timestamp_microseconds

```baml
function to_timestamp_microseconds(self: baml.time.Instant) -> bigint
```

Returns the number of microseconds since the UNIX epoch.
Lossy: rounds down

### to_timestamp_milliseconds

```baml
function to_timestamp_milliseconds(self: baml.time.Instant) -> bigint
```

Returns the number of milliseconds since the UNIX epoch.
Lossy: rounds down

### to_timestamp_nanoseconds

```baml
function to_timestamp_nanoseconds(self: baml.time.Instant) -> bigint
```

Returns the number of nanoseconds since the UNIX epoch.

### to_timestamp_seconds

```baml
function to_timestamp_seconds(self: baml.time.Instant) -> bigint
```

Returns the number of seconds since the UNIX epoch.
Lossy: rounds down

_Source: `<builtin>/baml/ns_time/instant.baml:42`_
