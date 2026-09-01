---
title: "random.Rng"
description: "Interface random.Rng from the generated baml package reference."
---

A source of random bytes.

Three implementations are provided by the standard library:
- `SystemRandom` is a secure singleton provider that calls out to the system's entropy source
- `Xoshiro256PlusPlus` is a fast seedable PRNG
- `ChaCha20` is a secure seedable PRNG

```baml
interface random.Rng
```

## Required methods

### random

```baml
function random(self: Self, bytes: int) -> uint8array
```

Produces `bytes` uniformly random bytes.

#### Panics

- If `bytes` is negative.
- If `bytes` bytes cannot be allocated (`baml.panics.AllocFailure`).
- If the implementation fails to produce the requested number of bytes
  (for example, if the underlying system random number generator is unavailable)

## Default methods

### random_int

```baml
function random_int(self: Self) -> int
```

Returns a uniformly random `int` over the full `[int.min_value(),
int.max_value()]` range.

BAML `int` is 63-bit, so this discards a single bit of the 64 drawn (the
top bit of the first byte). The remaining 63 bits are uniform, and so is
the value they spell.

_Source: `<builtin>/baml/ns_random/random.baml:290`_
