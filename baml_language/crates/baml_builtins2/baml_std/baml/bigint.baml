/// Arbitrary-precision signed integer.
///
/// Literals are written with a trailing `n` in decimal (`42n`), hex
/// (`0xFFn`), octal (`0o755n`), or binary (`0b1010n`), with `_` digit
/// separators allowed (`1_000_000n`).
///
/// # Bitwise on negatives
///
/// `&`, `|`, `^` use two's-complement semantics — bigints are treated as if
/// they had an infinite sign-extended bit string. So `(-1n) & 1n == 1n`,
/// `(-1n) | 0n == -1n`, `(-1n) ^ 0n == -1n`. This matches JavaScript `BigInt`
/// and Python `int`.
///
/// # Panics
///
/// Bigint operators and methods can raise the following unrecoverable
/// panics. These are not declared via `throws` because they signal that
/// the program tried to do something invalid (rather than a recoverable
/// runtime condition):
///
/// - `baml.panics.DivisionByZero` — `a / 0n` or `a % 0n`.
/// - `baml.panics.NegativeBitShift` — `a << -1n` or `a >> -1n`. The shift
///   count must be non-negative.
/// - `baml.panics.AllocFailure` — the operand or result would require
///   more than ~268M bits (`bigint`'s workspace cap). Sources:
///     * `a * b`, `a << n`, `a.pow(n)` where the predicted result exceeds
///       the cap.
///     * `bigint.parse(s)` where `s` carries more decimal digits than the
///       cap permits.
class Bigint {
    // ─── Comparisons / clamping ───────────────────────────────────────────────

    /// Returns the absolute value of `self`.
    /// # Examples
    /// ```
    /// (-7n).abs()         // 7n
    /// (3n).abs()          // 3n
    /// (0n).abs()          // 0n
    /// ```
    function abs(self) -> bigint throws never {
        $rust_function
    }

    /// Returns the smaller of `self` and `other`.
    ///
    /// # Examples
    /// ```
    /// (3n).min(5n)   // 3n
    /// (3n).min(3)   // 3n (note we cast up from `int` to `bigint` here)
    /// (-2n).min(0n)  // -2n
    /// ```
    function min(self, other: bigint) -> bigint throws never {
        $rust_function
    }

    /// Returns the larger of `self` and `other`.
    ///
    /// # Examples
    /// ```
    /// (3n).max(5n)   // 5n
    /// (3n).max(3)   // 3n (note we cast up from `int` to `bigint` here)
    /// (-2n).max(0n)  // 0n
    /// ```
    function max(self, other: bigint) -> bigint throws never {
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
    /// (5n).clamp(0n, 10n)    // 5n
    /// (-3n).clamp(0, 10)   // 0n (note we cast up from `int` to `bigint` here)
    /// (15n).clamp(0n, 10n)   // 10n
    /// ```
    function clamp(self, min: bigint, max: bigint) -> bigint throws never {
        $rust_function
    }

    // ─── Math ─────────────────────────────────────────────────────────────────

    /// Returns the integer square root of `self` — that is, the largest `bigint`
    /// `r` such that `r * r <= self`.
    ///
    /// Throws `InvalidArgument` if `self` is negative.
    ///
    /// # Examples
    /// ```
    /// (10n).isqrt()   // 3n
    /// (16n).isqrt()   // 4n
    /// (0n).isqrt()    // 0n
    /// (-1n).isqrt()   // throws
    /// ```
    function isqrt(self) -> bigint throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Returns `self ** exp`.
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
    /// (2n).pow(10n)      // 1024n
    /// (2n).pow(0n)       // 1n
    /// (0n).pow(0n)       // 1n   (convention)
    /// (2n).pow(-1n)      // 0n
    /// (10n).pow(100n)    // a very large number
    /// (-2n).pow(3n)      // -8n
    /// ```
    // Raises `baml.panics.AllocFailure` (a panic, not a catchable `throws`) when
    // the estimated result exceeds `MAX_BIGINT_BITS`.
    //baml:fallible
    function pow(self, exp: bigint) -> bigint throws never {
        $rust_function
    }

    /// Returns the integer logarithm of `self` in the given `base`, rounded
    /// down — i.e. the largest `n` such that `base ** n <= self`.
    ///
    /// Throws `InvalidArgument` if `self <= 0` or `base < 2`.
    ///
    /// # Examples
    /// ```
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
    /// Throws `InvalidArgument` if `self` is outside `int`'s range. BAML integers
    /// are 63-bit signed, so that range (`int.min_value()` ..= `int.max_value()`)
    /// is narrower than a machine 64-bit integer's.
    ///
    /// The opposite direction needs no conversion: an `int` widens to `bigint`
    /// wherever a `bigint` is expected, and `0n + value` forces it explicitly.
    ///
    /// # Examples
    /// ```
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

    /// Parses `text` as a base-ten signed integer.
    ///
    /// Accepts an optional leading `+` or `-` sign followed by one or more
    /// ASCII digits. No surrounding whitespace, no underscore separators,
    /// no hex / octal / binary prefix (`0x` / `0o` / `0b`), no Unicode digits,
    /// no scientific notation. Callers needing those should preprocess the
    /// string.
    ///
    /// Throws `ParseError` if `text` is empty or contains a non-digit character.
    ///
    /// # Examples
    /// ```
    /// bigint.parse("42")       // 42n
    /// bigint.parse("-7")       // -7n
    /// bigint.parse("+0")       // 0n
    /// bigint.parse("")         // throws — empty
    /// bigint.parse("12a")      // throws — non-digit
    /// bigint.parse("0x2a")     // throws — hex prefix not accepted
    /// bigint.parse("1_000")    // throws — underscores not accepted
    /// bigint.parse(" 5 ")      // throws — whitespace not allowed; trim first
    /// bigint.parse("99999999999999999999")  // 99999999999999999999n
    /// ```
    function parse(text: string) -> bigint throws root.errors.ParseError {
        $rust_function
    }

    /// Returns a uniformly distributed random integer in the half-open range
    /// `[lower, upper)`.
    ///
    /// Throws `InvalidArgument` if `lower >= upper` (the range would be empty).
    /// Uses the host's cryptographic entropy source — no user-supplied RNG
    /// handle.
    ///
    /// # Examples
    /// ```
    /// bigint.random(0n, 10n)         // some value in {0, 1, ..., 9}
    /// bigint.random(-5n, 5n)         // some value in {-5, -4, ..., 4}
    /// bigint.random(0n, 1n)          // always 0  (single-element range)
    /// bigint.random(5n, 5n)          // throws — empty range
    /// bigint.random(10n, 0n)         // throws — lower > upper
    /// ```
    function random(lower: bigint, upper: bigint) -> bigint throws root.errors.InvalidArgument {
        $rust_function
    }
}
