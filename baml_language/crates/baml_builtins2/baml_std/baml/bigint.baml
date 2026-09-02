/// An arbitrary-precision signed integer, like Python's `int` or JavaScript's
/// `BigInt`. Where `int` panics on overflow, `bigint` grows.
///
/// Literals carry a trailing `n` and may be decimal (`42n`), hex (`0xFFn`),
/// octal (`0o755n`), or binary (`0b1010n`), with `_` digit separators allowed
/// (`1_000_000n`).
///
/// `&`, `|`, and `^` treat a bigint as an infinite sign-extended two's
/// complement bit string, matching JavaScript `BigInt` and Python `int`:
/// `(-1n) & 1n` is `1n`, `(-1n) | 0n` is `-1n`, `(-1n) ^ 0n` is `-1n`.
///
/// # Panics
///
/// Bigint operators and methods raise the panics below. They are not declared
/// via `throws` because they signal that the program tried to do something
/// invalid, rather than a recoverable runtime condition.
///
/// - `baml.panics.DivisionByZero` — `a / 0n` or `a % 0n`.
/// - `baml.panics.NegativeBitShift` — `a << -1n` or `a >> -1n`. The shift
///   count must be non-negative.
/// - `baml.panics.AllocFailure` — an operand or result would need more than
///   about 268M bits, `bigint`'s workspace cap. Raised by `a * b`, `a << n`,
///   and `a.pow(n)` when the predicted result exceeds the cap, and by
///   `bigint.parse(s)` when `s` carries more decimal digits than the cap
///   permits.
class Bigint {
    // ─── Comparisons / clamping ───────────────────────────────────────────────

    /// Returns the absolute value of `self`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (-7n).abs()         // 7n
    /// (3n).abs()          // 3n
    /// ```
    function abs(self) -> bigint throws never {
        $rust_function
    }

    /// Returns the smaller of `self` and `other`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (3n).min(5n)   // 3n
    /// (3n).min(3)    // 3n  (the int widens to bigint)
    /// (-2n).min(0n)  // -2n
    /// ```
    function min(self, other: bigint) -> bigint throws never {
        $rust_function
    }

    /// Returns the larger of `self` and `other`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (3n).max(5n)   // 5n
    /// (3n).max(3)    // 3n  (the int widens to bigint)
    /// (-2n).max(0n)  // 0n
    /// ```
    function max(self, other: bigint) -> bigint throws never {
        $rust_function
    }

    /// Clamps `self` into the range `[min, max]`.
    ///
    /// Equivalent to `self.min(max).max(min)`. Pass `min <= max`; if `min > max`
    /// the result is always `min`, because the lower clamp runs last.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (5n).clamp(0n, 10n)   // 5n
    /// (-3n).clamp(0, 10)    // 0n  (the ints widen to bigint)
    /// (15n).clamp(0n, 10n)  // 10n
    /// ```
    function clamp(self, min: bigint, max: bigint) -> bigint throws never {
        $rust_function
    }

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
    /// (10n).isqrt()   // 3n
    /// (16n).isqrt()   // 4n
    /// (-1n).isqrt()   // throws
    /// ```
    function isqrt(self) -> bigint throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Returns `self ** exp`.
    ///
    /// `0 ** 0` is `1n`, by convention. A negative `exp` gives `0n`, because
    /// the exact value `1 / self ** -exp` lies in `(-1, 1)` for `|self| > 1`
    /// and rounds toward zero. That holds uniformly: for `self == 1n` or
    /// `self == -1n` the exact value is `±1n`, and the result is still `0n`.
    ///
    /// # Panics
    ///
    /// - `baml.panics.AllocFailure` if the estimated result exceeds `bigint`'s
    ///   workspace cap.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (2n).pow(10n)      // 1024n
    /// (2n).pow(0n)       // 1n
    /// (0n).pow(0n)       // 1n   (convention)
    /// (2n).pow(-1n)      // 0n
    /// (-2n).pow(3n)      // -8n
    /// (10n).pow(100n)    // a very large number
    /// ```
    //baml:fallible
    function pow(self, exp: bigint) -> bigint throws never {
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
    /// (1000n).ilog(10n)   // 3n
    /// (1024n).ilog(2n)    // 10n
    /// (1n).ilog(10n)      // 0n
    /// (0n).ilog(10n)      // throws
    /// (10n).ilog(1n)      // throws
    /// ```
    function ilog(self, base: bigint) -> bigint throws root.errors.InvalidArgument {
        $rust_function
    }

    // ─── Conversions ──────────────────────────────────────────────────────────

    /// Narrows `self` to a fixed-width `int`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `self` falls outside `int`'s range.
    ///   BAML integers are 63-bit signed, so `int.min_value()` through
    ///   `int.max_value()` is narrower than a machine 64-bit integer.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (42n).to_int()                          // 42
    /// (-7n).to_int()                          // -7
    /// (4611686018427387903n).to_int()         // int.max_value()
    /// (4611686018427387904n).to_int()         // throws — one past int.max_value()
    /// (10n).pow(30n).to_int()                 // throws
    /// ```
    function to_int(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    // ─── Parsing ──────────────────────────────────────────────────────────────

    /// Parses `text` as a base ten signed integer.
    ///
    /// Accepts an optional leading `+` or `-` followed by one or more ASCII
    /// digits, and nothing else: no surrounding whitespace, no underscore
    /// separators, no `0x` / `0o` / `0b` prefix, no Unicode digits, no
    /// scientific notation. Preprocess the string if you need any of those.
    ///
    /// # Throws
    ///
    /// - `baml.errors.ParseError` if `text` is empty or holds a non-digit
    ///   character.
    ///
    /// # Examples
    ///
    /// ```baml
    /// bigint.parse("42")       // 42n
    /// bigint.parse("-7")       // -7n
    /// bigint.parse("+0")       // 0n
    /// bigint.parse("")         // throws — empty
    /// bigint.parse("12a")      // throws — non-digit
    /// bigint.parse("0x2a")     // throws — hex prefix not accepted
    /// bigint.parse("1_000")    // throws — underscores not accepted
    /// bigint.parse(" 5 ")      // throws — whitespace; trim first
    /// bigint.parse("99999999999999999999")  // 99999999999999999999n
    /// ```
    function parse(text: string) -> bigint throws root.errors.ParseError {
        $rust_function
    }

    /// Returns a uniformly distributed random integer in the half-open range
    /// `[lower, upper)`, drawn from `rng`.
    ///
    /// `rng` defaults to `random.SystemRandom`, the host's cryptographic entropy
    /// source; pass a seeded generator to make the draw reproducible. Rejection
    /// sampling avoids modulo bias, so one result may consume several draws from
    /// `rng`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `lower >= upper`, which would leave
    ///   the range empty.
    ///
    /// # Examples
    ///
    /// ```baml
    /// bigint.random(0n, 10n)         // some value in {0, 1, ..., 9}
    /// bigint.random(-5n, 5n)         // some value in {-5, -4, ..., 4}
    /// bigint.random(0n, 1n)          // always 0  (single-element range)
    /// bigint.random(5n, 5n)          // throws — empty range
    /// bigint.random(10n, 0n)         // throws — lower > upper
    ///
    /// // Reproducible: the same seed replays the same values.
    /// let rng = baml.random.Xoshiro256PlusPlus.new(seed = my_seed);
    /// bigint.random(0n, 10n, rng = rng)
    /// ```
    function random(
        lower: bigint,
        upper: bigint,
        rng: root.random.Rng = root.random.SystemRandom.get(),
    ) -> bigint throws root.errors.InvalidArgument {
        if (lower >= upper) {
            throw root.errors.InvalidArgument {
                message: "bigint.random: lower (" + string.from(lower) + ") must be less than upper (" + string.from(upper) + ")"
            };
        }
        let width = bigint._random_byte_count(lower, upper);
        let r = bigint._random_in_range(rng.random(width), lower, upper);
        while (r == upper) {
            r = bigint._random_in_range(rng.random(width), lower, upper);
        }
        r
    }

    /// (internal) Number of bytes needed for one draw over `[lower, upper)`.
    function _random_byte_count(lower: bigint, upper: bigint) -> int throws never {
        $rust_function
    }

    /// (internal) Maps one draw onto `[lower, upper)`, returning `upper` when
    /// the draw must be rejected.
    function _random_in_range(
        draw: uint8array,
        lower: bigint,
        upper: bigint,
    ) -> bigint throws never {
        $rust_function
    }
}
