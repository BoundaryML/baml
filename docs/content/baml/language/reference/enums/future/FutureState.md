---
title: "future.FutureState"
description: "Enum future.FutureState from the generated baml package reference."
---

BEP-034 — concurrency via `spawn` / `await`.

`Future<T, E>` is the handle returned by `spawn { body }`. Awaiting
a future blocks the current thread until the future settles; on
settle, `await` either yields the value or re-throws the future's
error.

Sys-op calls (e.g. `http.fetch`, `fs.read`) are not futures — they
return their value directly and yield cooperatively under the hood.
To run one concurrently, wrap it in `spawn { ... }`.

The class is opaque to user code — its fields are runtime-managed.
All operations on a future go through the methods declared below.

State transitions: `Pending` → (`Ready` | `Error` | `Cancelled`).
Once settled, a future never changes state again.
Surface enumeration mirroring the runtime's `bex_vm_types::FutureRead`
states. Used by `Future.state(self)` to expose the settle status of
a future to BAML code.

```baml
enum future.FutureState
```

## Variants

### Pending

No description is available yet.

### Ready

No description is available yet.

### Error

No description is available yet.

### Cancelled

No description is available yet.

_Source: `<builtin>/baml/ns_future/future.baml:934`_
