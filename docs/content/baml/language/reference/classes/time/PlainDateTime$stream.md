---
title: "time.PlainDateTime$stream"
description: "Class time.PlainDateTime$stream from the generated baml package reference."
---

A civil ("wall-clock") date and time without a timezone, on the proleptic
Gregorian calendar: `1979-05-27T07:32:00`. It cannot be located on the
global timeline without supplying a timezone via `to_zoned`.

Equivalent to `Temporal.PlainDateTime` (TC39). The `Plain` prefix marks
anything without a timezone, following TC39 Temporal.

```baml
class time.PlainDateTime$stream
```

## Fields

### _nanoseconds

```baml
_nanoseconds: bigint | null
```

Internal representation: the wall-clock reading encoded as nanoseconds
since 1970-01-01T00:00:00, interpreting the reading as if it were UTC.
This is a civil quantity, not an absolute time.

_Source: `<builtin>/baml/ns_time/plaindatetime.baml:0`_
