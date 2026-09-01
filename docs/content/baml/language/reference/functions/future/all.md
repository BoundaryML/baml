---
title: "future.all"
description: "Function future.all from the generated baml package reference."
---

Await every future and return their values in input order, like
`all_complete`, but on the FIRST failure the remaining futures are
cancelled before the error is re-thrown (JS `Promise.all`). Use
`all_complete` instead when losers have side effects that must finish.

Note: failures are observed in input order (not strictly first-to-fail in
wall-clock time); the value contract — every value, or one error — is the
same either way.

```baml
function future.all<T, E>(futures: baml.future.Future<T, E>[]) -> baml.future.Future<T[], E>
```

_Source: `<builtin>/baml/ns_future/future.baml:3640`_
