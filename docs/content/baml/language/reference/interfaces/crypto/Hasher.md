---
title: "crypto.Hasher"
description: "Interface crypto.Hasher from the generated baml package reference."
---

A stateful hasher that can be used to compute a digest of data.

```baml
interface crypto.Hasher
```

## Required methods

### update

```baml
function update(self: Self, data: uint8array) -> void
```

Feeds data into the hasher's internal state.

### finish

```baml
function finish(self: Self) -> uint8array
```

Returns the digest of the hasher's internal state.

Hasher state may either reset after this call or remain unchanged.
Implementations should specify which behavior should be expected.
As a result, opaque callers should not rely on the hasher state after this call.

Provides no contract about the size or format of the digest,
each implementation should document its own digest format.

_Source: `<builtin>/baml/ns_crypto/interfaces.baml:68`_
