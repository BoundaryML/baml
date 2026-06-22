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

/// Converts any assertion operand into a user-facing string.
///
/// Parameters:
/// - `value`: The operand value to render.
///
/// Returns:
/// - A JSON-style string representation for panic messages.
///
/// Errors/Panics:
/// - Never throws. Falls back to a placeholder when serialization fails.
function format_operand(value: unknown) -> string {
  string.from(value)
}

/// Asserts that `actual` equals `expected`. Panics if they differ.
function equal(actual: unknown, expected: unknown) -> null {
  if (actual != expected) {
    baml.sys.panic(
      "assertion failed: left = " +
      format_operand(actual) +
      ", right = " +
      format_operand(expected)
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
