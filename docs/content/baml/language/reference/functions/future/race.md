---
title: "future.race"
description: "Function future.race from the generated baml package reference."
---

Settle with the FIRST future to settle, whether it succeeds or fails; the
remaining futures are cancelled (JS `Promise.race`). A fast failure from
one input therefore wins the race — use `any` when you want the first
SUCCESS. Racing an empty array never settles (matching JS).

```baml
function future.race<T, E>(futures: baml.future.Future<T, E>[]) -> baml.future.Future<T, E>
```

_Source: `<builtin>/baml/ns_future/future.baml:6384`_
