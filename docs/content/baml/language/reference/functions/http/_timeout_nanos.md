---
title: "http._timeout_nanos"
description: "Function http._timeout_nanos from the generated baml package reference."
---

Converts an optional timeout into the nanosecond count carried across the
sys-op boundary. `null` (no deadline) becomes `0n`, which the native layer
treats as unbounded. Modeled on Rust's `Option<Duration>`.

```baml
function http._timeout_nanos(timeout: baml.time.Duration | null) -> bigint
```

_Source: `<builtin>/baml/ns_http/http.baml:5407`_
