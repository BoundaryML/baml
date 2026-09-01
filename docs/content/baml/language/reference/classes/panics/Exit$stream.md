---
title: "panics.Exit$stream"
description: "Class panics.Exit$stream from the generated baml package reference."
---

A clean process-termination request from `baml.sys.exit(code)`.

Catchable like any other panic; if left unhandled, the engine
terminates the process with this code. Patterned after Python's
`SystemExit`.

```baml
class panics.Exit$stream
```

## Fields

### code

```baml
code: int | null
```

No description is available yet.

_Source: `<builtin>/baml/ns_panics/panics.baml:0`_
