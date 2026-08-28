/// A 63-bit signed integer. Range: -2^62 to 2^62-1
/// (`-4_611_686_018_427_387_904` to `4_611_686_018_427_387_903`).
///
/// The value is stored in a 64-bit machine word with one bit reserved for the
/// runtime's pointer/value tag, so the usable range is 63-bit, not 64-bit. Use
/// `bigint` for arbitrary-precision integers.
///
/// Literals may be written in decimal (`42`), hex (`0xFF`), octal (`0o755`),
/// or binary (`0b1010`), with `_` digit separators allowed (`1_000_000`).
///
/// Arithmetic (`+`, `-`, `*`, `/`, unary `-`) that would leave this range
/// throws a catchable `baml.panics.IntegerOverflow` rather than wrapping or
/// crashing; `/` and `%` by zero throw `baml.panics.DivisionByZero`.
///
/// `<<` is the exception: it discards the bits shifted past the 63-bit width
/// rather than throwing, so `1 << 62` is `int.min_value()` (bit 62 is the sign
/// bit) and `1 << 63` is `0`. A negative count on either shift throws
/// `baml.panics.NegativeBitShift`.
class Int {
    // ─── Comparisons / clamping ───────────────────────────────────────────────

    /// Returns the absolute value of `self`.
    ///
    /// Panics with `baml.panics.IntegerOverflow` for `int.min_value()`, whose
    /// absolute value (`2^62`) does not fit in an `int` — the same channel
    /// `-int.min_value()` and the arithmetic operators use for an overflow.
    ///
    /// # Examples
    /// ```
    /// (-7).abs()         // 7
    /// (3).abs()          // 3
    /// (0).abs()          // 0
    /// int.min_value().abs()  // panics — overflow
    /// ```
    //baml:fallible
    function abs(self) -> int throws never {
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
    function min(self, other: int) -> int throws never {
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
    function max(self, other: int) -> int throws never {
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
    function clamp(self, min: int, max: int) -> int throws never {
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
    function pow(self, exp: int) -> int throws never {
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
    // All bit operations interpret `int` as a 64-bit two's-complement value:
    // although `int`'s value range is 63-bit, it is stored sign-extended in a
    // 64-bit machine word, and these operations expose all 64 bits of that word
    // (e.g. `count_zeros` of a small positive value counts the high tag/sign
    // bits too).

    /// Returns the number of leading zero bits in the 64-bit two's-complement
    /// representation of `self`.
    ///
    /// # Examples
    /// ```
    /// (0).leading_zeros()  // 64
    /// (1).leading_zeros()  // 63
    /// (-1).leading_zeros() // 0  (all-ones)
    /// ```
    function leading_zeros(self) -> int throws never {
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
    function leading_ones(self) -> int throws never {
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
    function trailing_zeros(self) -> int throws never {
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
    function trailing_ones(self) -> int throws never {
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
    function count_zeros(self) -> int throws never {
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
    function count_ones(self) -> int throws never {
        $rust_function
    }

    // ─── Parsing ──────────────────────────────────────────────────────────────

    /// Parses `text` as a base-ten signed integer.
    ///
    /// Accepts an optional leading `+` or `-` sign followed by one or more
    /// ASCII digits. No surrounding whitespace, no underscores, no other
    /// numeric formats. The result must fit in the `int` range (63-bit signed).
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
    /// `[lower, upper)`, drawn from `rng`.
    ///
    /// Throws `InvalidArgument` if `lower >= upper` (the range would be empty).
    /// `rng` defaults to `random.SystemRandom`, the host's cryptographic entropy
    /// source; pass a seeded generator to make the draw reproducible. The largest
    /// representable half-open range is supported. Rejection sampling avoids
    /// modulo bias, so one result may consume multiple values from `rng`.
    ///
    /// # Examples
    /// ```
    /// int.random(0, 10)         // some value in {0, 1, ..., 9}
    /// int.random(-5, 5)         // some value in {-5, -4, ..., 4}
    /// int.random(0, 1)          // always 0  (single-element range)
    /// int.random(5, 5)          // throws — empty range
    /// int.random(10, 0)         // throws — lower > upper
    ///
    /// // Reproducible: the same seed replays the same values.
    /// let rng = baml.random.Xoshiro256PlusPlus.new(seed = my_seed);
    /// int.random(0, 10, rng = rng)
    /// ```
    function random(
        lower: int,
        upper: int,
        rng: root.random.Rng = root.random.SystemRandom.get(),
    ) -> int throws root.errors.InvalidArgument {
        if (lower >= upper) {
            throw root.errors.InvalidArgument {
                message: "int.random: lower (" + string.from(lower) + ") must be less than upper (" + string.from(upper) + ")"
            };
        }
        // Rust maps each draw; BAML owns retries because it can call `rng` again.
        let r = int._random_in_range(rng.random_int(), lower, upper);
        while (r == upper) {
            r = int._random_in_range(rng.random_int(), lower, upper);
        }
        r
    }

    /// Internal: maps one full-range draw onto `[lower, upper)`, returning
    /// `upper` when the draw must be rejected.
    function _random_in_range(draw: int, lower: int, upper: int) -> int throws never {
        $rust_function
    }

    // ─── Constants (as zero-arg static methods) ───────────────────────────────

    /// Returns the largest representable `int`, equal to `2^62 - 1`
    /// (`4_611_686_018_427_387_903`).
    ///
    /// Note: BAML integers are 63-bit signed (the runtime reserves one bit
    /// for the tagged-pointer Value encoding). Values outside the
    /// `[min_value(), max_value()]` range cannot round-trip through int.
    function max_value() -> int {
        4611686018427387903
    }

    /// Returns the smallest representable `int`, equal to `-2^62`
    /// (`-4_611_686_018_427_387_904`). Note `int.min_value().abs()` throws.
    function min_value() -> int {
        // Written as `-MAX - 1` to avoid the literal parser; same trick as the
        // old i64::MIN definition.
        -4611686018427387903 - 1
    }
}
