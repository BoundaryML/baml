---
title: "random.Xoshiro256PlusPlus"
description: "Class random.Xoshiro256PlusPlus from the generated baml package reference."
---

A fast, seedable, deterministic PRNG (xoshiro256++). Given the same seed it
always reproduces the same stream, which makes it ideal for tests,
simulations, and sampling.

NOT cryptographically secure: its output is predictable from observed draws.
Never use it for keys, tokens, nonces, or any security-sensitive value — use
`SystemRandom` or a well-seeded `ChaCha20` instead.

```baml
class random.Xoshiro256PlusPlus
```

## Fields

### _state

```baml
_state: $rust_type
```

Opaque, mutex-guarded generator state owned by the Rust runtime.

## Methods

### _new

```baml
function _new(seed: uint8array) -> baml.random.Xoshiro256PlusPlus
```

No description is available yet.

### new

```baml
function new(seed: uint8array) -> baml.random.Xoshiro256PlusPlus
```

Creates a new `Xoshiro256PlusPlus` pseudo-random number generator.

#### Parameters
- `seed`: The seed for the random number generator.
  If no seed is provided, a random seed is generated using the system random number generator.
  The seed must be at least 32 bytes long.

#### Panics
If `seed` is shorter than 32 bytes.

_Source: `<builtin>/baml/ns_random/random.baml:2684`_
