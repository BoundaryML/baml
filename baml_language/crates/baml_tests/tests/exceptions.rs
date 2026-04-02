//! Exception handling tests: catch/throw/panic semantics.
//!
//! Progression from simple to complex:
//!   §1  Single arm: wildcard catches user throw
//!   §2  Single arm: typed binding catches matching throw
//!   §3  Multi-arm: typed bindings dispatch by throw type
//!   §4  Unhandled throws propagate
//!   §5  Single arm: explicit panic pattern catches
//!   §6  Single arm: wildcard does NOT catch panics
//!   §7  Multi-arm: multiple panic patterns dispatch
//!   §8  Multi-arm: panic pattern + wildcard (errors + panics)
//!   §9  Three-arm: two panics + wildcard (full combinatoric)
//!   §10 Unmatched panic propagates past catch
//!   §11 Panic type alias
//!   §12 Nested catch, rethrow, sequential
//!   §13 Special cases

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// §1 — Single arm: wildcard catches user throw
// ============================================================================

#[tokio::test]
async fn wildcard_catches_thrown_string() {
    let output = baml_test!(
        r#"
        function fails() -> int {
            throw "boom";
        }

        function main() -> int {
            fails() catch (e) {
                _ => -1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn wildcard_catches_thrown_int() {
    let output = baml_test!(
        r#"
        function fails() -> int {
            throw 42;
        }

        function main() -> int {
            fails() catch (e) {
                _ => -1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ============================================================================
// §2 — Single arm: typed binding catches matching throw
// ============================================================================

#[tokio::test]
async fn typed_binding_catches_string_throw() {
    let output = baml_test!(
        r#"
        function fails() -> int {
            throw "boom";
        }

        function main() -> int {
            fails() catch (e) {
                _: string => -1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn typed_binding_catches_int_throw() {
    let output = baml_test!(
        r#"
        function fails() -> int {
            throw 42;
        }

        function main() -> int {
            fails() catch (e) {
                _: int => -1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ============================================================================
// §3 — Multi-arm: typed bindings dispatch by throw type
// ============================================================================

#[tokio::test]
async fn two_typed_arms_string_fires() {
    // Callee can throw string or int, so both arms are reachable.
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
async fn two_typed_arms_int_fires() {
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
async fn typed_arm_plus_wildcard_typed_matches() {
    let output = baml_test!(
        r#"
        function fails(mode: int) -> int {
            if (mode == 0) { throw "boom" }
            throw 42
        }

        function main() -> int {
            fails(0) catch (e) {
                _: string => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn typed_arm_plus_wildcard_wildcard_matches() {
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
// §4 — Unhandled throws propagate with correct values
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
// §5 — Single arm: explicit panic pattern catches
// ============================================================================

#[tokio::test]
async fn catch_division_by_zero() {
    let output = baml_test!(
        r#"
        function divides() -> int { 1 / 0 }

        function main() -> int {
            divides() catch (e) {
                DivisionByZero => -1
            }
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
            oob() catch (e) {
                IndexOutOfBounds => -1
            }
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
            bad() catch (e) {
                MapKeyNotFound => -1
            }
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
            bad() catch (e) {
                IndexOutOfBounds => -1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ============================================================================
// §6 — Single arm: wildcard does NOT catch panics
// ============================================================================

#[tokio::test]
async fn wildcard_skips_division_by_zero() {
    // divides() has a throw path so _ is reachable. At runtime x=0
    // hits the panic path; _ does not catch panics.
    let output = baml_test!(
        r#"
        function divides(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            divides(0) catch (e) {
                _ => -1
            }
        }
    "#
    );
    assert!(output.result.is_err(), "expected panic to propagate past _");
}

#[tokio::test]
async fn wildcard_skips_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function oob(x: int) -> int {
            if (x > 100) { throw "too big" }
            let a = [1, 2, 3];
            a[x]
        }

        function main() -> int {
            oob(99) catch (e) {
                _ => -1
            }
        }
    "#
    );
    assert!(output.result.is_err(), "expected panic to propagate past _");
}

// ============================================================================
// §7 — Multi-arm: multiple panic patterns dispatch to correct arm
// ============================================================================

#[tokio::test]
async fn two_panics_first_arm_matches() {
    let output = baml_test!(
        r#"
        function divides() -> int { 1 / 0 }

        function main() -> int {
            divides() catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn two_panics_second_arm_matches() {
    let output = baml_test!(
        r#"
        function oob() -> int { let a = [1]; a[5] }

        function main() -> int {
            oob() catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// ============================================================================
// §8 — Multi-arm: panic pattern + wildcard (panics + user errors)
// ============================================================================

#[tokio::test]
async fn panic_arm_plus_wildcard_panic_fires() {
    // x=0 → DivisionByZero → explicit arm catches.
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
    // x=999 → throw "too big" → wildcard catches.
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
    // x=5 → no error → returns normal value, catch never fires.
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
// §9 — Three-arm: two panics + wildcard (full combinatoric)
//
// A single callee that can: succeed, throw a user error, panic with
// DivisionByZero, or panic with IndexOutOfBounds.
// ============================================================================

#[tokio::test]
async fn three_arms_division_by_zero_fires() {
    let output = baml_test!(
        r#"
        function panic_div() -> int { 1 / 0 }
        function panic_oob() -> int { let a = [1]; a[5] }
        function throw_str() -> int { throw "fail" }

        function risky(mode: int) -> int {
            if (mode == 0) { return panic_div() }
            if (mode == 1) { return panic_oob() }
            if (mode == 2) { return throw_str() }
            99
        }

        function main() -> int {
            risky(0) catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2,
                _ => 3
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn three_arms_index_out_of_bounds_fires() {
    let output = baml_test!(
        r#"
        function panic_div() -> int { 1 / 0 }
        function panic_oob() -> int { let a = [1]; a[5] }
        function throw_str() -> int { throw "fail" }

        function risky(mode: int) -> int {
            if (mode == 0) { return panic_div() }
            if (mode == 1) { return panic_oob() }
            if (mode == 2) { return throw_str() }
            99
        }

        function main() -> int {
            risky(1) catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2,
                _ => 3
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn three_arms_user_error_fires() {
    let output = baml_test!(
        r#"
        function panic_div() -> int { 1 / 0 }
        function panic_oob() -> int { let a = [1]; a[5] }
        function throw_str() -> int { throw "fail" }

        function risky(mode: int) -> int {
            if (mode == 0) { return panic_div() }
            if (mode == 1) { return panic_oob() }
            if (mode == 2) { return throw_str() }
            99
        }

        function main() -> int {
            risky(2) catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2,
                _ => 3
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn three_arms_no_error() {
    let output = baml_test!(
        r#"
        function panic_div() -> int { 1 / 0 }
        function panic_oob() -> int { let a = [1]; a[5] }
        function throw_str() -> int { throw "fail" }

        function risky(mode: int) -> int {
            if (mode == 0) { return panic_div() }
            if (mode == 1) { return panic_oob() }
            if (mode == 2) { return throw_str() }
            99
        }

        function main() -> int {
            risky(3) catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2,
                _ => 3
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

// ============================================================================
// §10 — Unmatched panic propagates past catch
// ============================================================================

#[tokio::test]
#[ignore = "bare type sugar: first arm matches unconditionally (no IsType discrimination)"]
async fn wrong_panic_pattern_propagates() {
    let output = baml_test!(
        r#"
        function divides() -> int { 1 / 0 }

        function main() -> int {
            divides() catch (e) {
                IndexOutOfBounds => -1
            }
        }
    "#
    );
    assert!(
        output.result.is_err(),
        "expected DivisionByZero to propagate past IndexOutOfBounds arm"
    );
}

#[tokio::test]
async fn uncaught_division_by_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { 1 / 0 }
    "#
    );
    assert!(output.result.is_err());
}

#[tokio::test]
async fn uncaught_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function main() -> int { let a = [1]; a[5] }
    "#
    );
    assert!(output.result.is_err());
}

// ============================================================================
// §11 — Panic type alias catches all panics
// ============================================================================

#[tokio::test]
#[ignore = "IsType emit doesn't handle union types — Panic alias expands to a union"]
async fn panic_alias_catches_division_by_zero() {
    let output = baml_test!(
        r#"
        function divides() -> int { 1 / 0 }

        function main() -> int {
            divides() catch (e) {
                Panic => -1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
#[ignore = "IsType emit doesn't handle union types — Panic alias expands to a union"]
async fn panic_alias_plus_wildcard_panic_fires() {
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

#[tokio::test]
#[ignore = "IsType emit doesn't handle union types — Panic alias expands to a union"]
async fn panic_alias_plus_wildcard_user_error_fires() {
    let output = baml_test!(
        r#"
        function risky(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            risky(999) catch (e) {
                Panic => 1,
                _ => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
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
            let x = divides() catch (e) {
                DivisionByZero => -1
            };
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
            inner() catch (e) {
                _ => throw "rethrown"
            }
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
            oob() catch (e) {
                IndexOutOfBounds => e.index
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}
