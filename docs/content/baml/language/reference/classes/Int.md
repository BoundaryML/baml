---
title: "Int"
description: "Class Int from the generated baml package reference."
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
class Int
```

## Methods

### _random_in_range

```baml
function _random_in_range(draw: int, lower: int, upper: int) -> int
```

Internal: maps one full-range draw onto `[lower, upper)`, returning
`upper` when the draw must be rejected.

### abs

```baml
function abs(self: int) -> int throws baml.errors.InvalidArgument
```

Returns the absolute value of `self`.

Throws `InvalidArgument` for `int.min_value()` because its absolute value
(`2^62`) does not fit in an `int`.

### Examples
```
(-7).abs()         // 7
(3).abs()          // 3
(0).abs()          // 0
int.min_value().abs()  // throws — overflow
```

### clamp

```baml
function clamp(self: int, min: int, max: int) -> int
```

Clamps `self` into the range `[min, max]`.

Equivalent to `self.min(max).max(min)`. Callers should pass `min <= max`;
if `min > max` the result is always `min` (the lower-clamp wins because
it runs after the upper-clamp).

### Examples
```
(5).clamp(0, 10)    // 5
(-3).clamp(0, 10)   // 0
(15).clamp(0, 10)   // 10
```

### count_ones

```baml
function count_ones(self: int) -> int
```

Returns the total number of one bits in the 64-bit two's-complement
representation of `self`. Also known as the population count or
"popcount".

### Examples
```
(0).count_ones()   // 0
(7).count_ones()   // 3
(-1).count_ones()  // 64
```

### count_zeros

```baml
function count_zeros(self: int) -> int
```

Returns the total number of zero bits in the 64-bit two's-complement
representation of `self`.

### Examples
```
(0).count_zeros()   // 64
(-1).count_zeros()  // 0
```

### ilog

```baml
function ilog(self: int, base: int) -> int throws baml.errors.InvalidArgument
```

Returns the integer logarithm of `self` in the given `base`, rounded
down — i.e. the largest `n` such that `base ** n <= self`.

Throws `InvalidArgument` if `self <= 0` or `base < 2`.

### Examples
```
(1000).ilog(10)   // 3
(1024).ilog(2)    // 10
(1).ilog(10)      // 0
(0).ilog(10)      // throws
(10).ilog(1)      // throws
```

### isqrt

```baml
function isqrt(self: int) -> int throws baml.errors.InvalidArgument
```

Returns the integer square root of `self` — that is, the largest `int`
`r` such that `r * r <= self`.

Throws `InvalidArgument` if `self` is negative.

### Examples
```
(10).isqrt()   // 3
(16).isqrt()   // 4
(0).isqrt()    // 0
(-1).isqrt()   // throws
```

### leading_ones

```baml
function leading_ones(self: int) -> int
```

Returns the number of leading one bits in the 64-bit two's-complement
representation of `self`.

### Examples
```
(0).leading_ones()   // 0
(-1).leading_ones()  // 64
```

### leading_zeros

```baml
function leading_zeros(self: int) -> int
```

Returns the number of leading zero bits in the 64-bit two's-complement
representation of `self`.

### Examples
```
(0).leading_zeros()  // 64
(1).leading_zeros()  // 63
(-1).leading_zeros() // 0  (all-ones)
```

### max

```baml
function max(self: int, other: int) -> int
```

Returns the larger of `self` and `other`.

### Examples
```
(3).max(5)   // 5
(3).max(3)   // 3
(-2).max(0)  // 0
```

### max_value

```baml
function max_value() -> int
```

Returns the largest representable `int`, equal to `2^62 - 1`
(`4_611_686_018_427_387_903`).

Note: BAML integers are 63-bit signed (the runtime reserves one bit
for the tagged-pointer Value encoding). Values outside the
`[min_value(), max_value()]` range cannot round-trip through int.

### min

```baml
function min(self: int, other: int) -> int
```

Returns the smaller of `self` and `other`.

### Examples
```
(3).min(5)   // 3
(3).min(3)   // 3
(-2).min(0)  // -2
```

### min_value

```baml
function min_value() -> int
```

Returns the smallest representable `int`, equal to `-2^62`
(`-4_611_686_018_427_387_904`). Note `int.min_value().abs()` throws.

### parse

```baml
function parse(text: string) -> int throws baml.errors.ParseError
```

Parses `text` as a base-ten signed integer.

Accepts an optional leading `+` or `-` sign followed by one or more
ASCII digits. No surrounding whitespace, no underscores, no other
numeric formats. The result must fit in the `int` range (63-bit signed).

Throws `ParseError` if `text` is empty, contains a non-digit character,
or represents a value outside the `int` range.

### Examples
```
int.parse("42")       // 42
int.parse("-7")       // -7
int.parse("+0")       // 0
int.parse("")         // throws — empty
int.parse("12a")      // throws — non-digit
int.parse(" 5 ")      // throws — whitespace not allowed; trim first
int.parse("99999999999999999999")  // throws — out of range
```

### pow

```baml
function pow(self: int, exp: int) -> int
```

Returns `self ** exp`. Saturates on overflow (positive overflow returns
`int.max_value()`, negative overflow returns `int.min_value()`).

`0 ** 0` returns `1`, by convention.

If `exp` is negative the result is `0`, since the mathematical value
`self ** -n = 1 / self ** n` is in `(-1, 1)` for `|self| > 1` and rounds
to zero. (For `self == 1` or `self == -1` the value is `±1` which rounds
to itself; this implementation still returns `0` for negative exponents
uniformly.)

### Examples
```
(2).pow(10)      // 1024
(2).pow(0)       // 1
(0).pow(0)       // 1   (convention)
(2).pow(-1)      // 0
(10).pow(100)    // saturates to int.max_value()
(-2).pow(3)      // -8
```

### random

```baml
function random(lower: int, upper: int, rng: baml.random.Rng) -> int throws baml.errors.InvalidArgument
```

Returns a uniformly distributed random integer in the half-open range
`[lower, upper)`, drawn from `rng`.

Throws `InvalidArgument` if `lower >= upper` (the range would be empty).
`rng` defaults to `random.SystemRandom`, the host's cryptographic entropy
source; pass a seeded generator to make the draw reproducible. The largest
representable half-open range is supported. Rejection sampling avoids
modulo bias, so one result may consume multiple values from `rng`.

### Examples
```
int.random(0, 10)         // some value in {0, 1, ..., 9}
int.random(-5, 5)         // some value in {-5, -4, ..., 4}
int.random(0, 1)          // always 0  (single-element range)
int.random(5, 5)          // throws — empty range
int.random(10, 0)         // throws — lower > upper

// Reproducible: the same seed replays the same values.
let rng = baml.random.Xoshiro256PlusPlus.new(seed = my_seed);
int.random(0, 10, rng = rng)
```

### trailing_ones

```baml
function trailing_ones(self: int) -> int
```

Returns the number of trailing one bits.

### Examples
```
(7).trailing_ones()   // 3   (binary 111)
(8).trailing_ones()   // 0
(-1).trailing_ones()  // 64
```

### trailing_zeros

```baml
function trailing_zeros(self: int) -> int
```

Returns the number of trailing zero bits.

### Examples
```
(0).trailing_zeros()  // 64
(8).trailing_zeros()  // 3   (binary 1000)
(1).trailing_zeros()  // 0
```

_Source: `<builtin>/baml/int.baml:981`_
