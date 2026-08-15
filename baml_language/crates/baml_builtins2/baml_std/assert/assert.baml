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

/// Renders one operand of a failed assertion.
///
/// Quoted if it is a string, bare otherwise, so the reader can tell the two
/// apart. Interpolating every operand inside literal quotes printed an int as
/// `"1"` — a number wearing a string's clothes — and interpolating none printed
/// `assert.equal(1, "1")` as `left = 1, right = 1`: two identical-looking
/// values for an assertion that failed precisely because they differ.
///
/// Parameters:
/// - `value`: The operand value to render.
///
/// Returns:
/// - A JSON-style string representation for panic messages.
///
/// Errors/Panics:
/// - Never throws.
function format_operand(value: unknown) -> string {
    match (value) {
        let text: string => `"${text}"`,
        _ => string.from(value),
    }
}

/// Asserts that `actual` equals `expected` exactly. Panics if they differ.
///
/// Parameters:
/// - `actual`: The observed value.
/// - `expected`: The expected value.
///
/// Returns:
/// - `null` when `actual` and `expected` are exactly equal.
///
/// Errors/Panics:
/// - Panics when `actual != expected`.
/// - For floating-point values this comparison is exact; prefer
///   `assert.approx_equal(actual, expected, eps)` when comparing computed
///   results within a tolerance.
function equal(actual: unknown, expected: unknown) -> null {
    if (actual != expected) {
        baml.sys.panic(
            `assertion failed: left = ${format_operand(actual)}, right = ${format_operand(expected)}`,
        )
    } else {
        null
    }
}

/// Asserts that two floats are within `eps` of each other.
///
/// Parameters:
/// - `actual`: The observed floating-point value.
/// - `expected`: The expected floating-point value.
/// - `eps`: The maximum allowed absolute difference (`|actual - expected|`).
///
/// Returns:
/// - `null` when `|actual - expected| <= eps`.
///
/// Errors/Panics:
/// - Panics when `eps` is negative or NaN.
/// - Panics when the absolute difference exceeds `eps` or cannot be compared
///   (for example, when `actual`/`expected` produce NaN deltas).
function approx_equal(actual: float, expected: float, eps: float) -> null {
    if (eps < 0.0 || eps.is_nan()) {
        baml.sys.panic("assertion failed: epsilon must be a non-negative number")
    } else {
        let delta = (actual - expected).abs();
        if (delta.is_nan() || delta > eps) {
            baml.sys.panic(
                `assertion failed: |left - right| = "${delta}", eps = "${eps}", left = "${actual}", right = "${expected}"`,
            )
        } else {
            null
        }
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

/// Asserts that `actual` is of type `T`. Panics if not.
function is_type<T>(actual: unknown) -> T throws never {
    match (actual) {
        T => actual,
        _ => {
            let t: type = type.of<T>();
            baml.sys.panic(`assertion failed: expected type "${t}", got "${actual}"`)
        },
    }
}
