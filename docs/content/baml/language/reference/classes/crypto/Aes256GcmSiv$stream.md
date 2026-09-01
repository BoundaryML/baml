---
title: "crypto.Aes256GcmSiv$stream"
description: "Class crypto.Aes256GcmSiv$stream from the generated baml package reference."
---

AES-256-GCM-SIV: nonce-misuse-resistant authenticated encryption (RFC 8452)
under a 256-bit key.

The SIV construction derives each message's keystream counter, which is the
authentication tag itself, from the plaintext and `aad`. Repeating a
`(key, nonce)` pair therefore reveals only whether two messages were
identical. It does not leak the authentication key or the XOR of the two
plaintexts, both of which plain AES-GCM gives up on nonce reuse. That makes
AES-GCM-SIV the safer choice when nonces are drawn at random rather than
from a guaranteed-unique counter. It is not a reason to reuse a nonce, only
a limit on the damage when one repeats by accident.

```
let rng = baml.random.SystemRandom.get();
let key = baml.crypto.Aes256GcmSiv.random_key(rng);
let cipher = baml.crypto.Aes256GcmSiv.new(key);

let nonce = rng.random(12);
let sealed = cipher.encrypt(nonce, plaintext, aad);
let opened = cipher.decrypt(nonce, sealed, aad);
```

The nonce is not part of the ciphertext. Store or transmit it alongside.

```baml
class crypto.Aes256GcmSiv$stream
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
