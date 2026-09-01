---
title: "fs.write_bytes"
description: "Function fs.write_bytes from the generated baml package reference."
---

Writes raw bytes `content` to the file at `path`, creating or truncating it. Returns the number of bytes written.

```baml
function fs.write_bytes(path: string, content: uint8array) -> int throws baml.errors.Io
```

_Source: `<builtin>/baml/ns_fs/fs.baml:3518`_
