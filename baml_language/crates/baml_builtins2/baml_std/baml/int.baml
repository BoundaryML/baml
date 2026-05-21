/// A 64-bit signed integer (i64). Range: -2^63 to 2^63-1.
class Int {
  /// Serializes this integer to a JSON number.
  function to_json(self) -> baml.json.json {
    $rust_function
  }
  // ─── Comparisons / clamping ───────────────────────────────────────────────

  /// Returns the absolute value of `self`.
  ///
  /// Throws `InvalidArgument` for `int.min_value()` because its absolute value
  /// (`2^63`) does not fit in an `int`.
  ///
  /// # Examples
  /// ```
  /// (-7).abs()         // 7
  /// (3).abs()          // 3
  /// (0).abs()          // 0
  /// int.min_value().abs()  // throws — overflow
  /// ```
  function abs(self) -> int throws root.errors.InvalidArgument {
    $rust_function
  }

  /// Returns the smaller of `self` and `other`.
  ///
  /// # Examples
  /// ```
  /// (3).min(5)   // 3
  /// (3).min(3)   // 3
  /// (-2).min(0)  // -2
  /// ```
  function min(self, other: int) -> int {
    $rust_function
  }

  /// Returns the larger of `self` and `other`.
  ///
  /// # Examples
  /// ```
  /// (3).max(5)   // 5
  /// (3).max(3)   // 3
  /// (-2).max(0)  // 0
  /// ```
  function max(self, other: int) -> int {
    $rust_function
  }

  /// Clamps `self` into the range `[min, max]`.
  ///
  /// Equivalent to `self.min(max).max(min)`. Callers should pass `min <= max`;
  /// if `min > max` the result is always `min` (the lower-clamp wins because
  /// it runs after the upper-clamp).
  ///
  /// # Examples
  /// ```
  /// (5).clamp(0, 10)    // 5
  /// (-3).clamp(0, 10)   // 0
  /// (15).clamp(0, 10)   // 10
  /// ```
  function clamp(self, min: int, max: int) -> int {
    $rust_function
  }

  // ─── Math ─────────────────────────────────────────────────────────────────

  /// Returns the integer square root of `self` — that is, the largest `int`
  /// `r` such that `r * r <= self`.
  ///
  /// Throws `InvalidArgument` if `self` is negative.
  ///
  /// # Examples
  /// ```
  /// (10).isqrt()   // 3
  /// (16).isqrt()   // 4
  /// (0).isqrt()    // 0
  /// (-1).isqrt()   // throws
  /// ```
  function isqrt(self) -> int throws root.errors.InvalidArgument {
    $rust_function
  }

  /// Returns `self ** exp`. Saturates on overflow (positive overflow returns
  /// `int.max_value()`, negative overflow returns `int.min_value()`).
  ///
  /// `0 ** 0` returns `1`, by convention.
  ///
  /// If `exp` is negative the result is `0`, since the mathematical value
  /// `self ** -n = 1 / self ** n` is in `(-1, 1)` for `|self| > 1` and rounds
  /// to zero. (For `self == 1` or `self == -1` the value is `±1` which rounds
  /// to itself; this implementation still returns `0` for negative exponents
  /// uniformly.)
  ///
  /// # Examples
  /// ```
  /// (2).pow(10)      // 1024
  /// (2).pow(0)       // 1
  /// (0).pow(0)       // 1   (convention)
  /// (2).pow(-1)      // 0
  /// (10).pow(100)    // saturates to int.max_value()
  /// (-2).pow(3)      // -8
  /// ```
  function pow(self, exp: int) -> int {
    $rust_function
  }

  /// Returns the integer logarithm of `self` in the given `base`, rounded
  /// down — i.e. the largest `n` such that `base ** n <= self`.
  ///
  /// Throws `InvalidArgument` if `self <= 0` or `base < 2`.
  ///
  /// # Examples
  /// ```
  /// (1000).ilog(10)   // 3
  /// (1024).ilog(2)    // 10
  /// (1).ilog(10)      // 0
  /// (0).ilog(10)      // throws
  /// (10).ilog(1)      // throws
  /// ```
  function ilog(self, base: int) -> int throws root.errors.InvalidArgument {
    $rust_function
  }

  // ─── Bit operations ───────────────────────────────────────────────────────
  //
  // All bit operations interpret `int` as a 64-bit two's-complement value
  // (`int` in BAML is backed by `i64`).

  /// Returns the number of leading zero bits in the 64-bit two's-complement
  /// representation of `self`.
  ///
  /// # Examples
  /// ```
  /// (0).leading_zeros()  // 64
  /// (1).leading_zeros()  // 63
  /// (-1).leading_zeros() // 0  (all-ones)
  /// ```
  function leading_zeros(self) -> int {
    $rust_function
  }

  /// Returns the number of leading one bits in the 64-bit two's-complement
  /// representation of `self`.
  ///
  /// # Examples
  /// ```
  /// (0).leading_ones()   // 0
  /// (-1).leading_ones()  // 64
  /// ```
  function leading_ones(self) -> int {
    $rust_function
  }

  /// Returns the number of trailing zero bits.
  ///
  /// # Examples
  /// ```
  /// (0).trailing_zeros()  // 64
  /// (8).trailing_zeros()  // 3   (binary 1000)
  /// (1).trailing_zeros()  // 0
  /// ```
  function trailing_zeros(self) -> int {
    $rust_function
  }

  /// Returns the number of trailing one bits.
  ///
  /// # Examples
  /// ```
  /// (7).trailing_ones()   // 3   (binary 111)
  /// (8).trailing_ones()   // 0
  /// (-1).trailing_ones()  // 64
  /// ```
  function trailing_ones(self) -> int {
    $rust_function
  }

  /// Returns the total number of zero bits in the 64-bit two's-complement
  /// representation of `self`.
  ///
  /// # Examples
  /// ```
  /// (0).count_zeros()   // 64
  /// (-1).count_zeros()  // 0
  /// ```
  function count_zeros(self) -> int {
    $rust_function
  }

  /// Returns the total number of one bits in the 64-bit two's-complement
  /// representation of `self`. Also known as the population count or
  /// "popcount".
  ///
  /// # Examples
  /// ```
  /// (0).count_ones()   // 0
  /// (7).count_ones()   // 3
  /// (-1).count_ones()  // 64
  /// ```
  function count_ones(self) -> int {
    $rust_function
  }

  // ─── Parsing ──────────────────────────────────────────────────────────────

  /// Parses `text` as a base-ten signed integer.
  ///
  /// Accepts an optional leading `+` or `-` sign followed by one or more
  /// ASCII digits. No surrounding whitespace, no underscores, no other
  /// numeric formats. The result must fit in an `int` (`i64`).
  ///
  /// Throws `ParseError` if `text` is empty, contains a non-digit character,
  /// or represents a value outside the `int` range.
  ///
  /// # Examples
  /// ```
  /// int.parse("42")       // 42
  /// int.parse("-7")       // -7
  /// int.parse("+0")       // 0
  /// int.parse("")         // throws — empty
  /// int.parse("12a")      // throws — non-digit
  /// int.parse(" 5 ")      // throws — whitespace not allowed; trim first
  /// int.parse("99999999999999999999")  // throws — out of range
  /// ```
  function parse(text: string) -> int throws root.errors.ParseError {
    $rust_function
  }

  /// Returns a uniformly distributed random integer in the half-open range
  /// `[lower, upper)`.
  ///
  /// Throws `InvalidArgument` if `lower >= upper` (the range would be empty).
  /// Uses the host's cryptographic entropy source — no user-supplied RNG
  /// handle. The largest representable half-open range is supported (e.g.
  /// `random(int.min_value(), int.max_value())` returns a value in
  /// `[int.min_value(), int.max_value())`; `int.max_value()` itself is never
  /// returned).
  ///
  /// # Examples
  /// ```
  /// int.random(0, 10)         // some value in {0, 1, ..., 9}
  /// int.random(-5, 5)         // some value in {-5, -4, ..., 4}
  /// int.random(0, 1)          // always 0  (single-element range)
  /// int.random(5, 5)          // throws — empty range
  /// int.random(10, 0)         // throws — lower > upper
  /// ```
  function random(lower: int, upper: int) -> int throws root.errors.InvalidArgument {
    $rust_function
  }

  // ─── Constants (as zero-arg static methods) ───────────────────────────────

  /// Returns the largest representable `int`, equal to `2^63 - 1`
  /// (`9_223_372_036_854_775_807`).
  function max_value() -> int {
    9223372036854775807
  }

  /// Returns the smallest representable `int`, equal to `-2^63`
  /// (`-9_223_372_036_854_775_808`). Note `int.min_value().abs()` throws.
  function min_value() -> int {
    // Written as `-MAX - 1` because `9223372036854775808` (one past `i64::MAX`)
    // overflows the literal parser before the unary minus is applied.
    -9223372036854775807 - 1
  }
}
