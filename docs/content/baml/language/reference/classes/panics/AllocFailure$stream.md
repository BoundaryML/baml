---
title: "panics.AllocFailure$stream"
description: "Class panics.AllocFailure$stream from the generated baml package reference."
---

Memory allocation failure. This happens when an operation would have caused an unrecoverable
Out-Of-Memory error so we panic instead. Note that not all memory allocation failures are
guaranteed to panic; some may cause a hard failure.

```baml
class panics.AllocFailure$stream
```

## Fields

### message

```baml
message: string | null
```

No description is available yet.

_Source: `<builtin>/baml/ns_panics/panics.baml:0`_
