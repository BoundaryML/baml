---
title: "fs.open"
description: "Function fs.open from the generated baml package reference."
---

Opens the file at `path` with the given `mode`.

Mode values: `"r"` (read), `"r+"` (read/write), `"w"` (write/truncate),
`"w+"` (read/write/truncate), `"a"` (append), `"a+"` (read/append).

```baml
function fs.open(path: string, mode: "r" | "r+" | "w" | "w+" | "a" | "a+") -> baml.fs.File throws baml.errors.Io | baml.errors.InvalidArgument
```

_Source: `<builtin>/baml/ns_fs/fs.baml:2165`_
