---
title: "sys.ProcessExit"
description: "Class sys.ProcessExit from the generated baml package reference."
---

The terminal status of a process created with `start_process`.

```baml
class sys.ProcessExit
```

## Fields

### exit_code

```baml
exit_code: int
```

No description is available yet.

### signal

```baml
signal: string | null
```

No description is available yet.

## Methods

### ok

```baml
function ok(self: baml.sys.ProcessExit) -> bool
```

Returns `true` if the process exited normally with code `0`.

_Source: `<builtin>/baml/ns_sys/sys.baml:1396`_
