---
title: "Int$stream"
description: "Class Int$stream from the generated baml package reference."
---

A 63-bit signed integer. Range: -2^62 to 2^62-1
(`-4_611_686_018_427_387_904` to `4_611_686_018_427_387_903`).

The value is stored in a 64-bit machine word with one bit reserved for the
runtime's pointer/value tag, so the usable range is 63-bit, not 64-bit. Use
`bigint` for arbitrary-precision integers.

Literals may be written in decimal (`42`), hex (`0xFF`), octal (`0o755`),
or binary (`0b1010`), with `_` digit separators allowed (`1_000_000`).

Arithmetic (`+`, `-`, `*`, `/`, unary `-`) that would leave this range
throws a catchable `baml.panics.IntegerOverflow` rather than wrapping or
crashing; `/` and `%` by zero throw `baml.panics.DivisionByZero`.

`<<` is the exception: it discards the bits shifted past the 63-bit width
rather than throwing, so `1 << 62` is `int.min_value()` (bit 62 is the sign
bit) and `1 << 63` is `0`. A negative count on either shift throws
`baml.panics.NegativeBitShift`.

```baml
class Int$stream
```

_Source: `<builtin>/baml/int.baml:0`_
