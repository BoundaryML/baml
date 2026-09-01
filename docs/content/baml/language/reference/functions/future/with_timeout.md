---
title: "future.with_timeout"
description: "Function future.with_timeout from the generated baml package reference."
---

Run `body` under a deadline.

```baml
function future.with_timeout<T, E>(limit: baml.time.Duration, body: () -> T throws E) -> T throws E | baml.errors.Timeout | baml.panics.Cancelled
```

_Source: `<builtin>/baml/ns_future/future.baml:10339`_
