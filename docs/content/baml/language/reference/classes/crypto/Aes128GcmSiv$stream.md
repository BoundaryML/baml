---
title: "crypto.Aes128GcmSiv$stream"
description: "Class crypto.Aes128GcmSiv$stream from the generated baml package reference."
---

AES-128-GCM-SIV: nonce-misuse-resistant authenticated encryption (RFC 8452)
under a 128-bit key.

Identical in construction and guarantees to [`Aes256GcmSiv`], with a shorter
key. Prefer `Aes256GcmSiv` unless a 128-bit key is dictated by an existing
format or peer.

```baml
class crypto.Aes128GcmSiv$stream
```

## Fields

### _cipher

```baml
_cipher: $rust_type
```

Opaque, runtime-owned cipher state: the expanded AES key schedule.

The key lives here rather than in a `uint8array` field so that a cipher
carrying a wrong-length key cannot be constructed, and so key material
stays unreachable from BAML. It cannot be read back, rendered by
`string.from`, or serialized by `baml.json.from`.

_Source: `<builtin>/baml/ns_crypto/aes_gcm_siv.baml:0`_
