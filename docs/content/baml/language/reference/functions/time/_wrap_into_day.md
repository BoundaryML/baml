---
title: "time._wrap_into_day"
description: "Function time._wrap_into_day from the generated baml package reference."
---

Internal: reduce `total_ns` to nanoseconds-since-midnight in `[0, 24h)`.

Written `((n % day) + day) % day` rather than `n % day` because `%` keeps
the sign of the left operand, so a negative `total_ns` would otherwise stay
negative and break `PlainTime`'s range invariant.

```baml
function time._wrap_into_day(total_ns: bigint) -> int
```

_Source: `<builtin>/baml/ns_time/plaintime.baml:5224`_
