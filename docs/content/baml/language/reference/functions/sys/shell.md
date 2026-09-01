---
title: "sys.shell"
description: "Function sys.shell from the generated baml package reference."
---

Runs a shell `command` string (passed to `/bin/sh -c`) and returns its output.

```baml
function sys.shell(command: string, options: baml.sys.ProcessOptions | null) -> baml.sys.ShellOutput throws baml.errors.Io | baml.errors.Timeout
```

_Source: `<builtin>/baml/ns_sys/sys.baml:6740`_
