---
title: "sys.start_process"
description: "Function sys.start_process from the generated baml package reference."
---

Starts `program` and returns a live process with caller-owned pipes.

Unlike `exec`, output is not buffered until process exit. Reading
`process.stdout` suspends only the current BAML green thread. If
`ProcessOptions.stdin` is set, this call writes that input and waits for the
child to consume it, bounded by `ProcessOptions.timeout_ms`.

```baml
function sys.start_process(program: string, args: string[] | null, options: baml.sys.ProcessOptions | null) -> baml.sys.Process throws baml.errors.Io
```

_Source: `<builtin>/baml/ns_sys/sys.baml:6499`_
