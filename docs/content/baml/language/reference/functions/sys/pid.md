---
title: "sys.pid"
description: "Function sys.pid from the generated baml package reference."
---

Returns the process ID of the current process.

#### Panics
If the environment does not support process IDs (e.g. in a browser), with
`baml.panics.HostUnavailable`.

```baml
function sys.pid() -> int
```

_Source: `<builtin>/baml/ns_sys/sys.baml:8090`_
