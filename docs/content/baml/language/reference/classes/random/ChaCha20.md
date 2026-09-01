---
title: "random.ChaCha20"
description: "Class random.ChaCha20 from the generated baml package reference."
---

A seedable, deterministic CSPRNG (ChaCha20). Given the same seed it always
reproduces the same stream. It is cryptographically secure *only* when seeded
with high-entropy input — e.g. 32 bytes drawn from `SystemRandom` (the
default when no seed is given). A low-entropy or attacker-known seed makes
its output predictable.

```baml
class random.ChaCha20
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
function _new(seed: uint8array) -> baml.random.ChaCha20
```

No description is available yet.

### new

```baml
function new(seed: uint8array) -> baml.random.ChaCha20
```

Creates a new `ChaCha20` pseudo-random number generator.

#### Parameters
- `seed`: The seed for the random number generator.
  If no seed is provided, a random seed is generated using the system random number generator.
  The seed must be at least 32 bytes long.

#### Panics
If `seed` is shorter than 32 bytes.

_Source: `<builtin>/baml/ns_random/random.baml:4384`_
