---
title: "sys.collect_garbage"
description: "Function sys.collect_garbage from the generated baml package reference."
---

Forces a full garbage collection and runs queued `cleanup()` finalizers before returning.
Intended for deterministic tests and runtime diagnostics; production code should normally
let the runtime schedule collections automatically.

```baml
function sys.collect_garbage() -> null
```

_Source: `<builtin>/baml/ns_sys/sys.baml:7283`_
