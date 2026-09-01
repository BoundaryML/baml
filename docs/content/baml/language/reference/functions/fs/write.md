---
title: "fs.write"
description: "Function fs.write from the generated baml package reference."
---

Writes `content` to the file at `path`, creating or truncating it. Returns the number of bytes written.

```baml
function fs.write(path: string, content: string) -> int throws baml.errors.Io
```

_Source: `<builtin>/baml/ns_fs/fs.baml:3298`_
