---
title: "crypto.Sha256$stream"
description: "Class crypto.Sha256$stream from the generated baml package reference."
---

SHA-256, the 256-bit member of the SHA-2 family (FIPS 180-4).

Hashing is incremental. Feed the message in with `update`, in as many pieces
as convenient, then read the 32-byte digest with `finish`:

```
let h = baml.crypto.Sha256.new();
h.update("hello".to_utf8());
h.update(" world".to_utf8());
h.finish().to_hex()  // b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
```

A `Sha256` is a handle onto hasher state, not a value. Two references to the
same hasher feed the same digest, and `update` from separate `spawn` fibers
is serialized rather than racing. Use a fresh `Sha256.new()` per message.

SHA-256 is a plain hash, not a keyed one. It authenticates nothing on its
own: anyone can recompute the digest of a message they can guess. Do not use
it to check that a message came from who you think, and do not hash passwords
with it.

```baml
class crypto.Sha256$stream
```

## Fields

### _state

```baml
_state: $rust_type
```

Opaque, runtime-owned hasher state: the SHA-256 compression state and
the buffered tail of the message so far.

_Source: `<builtin>/baml/ns_crypto/sha2.baml:0`_
