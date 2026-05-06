class Float {
  function to_json(self) -> baml.json.json {
    $rust_function
  }
  // ─── Predicates ───────────────────────────────────────────────────────────

  /// Returns `true` if `self` is NaN (a "not a number" sentinel produced by
  /// invalid operations such as `0.0 / 0.0` or `(-1.0).sqrt()`).
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
  function is_nan(self) -> bool {
    $rust_function
  }

  /// Returns `true` if `self` is positive or negative infinity.
  ///
  /// # Examples
  /// ```
  /// float.inf().is_infinite()        // true
  /// (-float.inf()).is_infinite()     // true
  /// (1e308).is_infinite()            // false  (large but finite)
  /// float.nan().is_infinite()        // false
  /// ```
  function is_infinite(self) -> bool {
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
  function abs(self) -> float {
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
  function min(self, other: float) -> float {
    $rust_function
  }

  /// Returns the larger of `self` and `other`.
  ///
  /// NaN handling matches `min`: a non-NaN operand is preferred over NaN.
  function max(self, other: float) -> float {
    $rust_function
  }

  /// Clamps `self` into the range `[min, max]`.
  ///
  /// If `min > max`, the result is undefined-but-deterministic: this returns
  /// `max` for `self <= max`, otherwise `min` (the same step-by-step semantics
  /// as `int.clamp`). Most callers should pass `min <= max`.
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
  function clamp(self, min: float, max: float) -> float {
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
  function floor(self) -> float {
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
  function ceil(self) -> float {
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
  function round(self) -> float {
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
  function trunc(self) -> float {
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
  function fract(self) -> float {
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
  function sqrt(self) -> float {
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
  function pow(self, exp: float) -> float {
    $rust_function
  }

  /// Returns the logarithm of `self` in the given `base`.
  /// Returns NaN if `self <= 0`, `base <= 0`, or `base == 1`.
  ///
  /// # Examples
  /// ```
  /// (1000.0).log(10.0)   // 3.0
  /// (8.0).log(2.0)       // 3.0
  /// (-1.0).log(10.0)     // NaN
  /// ```
  function log(self, base: float) -> float {
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
  function hypot(self, other: float) -> float {
    $rust_function
  }

  // ─── Trigonometry (radians) ───────────────────────────────────────────────
  //
  // All angle arguments and return values are in radians. Use `to_radians()`
  // to convert from degrees (or `to_degrees()` to convert results back).

  /// Sine of `self` (in radians).
  function sin(self) -> float {
    $rust_function
  }

  /// Cosine of `self` (in radians).
  function cos(self) -> float {
    $rust_function
  }

  /// Tangent of `self` (in radians). Very large near odd multiples of π/2.
  function tan(self) -> float {
    $rust_function
  }

  /// Inverse sine, returning a value in `[-π/2, π/2]`.
  /// Returns NaN if `self` is outside `[-1, 1]`.
  function asin(self) -> float {
    $rust_function
  }

  /// Inverse cosine, returning a value in `[0, π]`.
  /// Returns NaN if `self` is outside `[-1, 1]`.
  function acos(self) -> float {
    $rust_function
  }

  /// Inverse tangent, returning a value in `(-π/2, π/2)`.
  function atan(self) -> float {
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
  function atan2(self, other: float) -> float {
    $rust_function
  }

  /// Hyperbolic sine.
  function sinh(self) -> float {
    $rust_function
  }

  /// Hyperbolic cosine.
  function cosh(self) -> float {
    $rust_function
  }

  /// Hyperbolic tangent. Bounded to `(-1, 1)`.
  function tanh(self) -> float {
    $rust_function
  }

  /// Inverse hyperbolic sine. Domain: all reals.
  function asinh(self) -> float {
    $rust_function
  }

  /// Inverse hyperbolic cosine. Domain: `[1, ∞)`. Returns NaN for `self < 1`.
  function acosh(self) -> float {
    $rust_function
  }

  /// Inverse hyperbolic tangent. Domain: `(-1, 1)`. Returns NaN outside that
  /// range, and ±∞ at the endpoints.
  function atanh(self) -> float {
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

  // ─── Constants (as zero-arg static methods) ───────────────────────────────

  /// Returns π, the ratio of a circle's circumference to its diameter,
  /// rounded to f64 precision (~15 decimal digits).
  function pi() -> float {
    $rust_function
  }

  /// Returns Euler's number `e`, the base of the natural logarithm.
  function e() -> float {
    $rust_function
  }

  /// Returns the golden ratio `φ = (1 + √5) / 2`, approximately `1.618`.
  function golden_ratio() -> float {
    $rust_function
  }

  /// Returns a NaN value. Use `x.is_nan()` (not equality) to test for NaN.
  function nan() -> float {
    $rust_function
  }

  /// Returns positive infinity. Use `-float.inf()` for negative infinity.
  function inf() -> float {
    $rust_function
  }
}
