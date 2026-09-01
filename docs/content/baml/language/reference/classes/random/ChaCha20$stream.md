---
title: "random.ChaCha20$stream"
description: "Class random.ChaCha20$stream from the generated baml package reference."
---

A seedable, deterministic CSPRNG (ChaCha20). Given the same seed it always
reproduces the same stream. It is cryptographically secure *only* when seeded
with high-entropy input — e.g. 32 bytes drawn from `SystemRandom` (the
default when no seed is given). A low-entropy or attacker-known seed makes
its output predictable.

```baml
class random.ChaCha20$stream
```

## Fields

### _state

```baml
_state: $rust_type
```

Opaque, mutex-guarded generator state owned by the Rust runtime.

_Source: `<builtin>/baml/ns_random/random.baml:0`_
