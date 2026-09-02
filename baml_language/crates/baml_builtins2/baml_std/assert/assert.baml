// Hard assertions — throw and stop execution on failure.

/// Asserts that `condition` is `true`. Panics with a failure message if not.
///
/// # Panics
///
/// - `baml.panics.AssertionFailed` if `condition` is `false`.
///
/// # Examples
///
/// ```baml
/// assert.is_true(true)  // no panic
/// assert.is_true(false)  // panic
/// ```
function is_true(condition: bool) -> void throws never {
    if (!condition) {
        throw baml.panics.AssertionFailed { message: "assertion failed: expected true" }
    }
}

/// Asserts that `value` is not `null`. Panics if it is.
///
/// # Panics
///
/// - `baml.panics.AssertionFailed` if `value` is `null`.
///
/// # Examples
///
/// ```baml
/// assert.not_null(1)  // no panic
/// assert.not_null(null)  // panic
/// ```
function not_null(value: unknown?) -> void throws never {
    if (value == null) {
        throw baml.panics.AssertionFailed { message: "assertion failed: expected non-null value" }
    }
}

/// Asserts that `actual` equals `expected` exactly. Panics if they differ.
/// Uses the `==` operator (`baml.ops.equals_equals`) to compare values.
///
/// When comparing floating-point values, use `assert.approx_equal(actual, expected, eps)`
/// instead to compare within a tolerance.
///
/// # Parameters
///
/// - `actual`: The observed value.
/// - `expected`: The expected value.
///
/// # Panics
///
/// - `baml.panics.AssertionFailed` when `actual != expected`.
function equal(actual: unknown, expected: unknown) -> void throws never {
    if (actual != expected) {
        let actual_str = match (actual) {
            string => `"${actual}"`,
            _ => string.from(actual),
        };
        let expected_str = match (expected) {
            string => `"${expected}"`,
            _ => string.from(expected),
        };

        throw baml.panics.AssertionFailed {
            message: `assertion failed: left = ${actual_str}, right = ${expected_str}`,
        };
    }
}

/// Asserts that two floats are within `eps` of each other.
///
/// # Parameters
///
/// - `actual`: The observed floating-point value.
/// - `expected`: The expected floating-point value.
/// - `eps`: The maximum allowed absolute difference (`|actual - expected|`).
///
/// # Panics
///
/// - `baml.panics.AssertionFailed` when `eps` is negative or NaN.
/// - `baml.panics.AssertionFailed` when the absolute difference exceeds `eps` or cannot be compared
///   (for example, when `actual`/`expected` produce NaN deltas).
function approx_equal(actual: float, expected: float, eps: float) -> void throws never {
    if (eps < 0.0 || eps.is_nan()) {
        throw baml.panics.AssertionFailed {
            message: "assertion failed: epsilon must be a non-negative number",
        };
    }
    let delta = (actual - expected).abs();
    if (delta.is_nan() || delta > eps) {
        throw baml.panics.AssertionFailed {
            message: `assertion failed: |left - right| = "${delta}", eps = "${eps}", left = "${actual}", right = "${expected}"`,
        };
    }
}

/// Asserts that `haystack` contains `needle` as a substring.
///
/// # Parameters
///
/// - `haystack`: The string to search within.
/// - `needle`: The substring to search for.
///
/// # Panics
///
/// - `baml.panics.AssertionFailed` when `haystack` does not contain `needle` as a substring.
function contains(haystack: string, needle: string) -> void throws never {
    if (!haystack.includes(needle)) {
        throw baml.panics.AssertionFailed {
            message: "assertion failed: string does not contain expected substring",
        };
    }
}

/// Asserts that `actual` is of type `T`.
///
/// # Returns
///
/// `actual` if it is of type `T`. This can be used to assert the type of a value and use it in further computations.
///
/// # Panics
///
/// - `baml.panics.AssertionFailed` when `actual` is not of type `T`.
function is_type<T>(actual: unknown) -> T throws never {
    match (actual) {
        T => actual,
        _ => {
            let t: reflect.Type = reflect.Type.of<T>();
            throw baml.panics.AssertionFailed {
                message: `assertion failed: expected type "${t}", got "${actual}"`,
            };
        },
    }
}
