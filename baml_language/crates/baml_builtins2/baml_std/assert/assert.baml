// Hard assertions — throw and stop execution on failure.

function is_true(condition: bool) -> null {
  if (!condition) {
    baml.sys.panic("assertion failed: expected true")
  } else {
    null
  }
}

function not_null(value: unknown?) -> null {
  if (value == null) {
    baml.sys.panic("assertion failed: expected non-null value")
  } else {
    null
  }
}

function equal(actual: unknown, expected: unknown) -> null {
  if (actual != expected) {
    baml.sys.panic("assertion failed: expected equal values")
  } else {
    null
  }
}

function contains(haystack: string, needle: string) -> null {
  if (haystack.includes(needle) == false) {
    baml.sys.panic("assertion failed: string does not contain expected substring")
  } else {
    null
  }
}
