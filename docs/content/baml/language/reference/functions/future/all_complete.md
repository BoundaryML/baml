---
title: "future.all_complete"
description: "Function future.all_complete from the generated baml package reference."
---

BEP-034 combinators over a homogeneous array of futures (all sharing value
type `T` and error type `E`). The inputs are already running, so awaiting
them — even in a loop — runs them concurrently; the combinator just decides
how to combine the results. Each returns a new `Future`, so they compose.
Await every future and return their values in input order. Like `all`, but
the losers KEEP RUNNING on a failure (they are not cancelled). Throws the
first error encountered in input order.

```baml
function future.all_complete<T, E>(futures: baml.future.Future<T, E>[]) -> baml.future.Future<T[], E>
```

_Source: `<builtin>/baml/ns_future/future.baml:2995`_
