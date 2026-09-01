---
title: "panics.SdkPanic"
description: "Class panics.SdkPanic from the generated baml package reference."
---

An internal BAML engine failure surfaced to the host. Opaque: the specific
engine error (an `EngineError` variant) is carried as text in `message`,
since these are engine/VM-internal failures with nothing host-actionable to
branch on. Synthesized host-side from any `EngineError`.

```baml
class panics.SdkPanic
```

## Fields

### message

```baml
message: string
```

No description is available yet.

_Source: `<builtin>/baml/ns_panics/panics.baml:2295`_
