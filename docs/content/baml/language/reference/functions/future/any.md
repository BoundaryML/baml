---
title: "future.any"
description: "Function future.any from the generated baml package reference."
---

Settle with the first future to SUCCEED; the remaining futures are then
cancelled (JS `Promise.any`). If every future fails, throws `AllFailed<E>`
carrying all the errors in input order. Cancelled inputs are skipped (they
neither win nor contribute an error).

```baml
function future.any<T, Err>(futures: baml.future.Future<T, Err>[]) -> baml.future.Future<T, baml.future.AllFailed<Err>>
```

_Source: `<builtin>/baml/ns_future/future.baml:7037`_
