---
title: "random.Xoshiro256PlusPlus$stream"
description: "Class random.Xoshiro256PlusPlus$stream from the generated baml package reference."
---

A fast, seedable, deterministic PRNG (xoshiro256++). Given the same seed it
always reproduces the same stream, which makes it ideal for tests,
simulations, and sampling.

NOT cryptographically secure: its output is predictable from observed draws.
Never use it for keys, tokens, nonces, or any security-sensitive value — use
`SystemRandom` or a well-seeded `ChaCha20` instead.

```baml
class random.Xoshiro256PlusPlus$stream
```

## Fields

### _state

```baml
_state: $rust_type
```

Opaque, mutex-guarded generator state owned by the Rust runtime.

_Source: `<builtin>/baml/ns_random/random.baml:0`_
