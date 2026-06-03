// Hard assertions — throw and stop execution on failure.

/// Asserts that `condition` is `true`. Panics with a failure message if not.
function is_true(condition: bool) -> null {
  if (!condition) {
    baml.sys.panic("assertion failed: expected true")
  } else {
    null
  }
}

/// Asserts that `value` is not `null`. Panics if it is.
function not_null(value: unknown?) -> null {
  if (value == null) {
    baml.sys.panic("assertion failed: expected non-null value")
  } else {
    null
  }
}

/// Asserts that `actual` equals `expected`. Panics if they differ.
function equal(actual: unknown, expected: unknown) -> null {
  if (actual != expected) {
    baml.sys.panic("assertion failed: expected equal values")
  } else {
    null
  }
}

/// Asserts that `actual` and `expected` floats differ by no more than `epsilon`.
/// Panics with the observed delta if `abs(actual - expected) > epsilon`.
function approx_equal(actual: float, expected: float, epsilon: float) -> null {
  let delta = (actual - expected).abs();
  if (!(delta <= epsilon)) {
    baml.sys.panic(
      "assertion failed: expected approximately equal floats (actual: " +
      baml.unstable.string(actual) +
      ", expected: " +
      baml.unstable.string(expected) +
      ", epsilon: " +
      baml.unstable.string(epsilon) +
      ", delta: " +
      baml.unstable.string(delta) +
      ")"
    )
  } else {
    null
  }
}

/// Asserts that `haystack` contains `needle` as a substring. Panics if not.
function contains(haystack: string, needle: string) -> null {
  if (!haystack.includes(needle)) {
    baml.sys.panic("assertion failed: string does not contain expected substring")
  } else {
    null
  }
}
