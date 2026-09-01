---
title: "time.ZonedDateTime$stream"
description: "Class time.ZonedDateTime$stream from the generated baml package reference."
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
class time.ZonedDateTime$stream
```

## Fields

### _nanoseconds

```baml
_nanoseconds: bigint | null
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

_Source: `<builtin>/baml/ns_time/zoneddatetime.baml:0`_
