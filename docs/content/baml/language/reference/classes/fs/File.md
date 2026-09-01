---
title: "fs.File"
description: "Class fs.File from the generated baml package reference."
---

A handle to an open file. Use `baml.fs.open` to obtain one.

Read and write operations on a closed handle throw `Io`. `text()`
additionally throws `ParseError` when the remaining bytes are not valid
UTF-8.

```baml
class fs.File
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

## Methods

### bytes

```baml
function bytes(self: baml.fs.File) -> uint8array throws baml.errors.Io
```

Reads the entire remaining file contents as raw bytes.

### close

```baml
function close(self: baml.fs.File) -> null throws baml.errors.Io | baml.errors.InvalidArgument
```

Closes the file handle, flushing any pending writes.

### seek_from

```baml
function seek_from(self: baml.fs.File, whence: "start" | "current" | "end", offset: int) -> int throws baml.errors.Io | baml.errors.InvalidArgument
```

Moves the file cursor. `whence` is `"start"`, `"current"`, or `"end"`. Returns the new cursor position in bytes.

Throws `InvalidArgument` if `offset` is negative when paired with
`whence="start"` (the underlying syscall takes an unsigned offset).

### text

```baml
function text(self: baml.fs.File) -> string throws baml.errors.Io | baml.errors.ParseError
```

Reads the entire remaining file contents as a UTF-8 string.

### write

```baml
function write(self: baml.fs.File, data: string | uint8array) -> int throws baml.errors.Io
```

Writes all of `data` to the file at the current cursor position.
Returns the encoded byte count.

_Source: `<builtin>/baml/ns_fs/fs.baml:225`_
