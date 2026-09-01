---
title: "panics.Exit"
description: "Class panics.Exit from the generated baml package reference."
---

A clean process-termination request from `baml.sys.exit(code)`.

Catchable like any other panic; if left unhandled, the engine
terminates the process with this code. Patterned after Python's
`SystemExit`.

```baml
class panics.Exit
```

## Fields

### code

```baml
code: int
```

No description is available yet.

_Source: `<builtin>/baml/ns_panics/panics.baml:1181`_
