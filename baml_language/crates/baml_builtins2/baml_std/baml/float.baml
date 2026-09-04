/// A 64-bit IEEE 754 floating-point number (f64).
///
/// Note: division by zero (`x / 0.0`) does not follow raw IEEE — it throws a
/// catchable `baml.panics.DivisionByZero` rather than yielding `±inf`/`NaN`,
/// matching integer division. `inf` and `NaN` still arise from `float.inf()` /
/// `float.nan()`, from overflow (e.g. `1e308 * 10.0` → `inf`), and from other
/// invalid operations (e.g. `(-1.0).sqrt()` → `NaN`).
class Float {
    /// Serializes this float to a JSON number. NaN and Infinity are serialized as null.
    // mut_vm: Float values are heap-boxed (Object::Float); construction needs a TLAB.
    // ─── Predicates ───────────────────────────────────────────────────────────

    /// Returns `true` if `self` is NaN (a "not a number" sentinel produced by
    /// invalid operations such as `(-1.0).sqrt()` or `float.inf() - float.inf()`).
    /// Note `0.0 / 0.0` throws `DivisionByZero` rather than producing NaN.
    ///
    /// NaN is the only float value that is not equal to itself (`x != x` iff
    /// `x.is_nan()`), so this predicate is the only correct way to test for NaN.
    ///
    /// # Examples
    /// ```
    /// (0.0).is_nan()         // false
    /// float.nan().is_nan()   // true
    /// float.inf().is_nan()   // false
    /// ```
    function is_nan(self) -> bool throws never {
        $rust_function
    }

    /// Returns `true` if `self` is positive or negative infinity.
    ///
    /// # Examples
    /// ```
    /// float.inf().is_infinite()                 // true
    /// (-float.inf()).is_infinite()              // true
    /// (1000000.0 * 1000000.0).is_infinite()     // false  (1e12, large but finite)
    /// float.nan().is_infinite()                 // false
    /// ```
    function is_infinite(self) -> bool throws never {
        $rust_function
    }

    /// Returns `true` if `self` is neither NaN nor infinite (i.e. an ordinary
    /// finite float, including subnormals and `±0.0`).
    ///
    /// # Examples
    /// ```
    /// (3.14).is_finite()         // true
    /// (0.0).is_finite()          // true
    /// float.inf().is_finite()    // false
    /// float.nan().is_finite()    // false
    /// ```
    function is_finite(self) -> bool {
        !self.is_nan() && !self.is_infinite()
    }

    // ─── Comparisons / clamping ───────────────────────────────────────────────

    /// Returns the absolute value of `self`. Preserves NaN.
    ///
    /// # Examples
    /// ```
    /// (-3.5).abs()           // 3.5
    /// (3.5).abs()            // 3.5
    /// (-0.0).abs()           // 0.0
    /// float.nan().abs()      // NaN
    /// ```
    function abs(self) -> float throws never {
        $rust_function
    }

    /// Returns the smaller of `self` and `other`.
    ///
    /// **NaN handling:** if exactly one operand is NaN, returns the non-NaN one
    /// (ergonomic NaN suppression — matches Rust `f64::min`). If both are NaN,
    /// the result is NaN. Use `is_nan()` upstream if you need stricter handling.
    ///
    /// # Examples
    /// ```
    /// (3.0).min(5.0)              // 3.0
    /// (3.0).min(float.nan())      // 3.0  (NaN suppressed)
    /// float.nan().min(float.nan()) // NaN
    /// ```
    function min(self, other: float) -> float throws never {
        $rust_function
    }

    /// Returns the larger of `self` and `other`.
    ///
    /// NaN handling matches `min`: a non-NaN operand is preferred over NaN.
    function max(self, other: float) -> float throws never {
        $rust_function
    }

    /// Clamps `self` into the range `[min, max]`.
    ///
    /// Equivalent to `self.min(max).max(min)`. Callers should pass `min <= max`;
    /// if `min > max` the result is always `min` (the lower-clamp wins because
    /// it runs after the upper-clamp).
    ///
    /// NaN propagates through both `min` and `max` calls — if `self` is NaN,
    /// the result is NaN.
    ///
    /// # Examples
    /// ```
    /// (5.0).clamp(0.0, 10.0)     // 5.0
    /// (-3.0).clamp(0.0, 10.0)    // 0.0
    /// (15.0).clamp(0.0, 10.0)    // 10.0
    /// ```
    function clamp(self, min: float, max: float) -> float throws never {
        $rust_function
    }

    /// Returns `1.0` if `self` is positive and `-1.0` if `self` is negative,
    /// where the sign of `±0.0` and of NaN counts.
    ///
    /// This is a sign test, not a comparison against zero: it never returns
    /// `0.0` or NaN, so every float maps to exactly `±1.0`. `(-0.0).signum()`
    /// is `-1.0` and `(0.0).signum()` is `1.0`; `float.nan()` is a positive
    /// NaN and so returns `1.0`.
    ///
    /// Native because the sign of a zero is unreachable from BAML: `==` and `<`
    /// are ordinary IEEE and cannot tell `-0.0` from `0.0`, and `1.0 / 0.0`
    /// throws `DivisionByZero` rather than yielding a signed infinity. The one
    /// surface that can see it — `Comparable for float`, which orders by IEEE
    /// 754 `totalOrder` — is implemented on the primitive `float` rather than
    /// on this class, so `self` cannot dispatch to it.
    ///
    /// # Examples
    /// ```
    /// (3.5).signum()          // 1.0
    /// (-3.5).signum()         // -1.0
    /// (0.0).signum()          // 1.0
    /// (-0.0).signum()         // -1.0
    /// (-float.inf()).signum() // -1.0
    /// float.nan().signum()    // 1.0
    /// ```
    function signum(self) -> float throws never {
        $rust_function
    }

    // ─── Rounding (returns float) ─────────────────────────────────────────────
    //
    // These return `float` because the rounded value may exceed the `int`
    // range; for fallible conversions to `int`, use the `i*` variants below.

    /// Returns the largest integer ≤ `self`, as a float.
    /// Preserves the sign of `±0.0` and propagates NaN/±∞.
    ///
    /// # Examples
    /// ```
    /// (3.7).floor()    // 3.0
    /// (-1.5).floor()   // -2.0
    /// (3.0).floor()    // 3.0
    /// ```
    function floor(self) -> float throws never {
        $rust_function
    }

    /// Returns the smallest integer ≥ `self`, as a float.
    /// Preserves the sign of `±0.0` and propagates NaN/±∞.
    ///
    /// # Examples
    /// ```
    /// (3.2).ceil()    // 4.0
    /// (-1.5).ceil()   // -1.0
    /// ```
    function ceil(self) -> float throws never {
        $rust_function
    }

    /// Returns the integer nearest to `self`, with ties rounded away from zero
    /// (so `0.5 → 1.0`, `-0.5 → -1.0`, `1.5 → 2.0`).
    ///
    /// # Examples
    /// ```
    /// (1.5).round()    // 2.0
    /// (-1.5).round()   // -2.0
    /// (1.4).round()    // 1.0
    /// (1.6).round()    // 2.0
    /// ```
    function round(self) -> float throws never {
        $rust_function
    }

    /// Returns `self` truncated toward zero — i.e. the integer part with the
    /// fractional part discarded.
    ///
    /// # Examples
    /// ```
    /// (3.7).trunc()    // 3.0
    /// (-3.7).trunc()   // -3.0
    /// ```
    function trunc(self) -> float throws never {
        $rust_function
    }

    /// Returns the fractional part of `self` (`self - self.trunc()`).
    ///
    /// # Examples
    /// ```
    /// (3.7).fract()    // 0.7
    /// (-3.7).fract()   // -0.7
    /// (5.0).fract()    // 0.0
    /// ```
    function fract(self) -> float throws never {
        $rust_function
    }

    // ─── Rounding to int (fallible) ───────────────────────────────────────────
    //
    // Each of these throws `InvalidArgument` if `self` is NaN, ±∞, or — after
    // rounding — falls outside the `int` (`i64`) range. The exact threshold is
    // `[-2^63, 2^63)`; values strictly less than `i64.min_value()` or ≥ `2^63`
    // (one past `i64.max_value()`, the smallest value not representable as `int`)
    // throw.

    /// Like `floor`, but returns `int`. Throws if the floored value would not
    /// fit in `int` or `self` is NaN.
    function ifloor(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Like `ceil`, but returns `int`. Throws if the ceiled value would not fit
    /// in `int` or `self` is NaN.
    function iceil(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Like `round`, but returns `int`. Throws if the rounded value would not
    /// fit in `int` or `self` is NaN.
    function iround(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Like `trunc`, but returns `int`. Throws if the integer part would not
    /// fit in `int` or `self` is NaN/±∞.
    function itrunc(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    // ─── Power / logarithm ────────────────────────────────────────────────────

    /// Returns the square root of `self`. For negative inputs returns NaN.
    ///
    /// # Examples
    /// ```
    /// (4.0).sqrt()       // 2.0
    /// (2.0).sqrt()       // 1.4142...
    /// (-1.0).sqrt()      // NaN
    /// (0.0).sqrt()       // 0.0
    /// ```
    function sqrt(self) -> float throws never {
        $rust_function
    }

    /// Returns `self ** exp` (floating-point exponentiation).
    ///
    /// Returns NaN for inputs where the mathematical value is not real (e.g.
    /// negative base raised to a non-integer power).
    ///
    /// # Examples
    /// ```
    /// (2.0).pow(10.0)         // 1024.0
    /// (2.0).pow(-1.0)         // 0.5
    /// (2.0).pow(0.5)          // sqrt(2)
    /// (-1.0).pow(0.5)         // NaN
    /// ```
    function pow(self, exp: float) -> float throws never {
        $rust_function
    }

    /// Returns the logarithm of `self` in the given `base`. Equivalent to
    /// `self.ln() / base.ln()`.
    ///
    /// The boundary behavior follows directly from that formula:
    /// - `self < 0` → NaN  (since `ln(negative) = NaN`)
    /// - `self == 0`, `base > 0`, `base != 1` → `-∞`
    /// - `base < 0` → NaN
    /// - `base == 0`, `self > 0` → `±0` (sign opposite of `ln(self)`, since
    ///   `±finite / -∞ = ∓0`)
    /// - `base == 1`, `self > 0`, `self != 1` → `±∞` (sign matches `ln(self)`,
    ///   since `ln(1) == +0.0` and `±finite / +0.0 = ±∞`)
    /// - `base == 1`, `self == 1` → NaN (`0 / 0`)
    ///
    /// The result is a ratio of two logarithms, so it is not correctly rounded
    /// even when the exact answer is representable: `(1000.0).log(10.0)` yields
    /// `2.9999999999999996` rather than `3.0`. Use `log10` (correctly rounded)
    /// when the base is 10, or `ln` (exact, since `ln(e)` is `1.0`) for base
    /// `e`.
    ///
    /// # Examples
    /// ```
    /// (1000.0).log(10.0)   // 2.9999999999999996  (not 3.0 — see above)
    /// (8.0).log(2.0)       // 3.0
    /// (-1.0).log(10.0)     // NaN
    /// (0.0).log(10.0)      // -infinity
    /// (2.0).log(1.0)       // +infinity
    /// (1.0).log(1.0)       // NaN
    /// ```
    function log(self, base: float) -> float throws never {
        $rust_function
    }

    /// Returns `e ** self`, the natural exponential of `self`.
    ///
    /// Computed as `float.e().pow(self)`. This is within one ulp of a
    /// correctly-rounded exponential for ordinary inputs, but not bit-exact:
    /// `(2.0).exp()` yields `7.3890560989306495` where the correctly-rounded
    /// value is `7.38905609893065`.
    ///
    /// # Examples
    /// ```
    /// (0.0).exp()           // 1.0
    /// (1.0).exp()           // 2.718281828459045
    /// (-float.inf()).exp()  // 0.0
    /// float.nan().exp()     // NaN
    /// ```
    function exp(self) -> float {
        // `self * 1.0` coerces `Float` to `float`: `pow` takes a `float`
        // exponent, and `self` is not one inside the class body. The multiply
        // is exact for every input, NaN and ±0.0 included.
        float.e().pow(self * 1.0)
    }

    /// Returns the natural logarithm (base `e`) of `self`.
    ///
    /// Exactly equal to `self.log(float.e())` — `ln(e)` is `1.0`, so the
    /// division that `log` performs is exact and costs no precision.
    ///
    /// Boundaries follow `log`: `self < 0` → NaN, `self == 0` → `-∞`,
    /// `self == 1` → `0.0`.
    ///
    /// # Examples
    /// ```
    /// (1.0).ln()        // 0.0
    /// float.e().ln()    // 1.0
    /// (0.0).ln()        // -infinity
    /// (-1.0).ln()       // NaN
    /// ```
    function ln(self) -> float {
        self.log(float.e())
    }

    /// Returns the base-2 logarithm of `self`.
    ///
    /// A readability alias for `self.log(2.0)`, with the same accuracy: exact
    /// for powers of two, within one ulp elsewhere (`(10.0).log2()` yields
    /// `3.3219280948873626` where the correctly-rounded value is
    /// `3.321928094887362`). Same boundaries as `ln`.
    ///
    /// # Examples
    /// ```
    /// (8.0).log2()      // 3.0
    /// (1024.0).log2()   // 10.0
    /// (1.0).log2()      // 0.0
    /// (0.0).log2()      // -infinity
    /// ```
    function log2(self) -> float {
        self.log(2.0)
    }

    /// Returns the base-10 logarithm of `self`.
    ///
    /// More accurate than `self.log(10.0)`, which computes `ln(self) / ln(10)`
    /// and loses the last bits at clean inputs — `(1000.0).log(10.0)` yields
    /// `2.9999999999999996` rather than `3.0`. Same boundaries as `ln`.
    ///
    /// # Examples
    /// ```
    /// (1000.0).log10()  // 3.0
    /// (100.0).log10()   // 2.0
    /// (1.0).log10()     // 0.0
    /// (0.0).log10()     // -infinity
    /// ```
    function log10(self) -> float throws never {
        $rust_function
    }

    /// Returns the real cube root of `self`.
    ///
    /// Defined for negative inputs, unlike `self.pow(1.0 / 3.0)`: `pow` rounds
    /// `1.0 / 3.0` to a double before exponentiating, so it returns NaN for a
    /// negative base and misses perfect cubes (`(1000.0).pow(1.0 / 3.0)` yields
    /// `9.999999999999998`).
    ///
    /// # Examples
    /// ```
    /// (8.0).cbrt()      // 2.0
    /// (-8.0).cbrt()     // -2.0
    /// (1000.0).cbrt()   // 10.0
    /// (0.0).cbrt()      // 0.0
    /// ```
    function cbrt(self) -> float throws never {
        $rust_function
    }

    /// Returns the Euclidean distance `sqrt(self² + other²)`, computed in a way
    /// that avoids spurious overflow even when both magnitudes are large.
    ///
    /// # Examples
    /// ```
    /// (3.0).hypot(4.0)    // 5.0
    /// (0.0).hypot(0.0)    // 0.0
    /// ```
    function hypot(self, other: float) -> float throws never {
        $rust_function
    }

    // ─── Trigonometry (radians) ───────────────────────────────────────────────
    //
    // All angle arguments and return values are in radians. Use `to_radians()`
    // to convert from degrees (or `to_degrees()` to convert results back).

    /// Sine of `self` (in radians).
    function sin(self) -> float throws never {
        $rust_function
    }

    /// Cosine of `self` (in radians).
    function cos(self) -> float throws never {
        $rust_function
    }

    /// Tangent of `self` (in radians). Very large near odd multiples of π/2.
    function tan(self) -> float throws never {
        $rust_function
    }

    /// Inverse sine, returning a value in `[-π/2, π/2]`.
    /// Returns NaN if `self` is outside `[-1, 1]`.
    function asin(self) -> float throws never {
        $rust_function
    }

    /// Inverse cosine, returning a value in `[0, π]`.
    /// Returns NaN if `self` is outside `[-1, 1]`.
    function acos(self) -> float throws never {
        $rust_function
    }

    /// Inverse tangent, returning a value in `(-π/2, π/2)`.
    function atan(self) -> float throws never {
        $rust_function
    }

    /// Returns the angle (in radians) of the vector `(other, self)` — i.e. the
    /// angle from the positive x-axis to the point `(x = other, y = self)`.
    /// The result is in `[-π, π]`.
    ///
    /// # Examples
    /// ```
    /// (1.0).atan2(1.0)        // π/4    (45°)
    /// (1.0).atan2(0.0)        // π/2    (90°, due north)
    /// (0.0).atan2(-1.0)       // π      (180°, due west)
    /// ```
    function atan2(self, other: float) -> float throws never {
        $rust_function
    }

    /// Hyperbolic sine.
    function sinh(self) -> float throws never {
        $rust_function
    }

    /// Hyperbolic cosine.
    function cosh(self) -> float throws never {
        $rust_function
    }

    /// Hyperbolic tangent. Bounded to `(-1, 1)`.
    function tanh(self) -> float throws never {
        $rust_function
    }

    /// Inverse hyperbolic sine. Domain: all reals.
    function asinh(self) -> float throws never {
        $rust_function
    }

    /// Inverse hyperbolic cosine. Domain: `[1, ∞)`. Returns NaN for `self < 1`.
    function acosh(self) -> float throws never {
        $rust_function
    }

    /// Inverse hyperbolic tangent. Domain: `(-1, 1)`. Returns NaN outside that
    /// range, and ±∞ at the endpoints.
    function atanh(self) -> float throws never {
        $rust_function
    }

    // ─── Angle conversions ────────────────────────────────────────────────────

    /// Converts `self` from degrees to radians.
    function to_radians(self) -> float {
        self * float.pi() / 180.0
    }

    /// Converts `self` from radians to degrees.
    function to_degrees(self) -> float {
        self * 180.0 / float.pi()
    }

    // ─── Parsing / randomness ─────────────────────────────────────────────────

    /// Parses `text` as a floating-point number.
    ///
    /// Accepts decimal notation (`1.5`, `-0.25`), scientific notation
    /// (`1e3`, `-1.5E-3`), and the special tokens `inf` / `infinity` / `nan`
    /// (case-insensitive). An optional leading `+` or `-` sign is allowed.
    /// No surrounding whitespace, no underscore separators.
    ///
    /// Throws `ParseError` if `text` cannot be parsed. Note that successfully
    /// parsing `"nan"` gives a NaN value — use `result.is_nan()` to detect it
    /// rather than equality.
    ///
    /// # Examples
    /// ```
    /// float.parse("1.5")        // 1.5
    /// float.parse("-0.25")      // -0.25
    /// float.parse("1e3")        // 1000.0
    /// float.parse("inf")        // +infinity
    /// float.parse("NaN")        // NaN
    /// float.parse("")           // throws — empty
    /// float.parse("hello")      // throws — non-numeric
    /// float.parse(" 1.0 ")      // throws — whitespace not allowed; trim first
    /// ```
    function parse(text: string) -> float throws root.errors.ParseError {
        $rust_function
    }

    /// Returns a uniformly distributed random float in the half-open range
    /// `[0.0, 1.0)`, drawn from `rng`.
    ///
    /// `rng` defaults to `random.SystemRandom`, the host's cryptographic entropy
    /// source; pass a seeded generator to make the draw reproducible. Each call
    /// consumes one `random_int` value and uses 53 random bits.
    ///
    /// # Examples
    /// ```
    /// float.random()              // some x with 0 <= x < 1
    /// float.random() * 100.0      // some x with 0 <= x < 100
    ///
    /// // Reproducible: the same seed replays the same values.
    /// let rng = baml.random.Xoshiro256PlusPlus.new(seed = my_seed);
    /// float.random(rng = rng)
    /// ```
    function random(rng: root.random.Rng = root.random.SystemRandom.get()) -> float throws never {
        float._unit_from_draw(rng.random_int())
    }

    /// Internal: maps 53 bits of an `int` draw onto `[0.0, 1.0)`.
    function _unit_from_draw(draw: int) -> float throws never {
        $rust_function
    }

    // ─── Constants (as zero-arg static methods) ───────────────────────────────

    /// Returns π, the ratio of a circle's circumference to its diameter,
    /// rounded to f64 precision (~15 decimal digits).
    function pi() -> float throws never {
        $rust_function
    }

    /// Returns Euler's number `e`, the base of the natural logarithm.
    function e() -> float throws never {
        $rust_function
    }

    /// Returns the golden ratio `φ = (1 + √5) / 2`, approximately `1.618`.
    function golden_ratio() -> float throws never {
        $rust_function
    }

    /// Returns a NaN value. Use `x.is_nan()` (not equality) to test for NaN.
    function nan() -> float throws never {
        $rust_function
    }

    /// Returns positive infinity. Use `-float.inf()` for negative infinity.
    function inf() -> float throws never {
        $rust_function
    }

    /// Returns the largest finite `float`, approximately
    /// `1.7976931348623157e308`.
    ///
    /// Note this is the largest *finite* value, not the largest value —
    /// `float.inf()` is larger. Named `max_finite` rather than `max_value`
    /// (the `int` spelling) because `float`'s range includes the infinities.
    function max_finite() -> float throws never {
        $rust_function
    }

    /// Returns the smallest (most negative) finite `float`, approximately
    /// `-1.7976931348623157e308`. Equal to `-float.max_finite()`.
    ///
    /// Note this is the most negative finite value, not the smallest positive
    /// one — some languages (notably Java's `Double.MIN_VALUE`) use the latter
    /// meaning for a similarly-named constant.
    function min_finite() -> float throws never {
        $rust_function
    }

    /// Returns the machine epsilon: the difference between `1.0` and the next
    /// larger representable float, approximately `2.220446049250313e-16`.
    ///
    /// This is the standard tolerance unit for approximate float comparison,
    /// e.g. `(a - b).abs() < float.epsilon() * scale`. It is *not* the smallest
    /// positive float, which is far smaller (`5e-324`, subnormal).
    function epsilon() -> float throws never {
        $rust_function
    }
}

// ─── Internal (private) native helpers ────────────────────────────────────────

/// Truncates `value` toward zero and returns the integer part, **saturating**
/// to the `int` range and mapping NaN to `0` — it never throws.
///
/// Private helper (leading `_`; formerly the public `baml.math.trunc`) backing
/// internal retry-delay math. User code should prefer the
/// range-checked, throwing `float.itrunc()` (or the float-returning
/// `float.trunc()`); this saturating form is intentionally not public surface.
function _trunc_to_int(value: float) -> int throws never {
    $rust_function
}
