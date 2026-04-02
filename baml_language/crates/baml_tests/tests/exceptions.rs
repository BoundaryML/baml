//! Exception handling tests: catch/throw/panic semantics.
//!
//! Progression from simple to complex, covering all catch arm pattern types:
//!   - Literal value patterns: "string" =>, 42 =>
//!   - Typed bindings: _: string =>, _: MyClass =>
//!   - Bare type sugar: DivisionByZero =>
//!   - Wildcard: _ =>
//!   - User-defined error classes
//!   - Multi-arm dispatch with mixed pattern types
//!   - Panics vs user throws
//!   - Nested, rethrow, sequential

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// §1 — Catch by literal value
// ============================================================================

#[tokio::test]
#[ignore = "TIR throw inference: literal patterns not recognized as matching throw type"]
async fn catch_literal_string_match() {
    let output = baml_test!(
        r#"
        function fails() -> int {
            throw "boom";
        }

        function main() -> int {
            fails() catch (e) {
                "boom" => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[ignore = "TIR throw inference: literal patterns not recognized as matching throw type"]
async fn catch_literal_string_no_match_falls_to_wildcard() {
    let output = baml_test!(
        r#"
        function fails() -> int {
            throw "other";
        }

        function main() -> int {
            fails() catch (e) {
                "boom" => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
#[ignore = "TIR throw inference: literal patterns not recognized as matching throw type"]
async fn catch_literal_int_match() {
    let output = baml_test!(
        r#"
        function fails() -> int {
            throw 42;
        }

        function main() -> int {
            fails() catch (e) {
                42 => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[ignore = "TIR throw inference: literal patterns not recognized as matching throw type"]
async fn catch_multiple_literals_dispatch() {
    let output = baml_test!(
        r#"
        function fails(mode: int) -> int {
            if (mode == 0) { throw "alpha" }
            if (mode == 1) { throw "beta" }
            throw "gamma"
        }

        function main() -> int {
            fails(1) catch (e) {
                "alpha" => 1,
                "beta" => 2,
                _ => 3
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// ============================================================================
// §2 — Catch by typed binding (_: Type =>)
// ============================================================================

#[tokio::test]
async fn typed_binding_string() {
    let output = baml_test!(
        r#"
        function fails() -> int {
            throw "boom";
        }

        function main() -> int {
            fails() catch (e) {
                _: string => 1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn typed_binding_int() {
    let output = baml_test!(
        r#"
        function fails() -> int {
            throw 42;
        }

        function main() -> int {
            fails() catch (e) {
                _: int => 1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn typed_binding_dispatch_string_vs_int() {
    let output = baml_test!(
        r#"
        function fails(mode: int) -> int {
            if (mode == 0) { throw "boom" }
            throw 42
        }

        function main() -> int {
            fails(0) catch (e) {
                _: string => 1,
                _: int => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn typed_binding_dispatch_int_vs_string() {
    let output = baml_test!(
        r#"
        function fails(mode: int) -> int {
            if (mode == 0) { throw "boom" }
            throw 42
        }

        function main() -> int {
            fails(1) catch (e) {
                _: string => 1,
                _: int => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn typed_binding_plus_wildcard() {
    let output = baml_test!(
        r#"
        function fails(mode: int) -> int {
            if (mode == 0) { throw "boom" }
            throw 42
        }

        function main() -> int {
            fails(1) catch (e) {
                _: string => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// ============================================================================
// §3 — Catch by user-defined class type
// ============================================================================

#[tokio::test]
async fn catch_user_class_single_arm() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }

        function fails() -> int {
            throw NetworkError { url: "http://example.com" }
        }

        function main() -> int {
            fails() catch (e) {
                _: NetworkError => 1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn catch_two_user_classes_dispatch_first() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }
        class ParseError { message string }

        function fails(mode: int) -> int {
            if (mode == 0) { throw NetworkError { url: "http://x" } }
            throw ParseError { message: "bad json" }
        }

        function main() -> int {
            fails(0) catch (e) {
                _: NetworkError => 1,
                _: ParseError => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn catch_two_user_classes_dispatch_second() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }
        class ParseError { message string }

        function fails(mode: int) -> int {
            if (mode == 0) { throw NetworkError { url: "http://x" } }
            throw ParseError { message: "bad json" }
        }

        function main() -> int {
            fails(1) catch (e) {
                _: NetworkError => 1,
                _: ParseError => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn catch_user_class_plus_wildcard() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }

        function fails(mode: int) -> int {
            if (mode == 0) { throw NetworkError { url: "http://x" } }
            throw "plain string error"
        }

        function main() -> int {
            fails(1) catch (e) {
                _: NetworkError => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn catch_three_user_classes_plus_wildcard() {
    let output = baml_test!(
        r#"
        class AuthError { reason string }
        class NotFound { path string }
        class RateLimit { retryAfter int }

        function api(mode: int) -> int {
            if (mode == 0) { throw AuthError { reason: "expired" } }
            if (mode == 1) { throw NotFound { path: "/users" } }
            if (mode == 2) { throw RateLimit { retryAfter: 30 } }
            throw "unknown"
        }

        function main() -> int {
            api(2) catch (e) {
                _: AuthError => 1,
                _: NotFound => 2,
                _: RateLimit => 3,
                _ => 4
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// §3b — Catch by bare class name (MyClass => without _: binding)
// ============================================================================

#[tokio::test]
async fn bare_class_single_arm() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }

        function fails() -> int {
            throw NetworkError { url: "http://example.com" }
        }

        function main() -> int {
            fails() catch (e) {
                NetworkError => 1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn bare_class_dispatch_first() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }
        class ParseError { message string }

        function fails(mode: int) -> int {
            if (mode == 0) { throw NetworkError { url: "http://x" } }
            throw ParseError { message: "bad" }
        }

        function main() -> int {
            fails(0) catch (e) {
                NetworkError => 1,
                ParseError => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn bare_class_dispatch_second() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }
        class ParseError { message string }

        function fails(mode: int) -> int {
            if (mode == 0) { throw NetworkError { url: "http://x" } }
            throw ParseError { message: "bad" }
        }

        function main() -> int {
            fails(1) catch (e) {
                NetworkError => 1,
                ParseError => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn bare_class_plus_wildcard() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }

        function fails(mode: int) -> int {
            if (mode == 0) { throw NetworkError { url: "http://x" } }
            throw "plain string"
        }

        function main() -> int {
            fails(0) catch (e) {
                NetworkError => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn bare_class_plus_wildcard_wildcard_fires() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }

        function fails(mode: int) -> int {
            if (mode == 0) { throw NetworkError { url: "http://x" } }
            throw "plain string"
        }

        function main() -> int {
            fails(1) catch (e) {
                NetworkError => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// ============================================================================
// §4 — Unhandled throws propagate
// ============================================================================

#[tokio::test]
async fn unhandled_throw_string() {
    let output = baml_test!(
        r#"
        function main() -> string {
            throw "something went wrong";
        }
    "#
    );
    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::UnhandledThrow {
            value: Box::new(BexExternalValue::String("something went wrong".to_string())),
        })
    );
}

#[tokio::test]
async fn unhandled_throw_int() {
    let output = baml_test!(
        r#"
        function main() -> int {
            throw 42;
        }
    "#
    );
    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::UnhandledThrow {
            value: Box::new(BexExternalValue::Int(42)),
        })
    );
}

// ============================================================================
// §5 — Panic patterns: single arm per panic type
// ============================================================================

#[tokio::test]
async fn catch_division_by_zero() {
    let output = baml_test!(
        r#"
        function divides() -> int { 1 / 0 }

        function main() -> int {
            divides() catch (e) { DivisionByZero => -1 }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn catch_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function oob() -> int { let a = [1, 2]; a[5] }

        function main() -> int {
            oob() catch (e) { IndexOutOfBounds => -1 }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn catch_map_key_not_found() {
    let output = baml_test!(
        r#"
        function bad() -> int { let m = {"a": 1}; m["x"] }

        function main() -> int {
            bad() catch (e) { MapKeyNotFound => -1 }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn catch_negative_index_as_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function bad() -> int { let a = [1, 2]; a[-1] }

        function main() -> int {
            bad() catch (e) { IndexOutOfBounds => -1 }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ============================================================================
// §6 — Wildcard does NOT catch panics
// ============================================================================

#[tokio::test]
async fn wildcard_skips_division_by_zero() {
    let output = baml_test!(
        r#"
        function risky(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            risky(0) catch (e) { _ => -1 }
        }
    "#
    );
    assert!(output.result.is_err(), "panic should propagate past _");
}

#[tokio::test]
async fn wildcard_skips_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function risky(x: int) -> int {
            if (x > 100) { throw "too big" }
            let a = [1, 2, 3];
            a[x]
        }

        function main() -> int {
            risky(99) catch (e) { _ => -1 }
        }
    "#
    );
    assert!(output.result.is_err(), "panic should propagate past _");
}

// ============================================================================
// §7 — Multi-arm: panics + user errors in same catch
// ============================================================================

#[tokio::test]
async fn panic_arm_plus_wildcard_panic_fires() {
    let output = baml_test!(
        r#"
        function risky(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            risky(0) catch (e) {
                DivisionByZero => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn panic_arm_plus_wildcard_user_error_fires() {
    let output = baml_test!(
        r#"
        function risky(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            risky(999) catch (e) {
                DivisionByZero => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn panic_arm_plus_wildcard_no_error() {
    let output = baml_test!(
        r#"
        function risky(x: int) -> int {
            if (x > 100) { throw "too big" }
            1000 / x
        }

        function main() -> int {
            risky(5) catch (e) {
                DivisionByZero => -1,
                _ => -2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(200)));
}

// ============================================================================
// §8 — Multi-arm: user class + panic + wildcard
// ============================================================================

#[tokio::test]
async fn user_class_plus_panic_plus_wildcard_class_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }

        function risky(mode: int) -> int {
            if (mode == 0) { throw AppError { code: 500 } }
            if (mode == 1) { 1 / 0 }
            throw "fallback"
        }

        function main() -> int {
            risky(0) catch (e) {
                _: AppError => 1,
                DivisionByZero => 2,
                _ => 3
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn user_class_plus_panic_plus_wildcard_panic_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }

        function do_div() -> int { 1 / 0 }

        function risky(mode: int) -> int {
            if (mode == 0) { throw AppError { code: 500 } }
            if (mode == 1) { return do_div() }
            throw "fallback"
        }

        function main() -> int {
            risky(1) catch (e) {
                _: AppError => 1,
                DivisionByZero => 2,
                _ => 3
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn user_class_plus_panic_plus_wildcard_string_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }

        function do_div() -> int { 1 / 0 }

        function risky(mode: int) -> int {
            if (mode == 0) { throw AppError { code: 500 } }
            if (mode == 1) { return do_div() }
            throw "fallback"
        }

        function main() -> int {
            risky(2) catch (e) {
                _: AppError => 1,
                DivisionByZero => 2,
                _ => 3
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// §9 — Four-arm: two panics + user class + wildcard
// ============================================================================

#[tokio::test]
async fn four_arms_division_by_zero_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }
        function do_div() -> int { 1 / 0 }
        function do_oob() -> int { let a = [1]; a[5] }

        function risky(mode: int) -> int {
            if (mode == 0) { return do_div() }
            if (mode == 1) { return do_oob() }
            if (mode == 2) { throw AppError { code: 404 } }
            throw "unknown"
        }

        function main() -> int {
            risky(0) catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2,
                _: AppError => 3,
                _ => 4
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn four_arms_index_out_of_bounds_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }
        function do_div() -> int { 1 / 0 }
        function do_oob() -> int { let a = [1]; a[5] }

        function risky(mode: int) -> int {
            if (mode == 0) { return do_div() }
            if (mode == 1) { return do_oob() }
            if (mode == 2) { throw AppError { code: 404 } }
            throw "unknown"
        }

        function main() -> int {
            risky(1) catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2,
                _: AppError => 3,
                _ => 4
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn four_arms_user_class_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }
        function do_div() -> int { 1 / 0 }
        function do_oob() -> int { let a = [1]; a[5] }

        function risky(mode: int) -> int {
            if (mode == 0) { return do_div() }
            if (mode == 1) { return do_oob() }
            if (mode == 2) { throw AppError { code: 404 } }
            throw "unknown"
        }

        function main() -> int {
            risky(2) catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2,
                _: AppError => 3,
                _ => 4
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn four_arms_wildcard_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }
        function do_div() -> int { 1 / 0 }
        function do_oob() -> int { let a = [1]; a[5] }

        function risky(mode: int) -> int {
            if (mode == 0) { return do_div() }
            if (mode == 1) { return do_oob() }
            if (mode == 2) { throw AppError { code: 404 } }
            throw "unknown"
        }

        function main() -> int {
            risky(3) catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2,
                _: AppError => 3,
                _ => 4
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

#[tokio::test]
async fn four_arms_no_error() {
    let output = baml_test!(
        r#"
        class AppError { code int }
        function do_div() -> int { 1 / 0 }
        function do_oob() -> int { let a = [1]; a[5] }

        function risky(mode: int) -> int {
            if (mode == 0) { return do_div() }
            if (mode == 1) { return do_oob() }
            if (mode == 2) { throw AppError { code: 404 } }
            if (mode == 3) { throw "unknown" }
            99
        }

        function main() -> int {
            risky(4) catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2,
                _: AppError => 3,
                _ => 4
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

// ============================================================================
// §10 — Uncaught panics propagate
// ============================================================================

#[tokio::test]
async fn uncaught_division_by_zero() {
    let output = baml_test!(r#" function main() -> int { 1 / 0 } "#);
    assert!(output.result.is_err());
}

#[tokio::test]
async fn uncaught_index_out_of_bounds() {
    let output = baml_test!(r#" function main() -> int { let a = [1]; a[5] } "#);
    assert!(output.result.is_err());
}

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn wrong_panic_pattern_propagates() {
    let output = baml_test!(
        r#"
        function divides() -> int { 1 / 0 }

        function main() -> int {
            divides() catch (e) { IndexOutOfBounds => -1 }
        }
    "#
    );
    assert!(
        output.result.is_err(),
        "DivisionByZero should propagate past IndexOutOfBounds arm"
    );
}

// ============================================================================
// §11 — Panic type alias (union of all panics)
// ============================================================================

#[tokio::test]
#[ignore = "IsType emit doesn't handle union types — Panic alias expands to a union"]
async fn panic_alias_catches_any_panic() {
    let output = baml_test!(
        r#"
        function divides() -> int { 1 / 0 }

        function main() -> int {
            divides() catch (e) { Panic => -1 }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
#[ignore = "IsType emit doesn't handle union types — Panic alias expands to a union"]
async fn panic_alias_plus_wildcard_dispatch() {
    let output = baml_test!(
        r#"
        function risky(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            risky(0) catch (e) {
                Panic => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ============================================================================
// §12 — Nested catch, rethrow, sequential
// ============================================================================

#[tokio::test]
async fn nested_inner_catches_outer_does_not_fire() {
    let output = baml_test!(
        r#"
        function inner() -> int { throw "inner" }

        function middle() -> int {
            inner() catch (e) { _ => 42 }
        }

        function main() -> int {
            middle() catch (e) { _ => -1 }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn nested_inner_catches_panic_outer_catches_rethrow() {
    let output = baml_test!(
        r#"
        function divides() -> int { 1 / 0 }

        function middle() -> int {
            let x = divides() catch (e) { DivisionByZero => -1 };
            throw "recovered but failing"
        }

        function main() -> int {
            middle() catch (e) { _ => 99 }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn rethrow_propagates_to_outer() {
    let output = baml_test!(
        r#"
        function inner() -> int { throw "original" }

        function middle() -> int {
            inner() catch (e) { _ => throw "rethrown" }
        }

        function main() -> int {
            middle() catch (e) { _ => 99 }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn sequential_catches_both_recover() {
    let output = baml_test!(
        r#"
        function f1() -> int { throw "one" }
        function f2() -> int { throw "two" }

        function main() -> int {
            let a = f1() catch (e) { _ => 10 };
            let b = f2() catch (e) { _ => 20 };
            a + b
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

// ============================================================================
// §13 — Special cases
// ============================================================================

#[tokio::test]
async fn inline_throw_catch() {
    let output = baml_test!(
        "
        function main() -> int {
            return throw 1 catch (e) { _ => 2 };
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn throw_in_match_arm_propagates() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let x = 2;
            match (x) {
                1 => "one",
                _ => { throw "boom" },
            }
        }
    "#
    );
    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::UnhandledThrow {
            value: Box::new(BexExternalValue::String("boom".to_string())),
        })
    );
}

#[tokio::test]
#[ignore = "needs catch arm type narrowing to access fields on specific panic type"]
async fn caught_panic_has_accessible_fields() {
    let output = baml_test!(
        r#"
        function oob() -> int { let a = [10, 20, 30]; a[7] }

        function main() -> int {
            oob() catch (e) { IndexOutOfBounds => e.index }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}
