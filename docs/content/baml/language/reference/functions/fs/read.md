---
title: "fs.read"
description: "Function fs.read from the generated baml package reference."
---

Reads the entire contents of the file at `path` as a UTF-8 string.
Throws `ParseError` if the file's bytes are not valid UTF-8.

```baml
function fs.read(path: string) -> string throws baml.errors.Io | baml.errors.ParseError
```

_Source: `<builtin>/baml/ns_fs/fs.baml:3078`_
