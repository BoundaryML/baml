/// A 63-bit signed integer, from `-2^62` (`-4_611_686_018_427_387_904`) to
/// `2^62 - 1` (`4_611_686_018_427_387_903`).
///
/// The runtime reserves one bit of a 64-bit machine word for its pointer/value
/// tag, so the usable width is 63 bits rather than 64. Unlike Python's `int`,
/// an `int` does not grow to fit; use `bigint` for arbitrary precision.
///
/// Literals may be decimal (`42`), hex (`0xFF`), octal (`0o755`), or binary
/// (`0b1010`), with `_` digit separators allowed (`1_000_000`).
///
/// Bit operations such as `leading_zeros` and `count_ones` read `self` as a
/// 64-bit two's-complement word. The value is stored sign-extended, so those
/// counts include the high sign/tag bits: `(1).leading_zeros()` is `63`, not
/// `62`.
///
/// # Panics
///
/// Arithmetic panics rather than wrapping. These panics are catchable, but are
/// not declared in `throws` clauses.
///
/// - `baml.panics.IntegerOverflow` if `+`, `-`, `*`, `/`, or unary `-` leaves
///   the `int` range.
/// - `baml.panics.DivisionByZero` if the right operand of `/` or `%` is zero.
/// - `baml.panics.NegativeBitShift` if the count given to `<<` or `>>` is
///   negative.
///
/// `<<` is the exception to the overflow rule: it truncates, discarding
/// whatever is shifted past the 63-bit width. `1 << 62` is `int.min_value()`
/// (bit 62 is the sign bit) and `1 << 63` is `0`. `>>` cannot overflow either;
/// a count of 63 or more saturates to the sign, giving `0` for a non-negative
/// `self` and `-1` for a negative one.

class Int {
    // ─── Magnitude ────────────────────────────────────────────────────────────

    /// Returns the absolute value of `self`.
    ///
    /// # Panics
    ///
    /// - `baml.panics.IntegerOverflow` for `int.min_value()`, whose absolute
    ///   value `2^62` is one past `int.max_value()`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (-7).abs()             // 7
    /// (3).abs()              // 3
    /// int.min_value().abs()  // panics
    /// ```
    //baml:fallible
    function abs(self) -> int throws never {
        $rust_function
    }

    // `min` / `max` / `clamp` are NOT declared here. They come from
    // `baml.ops.Compare`, which `int` implements — one definition of "smaller"
    // per type, so `(3).max(x)` cannot mean something different from the `max` a
    // `T extends baml.ops.Compare` bound reaches.
    //
    // ```
    // (3).min(5)          // 3
    // (-2).max(0)         // 0
    // (15).clamp(0, 10)   // 10
    // ```

    // ─── Math ─────────────────────────────────────────────────────────────────

    /// Returns the largest `r` such that `r * r <= self`: the integer square
    /// root of `self`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `self` is negative.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (10).isqrt()   // 3
    /// (16).isqrt()   // 4
    /// (-1).isqrt()   // throws
    /// ```
    function isqrt(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Returns `self ** exp`, saturating instead of panicking on overflow:
    /// `int.max_value()` for positive overflow, `int.min_value()` for negative.
    ///
    /// `0 ** 0` is `1`, by convention. A negative `exp` gives `0`, because the
    /// exact value `1 / self ** -exp` lies in `(-1, 1)` for `|self| > 1` and
    /// rounds toward zero. That holds uniformly: for `self == 1` or
    /// `self == -1` the exact value is `±1`, and the result is still `0`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (2).pow(10)      // 1024
    /// (2).pow(0)       // 1
    /// (0).pow(0)       // 1   (convention)
    /// (2).pow(-1)      // 0
    /// (-2).pow(3)      // -8
    /// (10).pow(100)    // saturates to int.max_value()
    /// ```
    function pow(self, exp: int) -> int throws never {
        $rust_function
    }

    /// Returns the largest `n` such that `base ** n <= self`: the logarithm of
    /// `self` in `base`, rounded down.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `self <= 0` or `base < 2`.
    ///
    /// # Examples
    ///
    /// ```baml
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

    /// Returns the number of leading zero bits.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (0).leading_zeros()  // 64
    /// (1).leading_zeros()  // 63
    /// (-1).leading_zeros() // 0   (all ones)
    /// ```
    function leading_zeros(self) -> int throws never {
        $rust_function
    }

    /// Returns the number of leading one bits.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (0).leading_ones()   // 0
    /// (-1).leading_ones()  // 64
    /// ```
    function leading_ones(self) -> int throws never {
        $rust_function
    }

    /// Returns the number of trailing zero bits.
    ///
    /// # Examples
    ///
    /// ```baml
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
    ///
    /// ```baml
    /// (7).trailing_ones()   // 3   (binary 111)
    /// (8).trailing_ones()   // 0
    /// (-1).trailing_ones()  // 64
    /// ```
    function trailing_ones(self) -> int throws never {
        $rust_function
    }

    /// Returns the total number of zero bits.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (0).count_zeros()   // 64
    /// (-1).count_zeros()  // 0
    /// ```
    function count_zeros(self) -> int throws never {
        $rust_function
    }

    /// Returns the total number of one bits, also known as the population
    /// count or "popcount".
    ///
    /// # Examples
    ///
    /// ```baml
    /// (0).count_ones()   // 0
    /// (7).count_ones()   // 3
    /// (-1).count_ones()  // 64
    /// ```
    function count_ones(self) -> int throws never {
        $rust_function
    }

    // ─── Parsing ──────────────────────────────────────────────────────────────

    /// Parses `text` as a base ten signed integer.
    ///
    /// Accepts an optional leading `+` or `-` followed by one or more ASCII
    /// digits, and nothing else: no surrounding whitespace, no underscores, no
    /// other numeric formats. The value must fit the 63-bit `int` range.
    ///
    /// # Throws
    ///
    /// - `baml.errors.ParseError` if `text` is empty, holds a non-digit
    ///   character, or names a value outside the `int` range.
    ///
    /// # Examples
    ///
    /// ```baml
    /// int.parse("42")       // 42
    /// int.parse("-7")       // -7
    /// int.parse("+0")       // 0
    /// int.parse("")         // throws — empty
    /// int.parse("12a")      // throws — non-digit
    /// int.parse(" 5 ")      // throws — whitespace; trim first
    /// int.parse("99999999999999999999")  // throws — out of range
    /// ```
    function parse(text: string) -> int throws root.errors.ParseError {
        $rust_function
    }

    /// Returns a uniformly distributed random integer in the half-open range
    /// `[lower, upper)`, drawn from `rng`.
    ///
    /// `rng` defaults to `random.SystemRandom`, the host's cryptographic entropy
    /// source; pass a seeded generator to make the draw reproducible. Even the
    /// widest representable half-open range works. Rejection sampling avoids
    /// modulo bias, so one result may consume several values from `rng`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `lower >= upper`, which would leave
    ///   the range empty.
    ///
    /// # Examples
    ///
    /// ```baml
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
                message: `int.random: lower (${lower}) must be less than upper (${upper})`
            };
        }
        // Rust maps each draw; BAML owns retries because it can call `rng` again.
        let r = int._random_in_range(rng.random_int(), lower, upper);
        while (r == upper) {
            r = int._random_in_range(rng.random_int(), lower, upper);
        }
        r
    }

    /// (internal) Maps one full-range draw onto `[lower, upper)`, returning
    /// `upper` when the draw must be rejected.
    function _random_in_range(draw: int, lower: int, upper: int) -> int throws never {
        $rust_function
    }

    // ─── Constants (as zero-arg static methods) ───────────────────────────────

    /// Returns the largest representable `int`, `2^62 - 1`
    /// (`4_611_686_018_427_387_903`).
    function max_value() -> int throws never {
        4611686018427387903
    }

    /// Returns the smallest representable `int`, `-2^62`
    /// (`-4_611_686_018_427_387_904`). `int.min_value().abs()` panics.
    function min_value() -> int throws never {
        // Written as `-MAX - 1` to avoid the literal parser; same trick as the
        // old i64::MIN definition.
        -4611686018427387903 - 1
    }
}
