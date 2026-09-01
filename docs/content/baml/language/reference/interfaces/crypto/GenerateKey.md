---
title: "crypto.GenerateKey"
description: "Interface crypto.GenerateKey from the generated baml package reference."
---

Implemented by algorithms that can draw a fresh key of their own size.

The key comes back as raw bytes rather than as a ready-to-use cipher, so it
can be stored, wrapped, or transported. Pass it to the algorithm's `new` to
get a cipher back.

```baml
interface crypto.GenerateKey
```

## Associated types

### Key

```baml
type Key
```

No description is available yet.

## Required methods

### random_key

```baml
function random_key(rng: baml.random.Rng) -> (Self as baml.crypto.GenerateKey).Key
```

Draws a fresh key of the algorithm's key length from `rng`.

A key is only as unpredictable as the generator it came from. Use a
well-seeded cryptographically secure RNG: `baml.random.SystemRandom`, or
a `baml.random.ChaCha20` seeded from it. Never pass
`baml.random.Xoshiro256PlusPlus`, whose output is predictable from
observed draws.

_Source: `<builtin>/baml/ns_crypto/interfaces.baml:3801`_
