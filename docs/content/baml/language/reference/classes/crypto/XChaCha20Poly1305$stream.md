---
title: "crypto.XChaCha20Poly1305$stream"
description: "Class crypto.XChaCha20Poly1305$stream from the generated baml package reference."
---

XChaCha20-Poly1305: [`ChaCha20Poly1305`] with a 192-bit nonce.

The extended nonce is the whole point. It is hashed down to a per-message
subkey before the standard construction runs, so the same 256-bit key,
stream cipher, and authenticator do the work, with a nonce large enough to
draw uniformly at random. At 192 bits, a repeat is negligible even across an
enormous number of messages, which removes the counter bookkeeping that
`ChaCha20Poly1305` demands.

Reach for this whenever nonces come from an RNG rather than from a counter
you control:

```
let rng = baml.random.SystemRandom.get();
let key = baml.crypto.XChaCha20Poly1305.random_key(rng);
let cipher = baml.crypto.XChaCha20Poly1305.new(key);

let nonce = rng.random(24);
let sealed = cipher.encrypt(nonce, plaintext, aad);
let opened = cipher.decrypt(nonce, sealed, aad);
```

A deliberately reused nonce still costs exactly what it costs for
`ChaCha20Poly1305`. The larger nonce makes an accidental collision
negligible; it does not make repetition safe.

The nonce is not part of the ciphertext. Store or transmit it alongside.

```baml
class crypto.XChaCha20Poly1305$stream
```

## Fields

### _cipher

```baml
_cipher: $rust_type
```

Opaque, runtime-owned cipher state.

The key lives here rather than in a `uint8array` field so that a cipher
carrying a wrong-length key cannot be constructed, and so key material
stays unreachable from BAML. It cannot be read back, rendered by
`string.from`, or serialized by `baml.json.from`.

_Source: `<builtin>/baml/ns_crypto/chacha20poly1305.baml:0`_
