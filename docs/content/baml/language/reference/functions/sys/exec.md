---
title: "sys.exec"
description: "Function sys.exec from the generated baml package reference."
---

Runs `program` with the given `args` and returns its output. Throws on I/O failure or timeout.

```baml
function sys.exec(program: string, args: string[] | null, options: baml.sys.ProcessOptions | null) -> baml.sys.ShellOutput throws baml.errors.Io | baml.errors.Timeout
```

_Source: `<builtin>/baml/ns_sys/sys.baml:5962`_
