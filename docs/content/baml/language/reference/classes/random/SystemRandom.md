---
title: "random.SystemRandom"
description: "Class random.SystemRandom from the generated baml package reference."
---

A cryptographically secure generator backed by the operating system's
entropy source (a CSPRNG). Draws fresh entropy on every call and is not
seedable, so it is non-deterministic and suitable for security-sensitive
randomness such as keys, tokens, and nonces.

```baml
class random.SystemRandom
```

## Methods

### get

```baml
function get() -> baml.random.SystemRandom
```

Returns a new `SystemRandom` instance using the global system random number generator.

_Source: `<builtin>/baml/ns_random/random.baml:1842`_
