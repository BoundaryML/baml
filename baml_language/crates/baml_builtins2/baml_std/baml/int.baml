class Int {
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
  /// If `min > max`, the result is `min` for `self <= max`, otherwise `max` —
  /// in other words the call effectively snaps to whichever bound was crossed
  /// first. Most callers will pass `min <= max`.
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
