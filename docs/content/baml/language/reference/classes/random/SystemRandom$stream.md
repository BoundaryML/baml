---
title: "random.SystemRandom$stream"
description: "Class random.SystemRandom$stream from the generated baml package reference."
---

A cryptographically secure generator backed by the operating system's
entropy source (a CSPRNG). Draws fresh entropy on every call and is not
seedable, so it is non-deterministic and suitable for security-sensitive
randomness such as keys, tokens, and nonces.

```baml
class random.SystemRandom$stream
```

_Source: `<builtin>/baml/ns_random/random.baml:0`_
