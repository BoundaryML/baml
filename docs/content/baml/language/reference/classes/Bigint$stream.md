---
title: "Bigint$stream"
description: "Class Bigint$stream from the generated baml package reference."
---

Arbitrary-precision signed integer.

Literals are written with a trailing `n` in decimal (`42n`), hex
(`0xFFn`), octal (`0o755n`), or binary (`0b1010n`), with `_` digit
separators allowed (`1_000_000n`).

### Bitwise on negatives

`&`, `|`, `^` use two's-complement semantics — bigints are treated as if
they had an infinite sign-extended bit string. So `(-1n) & 1n == 1n`,
`(-1n) | 0n == -1n`, `(-1n) ^ 0n == -1n`. This matches JavaScript `BigInt`
and Python `int`.

### Panics

Bigint operators and methods can raise the following unrecoverable
panics. These are not declared via `throws` because they signal that
the program tried to do something invalid (rather than a recoverable
runtime condition):

- `baml.panics.DivisionByZero` — `a / 0n` or `a % 0n`.
- `baml.panics.NegativeBitShift` — `a << -1n` or `a >> -1n`. The shift
  count must be non-negative.
- `baml.panics.AllocFailure` — the operand or result would require
  more than ~268M bits (`bigint`'s workspace cap). Sources:
    * `a * b`, `a << n`, `a.pow(n)` where the predicted result exceeds
      the cap.
    * `bigint.parse(s)` where `s` carries more decimal digits than the
      cap permits.

```baml
class Bigint$stream
```

_Source: `<builtin>/baml/bigint.baml:0`_
