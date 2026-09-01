---
title: "Uint8Array$stream"
description: "Class Uint8Array$stream from the generated baml package reference."
---

A mutable, growable array of bytes (u8 values in the range 0–255).

Used for binary data such as file contents, network payloads, and encoded strings.
`push` silently masks values to u8; `from_array` throws `InvalidArgument` for out-of-range values.

```baml
class Uint8Array$stream
```

_Source: `<builtin>/baml/uint8array.baml:0`_
