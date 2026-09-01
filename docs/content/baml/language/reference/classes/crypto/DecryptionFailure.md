---
title: "crypto.DecryptionFailure"
description: "Class crypto.DecryptionFailure from the generated baml package reference."
---

A ciphertext was rejected: it did not authenticate under the key, nonce, and
associated data it was decrypted with.

`reason` is coarse by design. AEAD authentication is all or nothing, and an
implementation that reported why a tag failed to verify would give an
attacker a decryption oracle. So `reason` never distinguishes a wrong key
from a wrong nonce, mismatched `aad`, or tampered ciphertext. It names only
facts the caller already holds, such as the ciphertext being too short to
contain a tag at all.

```baml
class crypto.DecryptionFailure
```

## Fields

### algorithm

```baml
algorithm: string
```

The algorithm that rejected the ciphertext, such as `"AES-256-GCM-SIV"`.

### reason

```baml
reason: string
```

A short description of the failure, never specific enough to identify
which input was wrong.

_Source: `<builtin>/baml/ns_crypto/errors.baml:544`_
