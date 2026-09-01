---
title: "errors.TypeMismatch"
description: "Class errors.TypeMismatch from the generated baml package reference."
---

A value/type mismatch at the call boundary — a caller passed an argument
that doesn't fit the callee's (possibly inferred) type, a generic `TypeVar`
could not be inferred and must be specified, or repeat occurrences of a
`TypeVar` have no consistent binding. Synthesized host-side from
`EngineError::TypeMismatch`; each host SDK surfaces it as its native
type-error (Python `TypeError`). The stdlib itself never throws this class
from BAML source (reflection's typed reads throw
`reflect.errors.TypeMismatch` instead); user code remains free to.

```baml
class errors.TypeMismatch
```

## Fields

### message

```baml
message: string
```

No description is available yet.

_Source: `<builtin>/baml/ns_errors/errors.baml:2850`_
