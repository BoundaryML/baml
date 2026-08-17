/// A 64-bit IEEE-754 floating-point number.
///
/// Arithmetic follows IEEE-754 throughout, so `float` never panics. Division
/// by zero has an answer: `1.0 / 0.0` is `inf` and `0.0 / 0.0` is `NaN`, as in
/// JavaScript. `int` and `bigint` have no value to give and panic instead.
///
/// `inf` and `NaN` also come from `float.inf()` and `float.nan()`, from
/// overflow (`1e308 * 10.0` is `inf`), and from other invalid operations
/// (`(-1.0).sqrt()` is `NaN`).
///
/// Neither has a JSON representation, so `baml.json.from` renders NaN and both
/// infinities as `null`.
///
/// Comparison departs from raw IEEE: `==` is reflexive and `<`/`<=`/`>`/`>=`
/// are a *total* order, with NaN a single value ordered above every number
/// (`-inf < …reals… < +inf < NaN`) — see `baml.ops.Compare for float`.
class Float {
    // mut_vm: Float values are heap-boxed (Object::Float); construction needs a TLAB.
    // ─── Predicates ───────────────────────────────────────────────────────────

    /// Returns `true` if `self` is NaN — the "not a number" sentinel invalid
    /// operations produce, such as `(-1.0).sqrt()`, `float.inf() - float.inf()`,
    /// or `0.0 / 0.0`.
    ///
    /// This predicate is the only way to test for NaN: BAML's `==` is
    /// reflexive, so `x != x` is false for every float — including NaN, which
    /// equals itself and every other NaN.
    ///
    /// # Examples
    ///
    /// ```baml
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
    ///
    /// ```baml
    /// float.inf().is_infinite()                 // true
    /// (-float.inf()).is_infinite()              // true
    /// (1000000.0 * 1000000.0).is_infinite()     // false  (1e12, large but finite)
    /// float.nan().is_infinite()                 // false
    /// ```
    function is_infinite(self) -> bool throws never {
        $rust_function
    }

    /// Returns `true` if `self` is neither NaN nor infinite. Subnormals and
    /// `±0.0` count as finite.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (3.14).is_finite()         // true
    /// (0.0).is_finite()          // true
    /// float.inf().is_finite()    // false
    /// float.nan().is_finite()    // false
    /// ```
    function is_finite(self) -> bool {
        !self.is_nan() && !self.is_infinite()
    }

    // ─── Magnitude ────────────────────────────────────────────────────────────

    /// Returns the absolute value of `self`. NaN stays NaN.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (-3.5).abs()           // 3.5
    /// (3.5).abs()            // 3.5
    /// (-0.0).abs()           // 0.0
    /// float.nan().abs()      // NaN
    /// ```
    function abs(self) -> float throws never {
        $rust_function
    }

    // `min` / `max` / `clamp` are NOT declared here. They come from
    // `baml.ops.Compare`, which `float` implements — one definition, driven by
    // the total float order, so `(3.0).max(x)` cannot mean something different
    // from the `max` a `T extends baml.ops.Compare` bound reaches. Note the
    // total order makes NaN the largest float, so it wins a `max` and loses a
    // `min` rather than being suppressed the way IEEE `minNum`/`maxNum` and
    // Rust's `f64::min`/`f64::max` suppress it.

    // ─── Rounding (returns float) ─────────────────────────────────────────────
    //
    // These return `float` because the rounded value may exceed the `int`
    // range; for fallible conversions to `int`, use the `i*` variants below.

    /// Returns the largest integer less than or equal to `self`, as a float.
    /// The sign of `±0.0` is preserved; NaN and `±inf` pass through.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (3.7).floor()    // 3.0
    /// (-1.5).floor()   // -2.0
    /// (3.0).floor()    // 3.0
    /// ```
    function floor(self) -> float throws never {
        $rust_function
    }

    /// Returns the smallest integer greater than or equal to `self`, as a
    /// float. The sign of `±0.0` is preserved; NaN and `±inf` pass through.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (3.2).ceil()    // 4.0
    /// (-1.5).ceil()   // -1.0
    /// ```
    function ceil(self) -> float throws never {
        $rust_function
    }

    /// Returns the integer nearest to `self`, as a float, with ties rounded
    /// away from zero: `0.5` to `1.0`, `-0.5` to `-1.0`, `1.5` to `2.0`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (1.5).round()    // 2.0
    /// (-1.5).round()   // -2.0
    /// (1.4).round()    // 1.0
    /// (1.6).round()    // 2.0
    /// ```
    function round(self) -> float throws never {
        $rust_function
    }

    /// Returns the integer part of `self`, discarding the fraction — that is,
    /// `self` rounded toward zero.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (3.7).trunc()    // 3.0
    /// (-3.7).trunc()   // -3.0
    /// ```
    function trunc(self) -> float throws never {
        $rust_function
    }

    /// Returns the fractional part of `self`, `self - self.trunc()`.
    ///
    /// # Examples
    ///
    /// ```baml
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
    // rounding — falls outside the `int` (`i63`) range. The exact threshold is
    // `[-2^62, 2^62)`; values strictly less than `int.min_value()` or ≥ `2^62`
    // (one past `int.max_value()`, the smallest value not representable as `int`)
    // throw.

    /// Like `floor`, but returns `int`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `self` is NaN or `±inf`, or rounds to
    ///   a value outside the `int` range.
    function ifloor(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Like `ceil`, but returns `int`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `self` is NaN or `±inf`, or rounds to
    ///   a value outside the `int` range.
    function iceil(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Like `round`, but returns `int`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `self` is NaN or `±inf`, or rounds to
    ///   a value outside the `int` range.
    function iround(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Like `trunc`, but returns `int`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `self` is NaN or `±inf`, or rounds to
    ///   a value outside the `int` range.
    function itrunc(self) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    // ─── Power / logarithm ────────────────────────────────────────────────────

    /// Returns the square root of `self`, or NaN if `self` is negative.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (4.0).sqrt()       // 2.0
    /// (2.0).sqrt()       // 1.4142...
    /// (-1.0).sqrt()      // NaN
    /// (0.0).sqrt()       // 0.0
    /// ```
    function sqrt(self) -> float throws never {
        $rust_function
    }

    /// Returns `self ** exp`, computed in floating point.
    ///
    /// Gives NaN wherever the mathematical value is not real, such as a
    /// negative base raised to a non-integer power.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (2.0).pow(10.0)         // 1024.0
    /// (2.0).pow(-1.0)         // 0.5
    /// (2.0).pow(0.5)          // sqrt(2)
    /// (-1.0).pow(0.5)         // NaN
    /// ```
    function pow(self, exp: float) -> float throws never {
        $rust_function
    }

    /// Returns the logarithm of `self` in the given `base`, computed as
    /// `ln(self) / ln(base)`.
    ///
    /// Every boundary case falls out of that division:
    ///
    /// - `self < 0` gives NaN, since `ln` of a negative is NaN.
    /// - `self == 0` with `base > 1` gives `-inf`: `-inf / positive`.
    /// - `self == 0` with `0 < base < 1` gives `+inf`; `ln(base)` is negative,
    ///   so the sign flips relative to the case above.
    /// - `self == 0` with `base == 1` gives `-inf`, since `ln(1)` is `+0.0`.
    /// - `base < 0` gives NaN.
    /// - `base == 0` with `self > 0` gives `±0`, signed opposite `ln(self)`,
    ///   since `±finite / -inf` is `∓0`.
    /// - `base == 1` with `self > 0` and `self != 1` gives `±inf`, signed to
    ///   match `ln(self)`, since `±finite / +0.0` is `±inf`.
    /// - `base == 1` with `self == 1` gives NaN: `0 / 0`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// (1000.0).log(10.0)   // 3.0
    /// (8.0).log(2.0)       // 3.0
    /// (-1.0).log(10.0)     // NaN
    /// (0.0).log(10.0)      // -infinity
    /// (0.0).log(0.5)       // +infinity
    /// (2.0).log(1.0)       // +infinity
    /// (1.0).log(1.0)       // NaN
    /// ```
    function log(self, base: float) -> float throws never {
        $rust_function
    }

    /// Returns the Euclidean distance `sqrt(self² + other²)`, computed so that
    /// large magnitudes do not overflow along the way.
    ///
    /// # Examples
    ///
    /// ```baml
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

    /// Sine of `self`, in radians.
    function sin(self) -> float throws never {
        $rust_function
    }

    /// Cosine of `self`, in radians.
    function cos(self) -> float throws never {
        $rust_function
    }

    /// Tangent of `self`, in radians. Very large near odd multiples of π/2.
    function tan(self) -> float throws never {
        $rust_function
    }

    /// Inverse sine, in `[-π/2, π/2]`. NaN if `self` is outside `[-1, 1]`.
    function asin(self) -> float throws never {
        $rust_function
    }

    /// Inverse cosine, in `[0, π]`. NaN if `self` is outside `[-1, 1]`.
    function acos(self) -> float throws never {
        $rust_function
    }

    /// Inverse tangent, in `(-π/2, π/2)`.
    function atan(self) -> float throws never {
        $rust_function
    }

    /// Returns the angle in radians from the positive x-axis to the point
    /// `(x = other, y = self)`, in `[-π, π]`.
    ///
    /// # Examples
    ///
    /// ```baml
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

    /// Hyperbolic tangent, bounded to `(-1, 1)`.
    function tanh(self) -> float throws never {
        $rust_function
    }

    /// Inverse hyperbolic sine, defined for every real.
    function asinh(self) -> float throws never {
        $rust_function
    }

    /// Inverse hyperbolic cosine, defined on `[1, ∞)`. NaN for `self < 1`.
    function acosh(self) -> float throws never {
        $rust_function
    }

    /// Inverse hyperbolic tangent, defined on `(-1, 1)`. `±∞` at the
    /// endpoints, NaN outside them.
    function atanh(self) -> float throws never {
        $rust_function
    }

    // ─── Angle conversions ────────────────────────────────────────────────────

    /// Converts `self` from degrees to radians.
    function to_radians(self) -> float throws never {
        self * float.pi() / 180.0
    }

    /// Converts `self` from radians to degrees.
    function to_degrees(self) -> float throws never {
        self * 180.0 / float.pi()
    }

    // ─── Parsing / randomness ─────────────────────────────────────────────────

    /// Parses `text` as a floating-point number.
    ///
    /// Accepts decimal notation (`1.5`, `-0.25`), scientific notation (`1e3`,
    /// `-1.5E-3`), and the case-insensitive tokens `inf`, `infinity`, and
    /// `nan`, each with an optional leading `+` or `-`. No surrounding
    /// whitespace, no underscore separators.
    ///
    /// Parsing `"nan"` succeeds and yields a NaN value, so test the result with
    /// `is_nan()` rather than equality.
    ///
    /// # Throws
    ///
    /// - `baml.errors.ParseError` if `text` cannot be parsed.
    ///
    /// # Examples
    ///
    /// ```baml
    /// float.parse("1.5")        // 1.5
    /// float.parse("-0.25")      // -0.25
    /// float.parse("1e3")        // 1000.0
    /// float.parse("inf")        // +infinity
    /// float.parse("NaN")        // NaN
    /// float.parse("")           // throws — empty
    /// float.parse("hello")      // throws — non-numeric
    /// float.parse(" 1.0 ")      // throws — whitespace; trim first
    /// ```
    function parse(text: string) -> float throws root.errors.ParseError {
        $rust_function
    }

    /// Returns a uniformly distributed random float in the half-open range
    /// `[0.0, 1.0)`, drawn from `rng`.
    ///
    /// `rng` defaults to `random.SystemRandom`, the host's cryptographic entropy
    /// source; pass a seeded generator to make the draw reproducible. Each call
    /// consumes one `random_int` value and uses 53 of its bits.
    ///
    /// # Examples
    ///
    /// ```baml
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

    /// (internal) Maps 53 bits of an `int` draw onto `[0.0, 1.0)`.
    function _unit_from_draw(draw: int) -> float throws never {
        $rust_function
    }

    // ─── Constants (as zero-arg static methods) ───────────────────────────────

    /// Returns π, the ratio of a circle's circumference to its diameter, to
    /// float precision (about 15 decimal digits).
    function pi() -> float throws never {
        $rust_function
    }

    /// Returns Euler's number `e`, the base of the natural logarithm.
    function e() -> float throws never {
        $rust_function
    }

    /// Returns the golden ratio `φ = (1 + √5) / 2`, about `1.618`.
    function golden_ratio() -> float throws never {
        $rust_function
    }

    /// Returns a NaN value. Test for it with `x.is_nan()`, never with equality.
    function nan() -> float throws never {
        $rust_function
    }

    /// Returns positive infinity. Negate it — `-float.inf()` — for negative
    /// infinity.
    function inf() -> float throws never {
        $rust_function
    }
}

// ─── Internal (private) native helpers ────────────────────────────────────────

/// (internal) Truncates `value` toward zero and returns the integer part,
/// saturating to the full 64-bit machine range rather than BAML's narrower
/// 63-bit `int` range, and mapping NaN to `0`. It never throws, so a saturated
/// result for a very large magnitude or `±∞` input can itself land outside
/// `[int.min_value(), int.max_value()]`.
///
/// Backs internal retry-delay math, whose inputs stay well within `int`'s range
/// in practice. User code should prefer the range-checked `float.itrunc()`, or
/// the float-returning `float.trunc()`; this saturating form is deliberately
/// not public surface.
function _trunc_to_int(value: float) -> int throws never {
    $rust_function
}
