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
