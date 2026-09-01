---
title: "fs.File$stream"
description: "Class fs.File$stream from the generated baml package reference."
---

A handle to an open file. Use `baml.fs.open` to obtain one.

Read and write operations on a closed handle throw `Io`. `text()`
additionally throws `ParseError` when the remaining bytes are not valid
UTF-8.

```baml
class fs.File$stream
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

_Source: `<builtin>/baml/ns_fs/fs.baml:0`_
