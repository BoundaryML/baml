---
title: "future.__await_any"
description: "Function future.__await_any from the generated baml package reference."
---

BEP-034 internal primitive: suspend until the FIRST of `futures` settles —
whether it succeeds, fails, or is cancelled — and return its index in input
order. Compiles to a dedicated `AwaitAny` suspend point (it is `await` over
many futures), and is the building block for `race` and `any`. The inputs
are already running, so this only decides *which* settled first; it never
re-throws (callers `await futures[i]` to observe the value or error).

```baml
function future.__await_any<T, E>(futures: baml.future.Future<T, E>[]) -> int
```

_Source: `<builtin>/baml/ns_future/future.baml:5985`_
