---
title: "crypto.ChaCha20Poly1305"
description: "Class crypto.ChaCha20Poly1305 from the generated baml package reference."
---

ChaCha20-Poly1305 (RFC 8439): authenticated encryption under a 256-bit key
with a 96-bit nonce.

A stream cipher paired with a one-time authenticator. It runs on ordinary
arithmetic rather than table lookups, so it is naturally constant-time and
fast in software. Prefer it over the AES ciphers on hardware without AES
instructions, and where a peer or format already specifies it (TLS, SSH,
WireGuard, `age`).

```
let rng = baml.random.SystemRandom.get();
let key = baml.crypto.ChaCha20Poly1305.random_key(rng);
let cipher = baml.crypto.ChaCha20Poly1305.new(key);

let nonce = rng.random(12);
let sealed = cipher.encrypt(nonce, plaintext, aad);
let opened = cipher.decrypt(nonce, sealed, aad);
```

#### Never reuse a nonce

Unlike [`Aes256GcmSiv`], this construction offers nothing when a
`(key, nonce)` pair repeats. Two messages encrypted under the same pair leak
the XOR of their plaintexts, and the repetition also exposes the Poly1305
key, which lets an attacker forge ciphertexts that authenticate. One repeat
is enough to lose both confidentiality and integrity for that key.

A 96-bit nonce is too small to draw at random for a long-lived key: at
random, repeats become likely after roughly 2^48 messages. Derive nonces from
a counter you know is unique, or use [`XChaCha20Poly1305`], whose 192-bit
nonce is large enough to draw at random.

The nonce is not part of the ciphertext. Store or transmit it alongside.

```baml
class crypto.ChaCha20Poly1305
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

## Methods

### new

```baml
function new(key: uint8array) -> baml.crypto.ChaCha20Poly1305 throws baml.errors.InvalidArgument
```

No description is available yet.

_Source: `<builtin>/baml/ns_crypto/chacha20poly1305.baml:1548`_
