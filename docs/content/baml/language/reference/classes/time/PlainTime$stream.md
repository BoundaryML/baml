---
title: "time.PlainTime$stream"
description: "Class time.PlainTime$stream from the generated baml package reference."
---

A civil wall-clock time without a date or timezone: `07:32:00.5`.

Equivalent to `Temporal.PlainTime` (TC39).

```baml
class time.PlainTime$stream
```

## Fields

### _nanoseconds

```baml
_nanoseconds: int | null
```

Internal representation: nanoseconds since midnight, in `[0, 24h)`.

_Source: `<builtin>/baml/ns_time/plaintime.baml:0`_
