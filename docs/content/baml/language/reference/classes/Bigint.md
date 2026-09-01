---
title: "Bigint"
description: "Class Bigint from the generated baml package reference."
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
class Bigint
```

## Methods

### _random_byte_count

```baml
function _random_byte_count(lower: bigint, upper: bigint) -> int
```

Internal: number of bytes needed for one draw over `[lower, upper)`.

### _random_in_range

```baml
function _random_in_range(draw: uint8array, lower: bigint, upper: bigint) -> bigint
```

Internal: maps one draw onto `[lower, upper)`, returning `upper` when the
draw must be rejected.

### abs

```baml
function abs(self: bigint) -> bigint
```

Returns the absolute value of `self`.
### Examples
```
(-7n).abs()         // 7n
(3n).abs()          // 3n
(0n).abs()          // 0n
```

### clamp

```baml
function clamp(self: bigint, min: bigint, max: bigint) -> bigint
```

Clamps `self` into the range `[min, max]`.

Equivalent to `self.min(max).max(min)`. Callers should pass `min <= max`;
if `min > max` the result is always `min` (the lower-clamp wins because
it runs after the upper-clamp).

### Examples
```
(5n).clamp(0n, 10n)    // 5n
(-3n).clamp(0, 10)   // 0n (note we cast up from `int` to `bigint` here)
(15n).clamp(0n, 10n)   // 10n
```

### ilog

```baml
function ilog(self: bigint, base: bigint) -> bigint throws baml.errors.InvalidArgument
```

Returns the integer logarithm of `self` in the given `base`, rounded
down — i.e. the largest `n` such that `base ** n <= self`.

Throws `InvalidArgument` if `self <= 0` or `base < 2`.

### Examples
```
(1000n).ilog(10n)   // 3n
(1024n).ilog(2n)    // 10n
(1n).ilog(10n)      // 0n
(0n).ilog(10n)      // throws
(10n).ilog(1n)      // throws
```

### isqrt

```baml
function isqrt(self: bigint) -> bigint throws baml.errors.InvalidArgument
```

Returns the integer square root of `self` — that is, the largest `bigint`
`r` such that `r * r <= self`.

Throws `InvalidArgument` if `self` is negative.

### Examples
```
(10n).isqrt()   // 3n
(16n).isqrt()   // 4n
(0n).isqrt()    // 0n
(-1n).isqrt()   // throws
```

### max

```baml
function max(self: bigint, other: bigint) -> bigint
```

Returns the larger of `self` and `other`.

### Examples
```
(3n).max(5n)   // 5n
(3n).max(3)   // 3n (note we cast up from `int` to `bigint` here)
(-2n).max(0n)  // 0n
```

### min

```baml
function min(self: bigint, other: bigint) -> bigint
```

Returns the smaller of `self` and `other`.

### Examples
```
(3n).min(5n)   // 3n
(3n).min(3)   // 3n (note we cast up from `int` to `bigint` here)
(-2n).min(0n)  // -2n
```

### parse

```baml
function parse(text: string) -> bigint throws baml.errors.ParseError
```

Parses `text` as a base-ten signed integer.

Accepts an optional leading `+` or `-` sign followed by one or more
ASCII digits. No surrounding whitespace, no underscore separators,
no hex / octal / binary prefix (`0x` / `0o` / `0b`), no Unicode digits,
no scientific notation. Callers needing those should preprocess the
string.

Throws `ParseError` if `text` is empty or contains a non-digit character.

### Examples
```
bigint.parse("42")       // 42n
bigint.parse("-7")       // -7n
bigint.parse("+0")       // 0n
bigint.parse("")         // throws — empty
bigint.parse("12a")      // throws — non-digit
bigint.parse("0x2a")     // throws — hex prefix not accepted
bigint.parse("1_000")    // throws — underscores not accepted
bigint.parse(" 5 ")      // throws — whitespace not allowed; trim first
bigint.parse("99999999999999999999")  // 99999999999999999999n
```

### pow

```baml
function pow(self: bigint, exp: bigint) -> bigint
```

No description is available yet.

### random

```baml
function random(lower: bigint, upper: bigint, rng: baml.random.Rng) -> bigint throws baml.errors.InvalidArgument
```

Returns a uniformly distributed random integer in the half-open range
`[lower, upper)`, drawn from `rng`.

Throws `InvalidArgument` if `lower >= upper` (the range would be empty).
`rng` defaults to `random.SystemRandom`, the host's cryptographic entropy
source; pass a seeded generator to make the draw reproducible.

Rejection sampling avoids modulo bias, so one result may consume multiple
draws from `rng`.

### Examples
```
bigint.random(0n, 10n)         // some value in {0, 1, ..., 9}
bigint.random(-5n, 5n)         // some value in {-5, -4, ..., 4}
bigint.random(0n, 1n)          // always 0  (single-element range)
bigint.random(5n, 5n)          // throws — empty range
bigint.random(10n, 0n)         // throws — lower > upper

// Reproducible: the same seed replays the same values.
let rng = baml.random.Xoshiro256PlusPlus.new(seed = my_seed);
bigint.random(0n, 10n, rng = rng)
```

### to_int

```baml
function to_int(self: bigint) -> int throws baml.errors.InvalidArgument
```

Narrows `self` to a fixed-width `int`.

Throws `InvalidArgument` if `self` is outside `int`'s range. BAML integers
are 63-bit signed, so that range (`int.min_value()` ..= `int.max_value()`)
is narrower than a machine 64-bit integer's.

The opposite direction needs no conversion: an `int` widens to `bigint`
wherever a `bigint` is expected, and `0n + value` forces it explicitly.

### Examples
```
(42n).to_int()                          // 42
(-7n).to_int()                          // -7
(4611686018427387903n).to_int()         // int.max_value()
(4611686018427387904n).to_int()         // throws — one past int.max_value()
(10n).pow(30n).to_int()                 // throws
```

_Source: `<builtin>/baml/bigint.baml:1279`_
