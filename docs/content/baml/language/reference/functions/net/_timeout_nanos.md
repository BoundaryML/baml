---
title: "net._timeout_nanos"
description: "Function net._timeout_nanos from the generated baml package reference."
---

Converts an optional timeout into the nanosecond count carried across the
sys-op boundary. `null` (no deadline) becomes `0n`, which the native layer
treats as unbounded (see `timeout_from_nanos`). Modeled on Rust's
`Option<Duration>`, where `None` means "block forever".

```baml
function net._timeout_nanos(timeout: baml.time.Duration | null) -> bigint
```

_Source: `<builtin>/baml/ns_net/net.baml:287`_
