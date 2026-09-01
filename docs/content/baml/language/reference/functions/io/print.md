---
title: "io.print"
description: "Function io.print from the generated baml package reference."
---

Write `s` to stdout with no trailing newline. Flushes immediately so the
bytes are visible before the next sysop or await suspends the thread.

```baml
function io.print(s: string) -> null
```

_Source: `<builtin>/baml/ns_io/io.baml:336`_
