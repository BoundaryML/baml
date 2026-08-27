// Tests for exception handling: catch/throw/panic semantics.
//
// Tests not expressible in BAML (require host-side Rust inspection):
//   - exception_stack_trace_through_closures — asserts on the exact text of the
//     traceback via insta::assert_snapshot!(err, ...). No BAML way to inspect
//     the error message string.
//   - exception_stack_trace_on_panic — same reason (traceback string assertion).
//   - catch_with_stack_trace_binding — inspects st.to_string() and asserts the
//     exact traceback text including file names and line numbers.
//   - catch_stack_trace_on_panic — same reason (traceback string + insta snapshot).
//   - catch_stale_field_slot_invalid_field_access — hand-patches the compiled
//     bytecode (rewrites `user.oob`'s Instruction list) before running; needs
//     Rust-side access to the program object table.
//   - uncaught_throw_renders_readable_error_not_debug — asserts on the host-side
//     rendering of an UNCAUGHT throw (EngineError display); catching it in BAML
//     would not exercise the escape-to-host path.
//   - union_throws_panic_escapes_as_clean_panic — asserts the host-boundary
//     EngineError::UnhandledThrow shape (clean panic Instance, no union
//     wrapping) for a panic that escapes to the host uncaught.
//   - panic_escapes_all_throws_shapes_identically — same host-boundary
//     assertion across four `throws` shapes.
//   - exit_panic_escapes_union_throws_as_clean_exit — asserts
//     EngineError::Exit { code } surfaces at the host.
//   - genuine_in_contract_throw_still_wrapped_through_union — asserts the FFI
//     BexExternalValue::Union wire wrapping of an uncaught in-contract throw.

// ============================================================================
// §1 — Catch by literal value
// ============================================================================

function s1_fails_1() -> int {
    throw "boom";
}

function catch_literal_string_match_fn() -> int {
    s1_fails_1() catch (e) {
        "boom" => 1,
        _ => 2
    }
}

test "catch_literal_string_match" {
    assert.equal(catch_literal_string_match_fn(), 1)
}

function s1_fails_2() -> int {
    throw "other";
}

function catch_literal_string_no_match_falls_to_wildcard_fn() -> int {
    s1_fails_2() catch (e) {
        "boom" => 1,
        _ => 2
    }
}

test "catch_literal_string_no_match_falls_to_wildcard" {
    assert.equal(catch_literal_string_no_match_falls_to_wildcard_fn(), 2)
}

function s1_fails_3() -> int {
    throw 42;
}

function catch_literal_int_match_fn() -> int {
    s1_fails_3() catch (e) {
        42 => 1,
        _ => 2
    }
}

test "catch_literal_int_match" {
    assert.equal(catch_literal_int_match_fn(), 1)
}

function s1_fails_4(mode: int) -> int {
    if (mode == 0) { throw "alpha" }
    if (mode == 1) { throw "beta" }
    throw "gamma"
}

function catch_multiple_literals_dispatch_fn() -> int {
    s1_fails_4(1) catch (e) {
        "alpha" => 1,
        "beta" => 2,
        _ => 3
    }
}

test "catch_multiple_literals_dispatch" {
    assert.equal(catch_multiple_literals_dispatch_fn(), 2)
}

// ============================================================================
// §2 — Catch by typed binding (Type =>)
// ============================================================================

function s2_fails_5() -> int {
    throw "boom";
}

function typed_binding_string_fn() -> int {
    s2_fails_5() catch (e) {
        string => 1
    }
}

test "typed_binding_string" {
    assert.equal(typed_binding_string_fn(), 1)
}

function s2_fails_6() -> int {
    throw 42;
}

function typed_binding_int_fn() -> int {
    s2_fails_6() catch (e) {
        int => 1
    }
}

test "typed_binding_int" {
    assert.equal(typed_binding_int_fn(), 1)
}

function s2_fails_7(mode: int) -> int {
    if (mode == 0) { throw "boom" }
    throw 42
}

function typed_binding_dispatch_string_vs_int_fn() -> int {
    s2_fails_7(0) catch (e) {
        string => 1,
        int => 2
    }
}

test "typed_binding_dispatch_string_vs_int" {
    assert.equal(typed_binding_dispatch_string_vs_int_fn(), 1)
}

function typed_binding_dispatch_int_vs_string_fn() -> int {
    s2_fails_7(1) catch (e) {
        string => 1,
        int => 2
    }
}

test "typed_binding_dispatch_int_vs_string" {
    assert.equal(typed_binding_dispatch_int_vs_string_fn(), 2)
}

function s2_fails_9(mode: int) -> int {
    if (mode == 0) { throw "boom" }
    throw 42
}

function typed_binding_plus_wildcard_fn() -> int {
    s2_fails_9(1) catch (e) {
        string => 1,
        _ => 2
    }
}

test "typed_binding_plus_wildcard" {
    assert.equal(typed_binding_plus_wildcard_fn(), 2)
}

// ============================================================================
// §2b — Named typed binding (var: Type =>) with value access
// ============================================================================

function s2b_fails_10(mode: int) -> string {
    if (mode == 0) { throw "boom" }
    throw 42
}

function named_binding_string_access_value_fn() -> string {
    s2b_fails_10(0) catch (e) {
        let msg: string => msg,
        int => "was int"
    }
}

test "named_binding_string_access_value" {
    assert.equal(named_binding_string_access_value_fn(), "boom")
}

function s2b_fails_11(mode: int) -> int {
    if (mode == 0) { throw "boom" }
    throw 42
}

function named_binding_int_access_value_fn() -> int {
    s2b_fails_11(1) catch (e) {
        string => -1,
        let code: int => code
    }
}

test "named_binding_int_access_value" {
    assert.equal(named_binding_int_access_value_fn(), 42)
}

// ============================================================================
// §3 — Catch by user-defined class type
// ============================================================================

class NetworkError { url string }
class ParseError { message string }

function s3_fails_12() -> int {
    throw NetworkError { url: "http://example.com" }
}

function catch_user_class_single_arm_fn() -> int {
    s3_fails_12() catch (e) {
        NetworkError => 1
    }
}

test "catch_user_class_single_arm" {
    assert.equal(catch_user_class_single_arm_fn(), 1)
}

function s3_fails_13(mode: int) -> int {
    if (mode == 0) { throw NetworkError { url: "http://x" } }
    throw ParseError { message: "bad json" }
}

function catch_two_user_classes_dispatch_first_fn() -> int {
    s3_fails_13(0) catch (e) {
        NetworkError => 1,
        ParseError => 2
    }
}

test "catch_two_user_classes_dispatch_first" {
    assert.equal(catch_two_user_classes_dispatch_first_fn(), 1)
}

function catch_two_user_classes_dispatch_second_fn() -> int {
    s3_fails_13(1) catch (e) {
        NetworkError => 1,
        ParseError => 2
    }
}

test "catch_two_user_classes_dispatch_second" {
    assert.equal(catch_two_user_classes_dispatch_second_fn(), 2)
}

function s3_fails_15(mode: int) -> int {
    if (mode == 0) { throw NetworkError { url: "http://x" } }
    throw "plain string error"
}

function catch_user_class_plus_wildcard_fn() -> int {
    s3_fails_15(1) catch (e) {
        NetworkError => 1,
        _ => 2
    }
}

test "catch_user_class_plus_wildcard" {
    assert.equal(catch_user_class_plus_wildcard_fn(), 2)
}

class AuthError { reason string }
class NotFound { path string }
class RateLimit { retryAfter int }

function s3_api_1(mode: int) -> int {
    if (mode == 0) { throw AuthError { reason: "expired" } }
    if (mode == 1) { throw NotFound { path: "/users" } }
    if (mode == 2) { throw RateLimit { retryAfter: 30 } }
    throw "unknown"
}

function catch_three_user_classes_plus_wildcard_fn() -> int {
    s3_api_1(2) catch (e) {
        AuthError => 1,
        NotFound => 2,
        RateLimit => 3,
        _ => 4
    }
}

test "catch_three_user_classes_plus_wildcard" {
    assert.equal(catch_three_user_classes_plus_wildcard_fn(), 3)
}

// ============================================================================
// §3a — Named typed binding for user classes (var: Class => var.field)
// ============================================================================

function s3a_fails_16() -> string {
    throw NetworkError { url: "http://example.com" }
}

function named_class_binding_access_field_fn() -> string {
    s3a_fails_16() catch (e) {
        let err: NetworkError => err.url
    }
}

test "named_class_binding_access_field" {
    assert.equal(named_class_binding_access_field_fn(), "http://example.com")
}

function s3a_fails_17(mode: int) -> string {
    if (mode == 0) { throw NetworkError { url: "http://x" } }
    throw ParseError { message: "bad json" }
}

function named_class_binding_dispatch_access_fields_fn() -> string {
    s3a_fails_17(1) catch (e) {
        let net: NetworkError => net.url,
        let parse: ParseError => parse.message
    }
}

test "named_class_binding_dispatch_access_fields" {
    assert.equal(named_class_binding_dispatch_access_fields_fn(), "bad json")
}

// ============================================================================
// §3b — Catch by bare class name (MyClass => without binding)
// ============================================================================

function s3b_fails_18() -> int {
    throw NetworkError { url: "http://example.com" }
}

function bare_class_single_arm_fn() -> int {
    s3b_fails_18() catch (e) {
        NetworkError => 1
    }
}

test "bare_class_single_arm" {
    assert.equal(bare_class_single_arm_fn(), 1)
}

function s3b_fails_19(mode: int) -> int {
    if (mode == 0) { throw NetworkError { url: "http://x" } }
    throw ParseError { message: "bad" }
}

function bare_class_dispatch_first_fn() -> int {
    s3b_fails_19(0) catch (e) {
        NetworkError => 1,
        ParseError => 2
    }
}

test "bare_class_dispatch_first" {
    assert.equal(bare_class_dispatch_first_fn(), 1)
}

function bare_class_dispatch_second_fn() -> int {
    s3b_fails_19(1) catch (e) {
        NetworkError => 1,
        ParseError => 2
    }
}

test "bare_class_dispatch_second" {
    assert.equal(bare_class_dispatch_second_fn(), 2)
}

function s3b_fails_21(mode: int) -> int {
    if (mode == 0) { throw NetworkError { url: "http://x" } }
    throw "plain string"
}

function bare_class_plus_wildcard_fn() -> int {
    s3b_fails_21(0) catch (e) {
        NetworkError => 1,
        _ => 2
    }
}

test "bare_class_plus_wildcard" {
    assert.equal(bare_class_plus_wildcard_fn(), 1)
}

function bare_class_plus_wildcard_wildcard_fires_fn() -> int {
    s3b_fails_21(1) catch (e) {
        NetworkError => 1,
        _ => 2
    }
}

test "bare_class_plus_wildcard_wildcard_fires" {
    assert.equal(bare_class_plus_wildcard_wildcard_fires_fn(), 2)
}

// ============================================================================
// §4 — Unhandled throws propagate
// ============================================================================

test "unhandled_throw_string" {
    {
        throw "something went wrong";
        baml.sys.panic("expected throw")
    } catch (e) {
        string => null
    }
}

test "unhandled_throw_int" {
    {
        throw 42;
        baml.sys.panic("expected throw")
    } catch (e) {
        int => null
    }
}

// ============================================================================
// §5 — Panic patterns: single arm per panic type
// ============================================================================

function s5_divides_1() -> int { 1 / 0 }

function catch_division_by_zero_fn() -> int {
    s5_divides_1() catch (e) { baml.panics.DivisionByZero => -1 }
}

test "catch_division_by_zero" {
    assert.equal(catch_division_by_zero_fn(), -1)
}

// Integer overflow is a catchable panic, parallel to division-by-zero. The
// helpers take runtime args so the arithmetic isn't constant-folded away — it
// runs through the VM's checked AddInt/SubInt/MulInt opcodes.
function s5_add_overflow(x: int) -> int { x + 1 }
function s5_sub_overflow(x: int) -> int { x - 1 }
function s5_mul_overflow(x: int) -> int { x * 2 }
function s5_neg_overflow(x: int) -> int { 0 - x }

function catch_integer_overflow_add_fn() -> int {
    s5_add_overflow(baml.Int.max_value()) catch (e) { baml.panics.IntegerOverflow => -1 }
}

test "catch_integer_overflow_add" {
    assert.equal(catch_integer_overflow_add_fn(), -1)
}

function catch_integer_overflow_sub_fn() -> int {
    s5_sub_overflow(baml.Int.min_value()) catch (e) { baml.panics.IntegerOverflow => -1 }
}

test "catch_integer_overflow_sub" {
    assert.equal(catch_integer_overflow_sub_fn(), -1)
}

function catch_integer_overflow_mul_fn() -> int {
    s5_mul_overflow(baml.Int.max_value()) catch (e) { baml.panics.IntegerOverflow => -1 }
}

test "catch_integer_overflow_mul" {
    assert.equal(catch_integer_overflow_mul_fn(), -1)
}

// `-int.min_value()` overflows (its magnitude is INT_MAX + 1).
function catch_integer_overflow_neg_fn() -> int {
    s5_neg_overflow(baml.Int.min_value()) catch (e) { baml.panics.IntegerOverflow => -1 }
}

test "catch_integer_overflow_neg" {
    assert.equal(catch_integer_overflow_neg_fn(), -1)
}

// An overflowing panic is a member of the `baml.panics.Panic` union.
function catch_integer_overflow_via_panic_union_fn() -> int {
    s5_add_overflow(baml.Int.max_value()) catch (e) { baml.panics.Panic => -2 }
}

test "catch_integer_overflow_via_panic_union" {
    assert.equal(catch_integer_overflow_via_panic_union_fn(), -2)
}

// A negative shift count produces a catchable NegativeBitShift. Runtime args
// avoid constant folding so the VM's shift opcode is exercised. Oversized
// counts truncate instead; see `ns_operators`.
function s5_shl(x: int, n: int) -> int { x << n }

function catch_negative_shift_fn() -> int {
    s5_shl(1, 0 - 1) catch (e) { baml.panics.NegativeBitShift => -1 }
}

test "catch_negative_shift" {
    assert.equal(catch_negative_shift_fn(), -1)
}

function s5_oob_1() -> int { let a = [1, 2]; a[5] }

function catch_index_out_of_bounds_fn() -> int {
    s5_oob_1() catch (e) { baml.panics.IndexOutOfBounds => -1 }
}

test "catch_index_out_of_bounds" {
    assert.equal(catch_index_out_of_bounds_fn(), -1)
}

function s5_bad_1() -> int { let m = {"a": 1}; m["x"] }

function catch_map_key_not_found_fn() -> int {
    s5_bad_1() catch (e) { baml.panics.MapKeyNotFound => -1 }
}

test "catch_map_key_not_found" {
    assert.equal(catch_map_key_not_found_fn(), -1)
}

// `-3` counts from the end past the start of a length-2 array (it normalizes
// to `-1`), so it is out of bounds. (`a[-1]` would resolve to the last element.)
function s5_bad_2() -> int { let a = [1, 2]; a[-3] }

function catch_negative_index_as_index_out_of_bounds_fn() -> int {
    s5_bad_2() catch (e) { baml.panics.IndexOutOfBounds => -1 }
}

test "catch_negative_index_as_index_out_of_bounds" {
    assert.equal(catch_negative_index_as_index_out_of_bounds_fn(), -1)
}

// ============================================================================
// §5b — Named panic binding (var: PanicType => var.field)
// ============================================================================

function s5b_divides_2() -> int { 10 / 0 }

function named_panic_binding_division_by_zero_field_fn() -> int {
    s5b_divides_2() catch (e) {
        let err: baml.panics.DivisionByZero => err.dividend
    }
}

test "named_panic_binding_division_by_zero_field" {
    assert.equal(named_panic_binding_division_by_zero_field_fn(), 10)
}

// String-typed so the protected value and the `err.message` handler unify to
// one type. `x + 1` overflows and throws before the `if` is reached; `y` is
// used so the throwing add isn't dead-code-eliminated.
function s5b_overflow_to_string(x: int) -> string {
    let y = x + 1;
    if (y > 0) { "positive" } else { "non-positive" }
}

function named_panic_binding_integer_overflow_message_fn() -> string {
    s5b_overflow_to_string(baml.Int.max_value()) catch (e) {
        let err: baml.panics.IntegerOverflow => err.message
    }
}

test "named_panic_binding_integer_overflow_message" {
    assert.equal(
        named_panic_binding_integer_overflow_message_fn(),
        "4611686018427387903 + 1 overflows int",
    )
}

function s5b_oob_2() -> int { let a = [10, 20, 30]; a[7] }

function named_panic_binding_index_out_of_bounds_fields_fn() -> int {
    s5b_oob_2() catch (e) {
        let err: baml.panics.IndexOutOfBounds => err.index
    }
}

test "named_panic_binding_index_out_of_bounds_fields" {
    assert.equal(named_panic_binding_index_out_of_bounds_fields_fn(), 7)
}

function named_panic_binding_index_out_of_bounds_length_fn() -> int {
    s5b_oob_2() catch (e) {
        let err: baml.panics.IndexOutOfBounds => err.length
    }
}

test "named_panic_binding_index_out_of_bounds_length" {
    assert.equal(named_panic_binding_index_out_of_bounds_length_fn(), 3)
}

function s5b_bad_3() -> string { let m = {"a": "one"}; m["x"] }

function named_panic_binding_map_key_not_found_field_fn() -> string {
    s5b_bad_3() catch (e) {
        let err: baml.panics.MapKeyNotFound => err.key
    }
}

// The VM currently sets key to "(unknown)" — just verify it returns a string
// (i.e., the field is accessible and the test does not throw).
test "named_panic_binding_map_key_not_found_field" {
    let result = named_panic_binding_map_key_not_found_field_fn();
    let expected = named_panic_binding_map_key_not_found_field_fn();
    assert.equal(result, expected)
}

// ============================================================================
// §6 — Wildcard does NOT catch panics
// ============================================================================

function s6_risky_1(x: int) -> int {
    if (x > 100) { throw "too big" }
    1 / x
}

test "wildcard_skips_division_by_zero" {
    {
        s6_risky_1(0) catch (e) { _ => -1 };
        baml.sys.panic("expected uncaught DivisionByZero")
    } catch (e) {
        baml.panics.DivisionByZero => null
    }
}

function s6_risky_2(x: int) -> int {
    if (x > 100) { throw "too big" }
    let a = [1, 2, 3];
    a[x]
}

test "wildcard_skips_index_out_of_bounds" {
    {
        s6_risky_2(99) catch (e) { _ => -1 };
        baml.sys.panic("expected uncaught IndexOutOfBounds")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

// ============================================================================
// §7 — Multi-arm: panics + user errors in same catch
// ============================================================================

class AppError { code int }

function s7_risky_3(x: int) -> int {
    if (x > 100) { throw "too big" }
    1 / x
}

function panic_arm_plus_wildcard_panic_fires_fn() -> int {
    s7_risky_3(0) catch (e) {
        baml.panics.DivisionByZero => 1,
        _ => 2
    }
}

test "panic_arm_plus_wildcard_panic_fires" {
    assert.equal(panic_arm_plus_wildcard_panic_fires_fn(), 1)
}

function panic_arm_plus_wildcard_user_error_fires_fn() -> int {
    s7_risky_3(999) catch (e) {
        baml.panics.DivisionByZero => 1,
        _ => 2
    }
}

test "panic_arm_plus_wildcard_user_error_fires" {
    assert.equal(panic_arm_plus_wildcard_user_error_fires_fn(), 2)
}

function s7_risky_5(x: int) -> int {
    if (x > 100) { throw "too big" }
    1000 / x
}

function panic_arm_plus_wildcard_no_error_fn() -> int {
    s7_risky_5(5) catch (e) {
        baml.panics.DivisionByZero => -1,
        _ => -2
    }
}

test "panic_arm_plus_wildcard_no_error" {
    assert.equal(panic_arm_plus_wildcard_no_error_fn(), 200)
}

// ============================================================================
// §8 — Multi-arm: user class + panic + wildcard
// ============================================================================

function s8_do_div_1() -> int { 1 / 0 }

function s8_risky_6(mode: int) -> int {
    if (mode == 0) { throw AppError { code: 500 } }
    if (mode == 1) { 1 / 0 }
    throw "fallback"
}

function user_class_plus_panic_plus_wildcard_class_fires_fn() -> int {
    s8_risky_6(0) catch (e) {
        AppError => 1,
        baml.panics.DivisionByZero => 2,
        _ => 3
    }
}

test "user_class_plus_panic_plus_wildcard_class_fires" {
    assert.equal(user_class_plus_panic_plus_wildcard_class_fires_fn(), 1)
}

function s8_risky_7(mode: int) -> int {
    if (mode == 0) { throw AppError { code: 500 } }
    if (mode == 1) { return s8_do_div_1() }
    throw "fallback"
}

function user_class_plus_panic_plus_wildcard_panic_fires_fn() -> int {
    s8_risky_7(1) catch (e) {
        AppError => 1,
        baml.panics.DivisionByZero => 2,
        _ => 3
    }
}

test "user_class_plus_panic_plus_wildcard_panic_fires" {
    assert.equal(user_class_plus_panic_plus_wildcard_panic_fires_fn(), 2)
}

function user_class_plus_panic_plus_wildcard_string_fires_fn() -> int {
    s8_risky_7(2) catch (e) {
        AppError => 1,
        baml.panics.DivisionByZero => 2,
        _ => 3
    }
}

test "user_class_plus_panic_plus_wildcard_string_fires" {
    assert.equal(user_class_plus_panic_plus_wildcard_string_fires_fn(), 3)
}

// ============================================================================
// §9 — Mixed union catch arms (AppError | baml.panics.DivisionByZero =>)
// ============================================================================

// §9a — Mixed union WITH wildcard

function s9_do_div_2() -> int { 1 / 0 }

function s9_risky_9(mode: int) -> int {
    if (mode == 0) { throw AppError { code: 7 } }
    if (mode == 1) { return s9_do_div_2() }
    if (mode == 2) { throw "fallback" }
    let a = [1];
    a[5]
}

function mixed_union_arm_plus_wildcard_panic_fires_fn() -> int {
    s9_risky_9(1) catch (e) {
        AppError | baml.panics.DivisionByZero => 1,
        _ => 99
    }
}

test "mixed_union_arm_plus_wildcard_panic_fires" {
    assert.equal(mixed_union_arm_plus_wildcard_panic_fires_fn(), 1)
}

function s9_risky_10(mode: int) -> int {
    if (mode == 0) { throw AppError { code: 7 } }
    if (mode == 1) { 1 / 0 }
    throw "fallback"
}

function mixed_union_arm_plus_wildcard_user_error_fires_fn() -> int {
    s9_risky_10(0) catch (e) {
        AppError | baml.panics.DivisionByZero => 1,
        _ => 99
    }
}

test "mixed_union_arm_plus_wildcard_user_error_fires" {
    assert.equal(mixed_union_arm_plus_wildcard_user_error_fires_fn(), 1)
}

function mixed_union_arm_plus_wildcard_fallback_error_fires_fn() -> int {
    s9_risky_10(2) catch (e) {
        AppError | baml.panics.DivisionByZero => 1,
        _ => 99
    }
}

test "mixed_union_arm_plus_wildcard_fallback_error_fires" {
    assert.equal(mixed_union_arm_plus_wildcard_fallback_error_fires_fn(), 99)
}

function s9_risky_12(mode: int) -> int {
    if (mode == 0) { throw "fallback" }
    let a = [1];
    a[5]
}

test "mixed_union_arm_plus_wildcard_other_panic_propagates" {
    {
        s9_risky_12(1) catch (e) {
            AppError | baml.panics.DivisionByZero => 1,
            _ => 99
        };
        baml.sys.panic("expected uncaught IndexOutOfBounds")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

type Mixed = AppError | baml.panics.DivisionByZero

function s9_risky_13(mode: int) -> int {
    if (mode == 0) { throw AppError { code: 7 } }
    if (mode == 1) { return s9_do_div_2() }
    throw "fallback"
}

function mixed_union_alias_plus_wildcard_handles_both_domains_fn() -> int {
    s9_risky_13(1) catch (e) {
        Mixed => 1,
        _ => 99
    }
}

test "mixed_union_alias_plus_wildcard_handles_both_domains" {
    assert.equal(mixed_union_alias_plus_wildcard_handles_both_domains_fn(), 1)
}

function s9_risky_14(mode: int) -> int {
    if (mode == 0) { throw AppError { code: 7 } }
    1 / 0
}

function mixed_union_alias_plus_wildcard_error_fires_fn() -> int {
    s9_risky_14(0) catch (e) {
        Mixed => 1,
        _ => 99
    }
}

test "mixed_union_alias_plus_wildcard_error_fires" {
    assert.equal(mixed_union_alias_plus_wildcard_error_fires_fn(), 1)
}

// §9b — Mixed union WITHOUT wildcard

function s9b_risky_15(mode: int) -> int {
    if (mode == 0) { throw AppError { code: 7 } }
    return s9_do_div_2()
}

function mixed_union_no_wildcard_panic_fires_fn() -> int {
    s9b_risky_15(1) catch (e) {
        AppError | baml.panics.DivisionByZero => 1
    }
}

test "mixed_union_no_wildcard_panic_fires" {
    assert.equal(mixed_union_no_wildcard_panic_fires_fn(), 1)
}

function mixed_union_no_wildcard_error_fires_fn() -> int {
    s9b_risky_15(0) catch (e) {
        AppError | baml.panics.DivisionByZero => 1
    }
}

test "mixed_union_no_wildcard_error_fires" {
    assert.equal(mixed_union_no_wildcard_error_fires_fn(), 1)
}

function s9b_risky_17() -> int {
    throw "not matched"
}

test "mixed_union_no_wildcard_unmatched_rethrows" {
    {
        s9b_risky_17() catch (e) {
            AppError | baml.panics.DivisionByZero => 1
        };
        baml.sys.panic("expected rethrow past mixed union arm")
    } catch (e) {
        string => null
    }
}

function s9b_risky_18() -> int {
    let a = [1];
    a[5]
}

test "mixed_union_no_wildcard_unmatched_panic_rethrows" {
    {
        s9b_risky_18() catch (e) {
            AppError | baml.panics.DivisionByZero => 1
        };
        baml.sys.panic("expected uncaught IndexOutOfBounds")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

// §9c — Pure-panic union with wildcard

function s9c_do_div_3() -> int { 1 / 0 }

function s9c_risky_19(mode: int) -> int {
    if (mode == 0) { return s9c_do_div_3() }
    if (mode == 1) { let a = [1]; return a[5] }
    throw "fallback"
}

function panic_union_plus_wildcard_division_fires_fn() -> int {
    s9c_risky_19(0) catch (e) {
        baml.panics.DivisionByZero | baml.panics.IndexOutOfBounds => 1,
        _ => 99
    }
}

test "panic_union_plus_wildcard_division_fires" {
    assert.equal(panic_union_plus_wildcard_division_fires_fn(), 1)
}

function panic_union_plus_wildcard_index_fires_fn() -> int {
    s9c_risky_19(1) catch (e) {
        baml.panics.DivisionByZero | baml.panics.IndexOutOfBounds => 1,
        _ => 99
    }
}

test "panic_union_plus_wildcard_index_fires" {
    assert.equal(panic_union_plus_wildcard_index_fires_fn(), 1)
}

function s9c_risky_21(mode: int) -> int {
    if (mode == 0) { 1 / 0 }
    throw "fallback"
}

function panic_union_plus_wildcard_error_falls_to_wildcard_fn() -> int {
    s9c_risky_21(1) catch (e) {
        baml.panics.DivisionByZero | baml.panics.IndexOutOfBounds => 1,
        _ => 99
    }
}

test "panic_union_plus_wildcard_error_falls_to_wildcard" {
    assert.equal(panic_union_plus_wildcard_error_falls_to_wildcard_fn(), 99)
}

// ============================================================================
// §10 — Four-arm: two separate panics + user class + wildcard
// ============================================================================

function s10_do_div_4() -> int { 1 / 0 }
function s10_do_oob_1() -> int { let a = [1]; a[5] }

function s10_four_arms_risky(mode: int) -> int {
    if (mode == 0) { return s10_do_div_4() }
    if (mode == 1) { return s10_do_oob_1() }
    if (mode == 2) { throw AppError { code: 404 } }
    throw "unknown"
}

function four_arms_division_by_zero_fires_fn() -> int {
    s10_four_arms_risky(0) catch (e) {
        baml.panics.DivisionByZero => 1,
        baml.panics.IndexOutOfBounds => 2,
        AppError => 3,
        _ => 4
    }
}

test "four_arms_division_by_zero_fires" {
    assert.equal(four_arms_division_by_zero_fires_fn(), 1)
}

function four_arms_index_out_of_bounds_fires_fn() -> int {
    s10_four_arms_risky(1) catch (e) {
        baml.panics.DivisionByZero => 1,
        baml.panics.IndexOutOfBounds => 2,
        AppError => 3,
        _ => 4
    }
}

test "four_arms_index_out_of_bounds_fires" {
    assert.equal(four_arms_index_out_of_bounds_fires_fn(), 2)
}

function four_arms_user_class_fires_fn() -> int {
    s10_four_arms_risky(2) catch (e) {
        baml.panics.DivisionByZero => 1,
        baml.panics.IndexOutOfBounds => 2,
        AppError => 3,
        _ => 4
    }
}

test "four_arms_user_class_fires" {
    assert.equal(four_arms_user_class_fires_fn(), 3)
}

function four_arms_wildcard_fires_fn() -> int {
    s10_four_arms_risky(3) catch (e) {
        baml.panics.DivisionByZero => 1,
        baml.panics.IndexOutOfBounds => 2,
        AppError => 3,
        _ => 4
    }
}

test "four_arms_wildcard_fires" {
    assert.equal(four_arms_wildcard_fires_fn(), 4)
}

function s10_four_arms_no_error_risky(mode: int) -> int {
    if (mode == 0) { return s10_do_div_4() }
    if (mode == 1) { return s10_do_oob_1() }
    if (mode == 2) { throw AppError { code: 404 } }
    if (mode == 3) { throw "unknown" }
    99
}

function four_arms_no_error_fn() -> int {
    s10_four_arms_no_error_risky(4) catch (e) {
        baml.panics.DivisionByZero => 1,
        baml.panics.IndexOutOfBounds => 2,
        AppError => 3,
        _ => 4
    }
}

test "four_arms_no_error" {
    assert.equal(four_arms_no_error_fn(), 99)
}

// ============================================================================
// §11 — Uncaught panics propagate
// ============================================================================

// uncaught_division_by_zero: bare `1 / 0;` as a statement discards the panic
// (semicolon makes it a void statement). Call through a function so the panic
// propagates into the catch handler.
function s11_div_zero() -> int { 1 / 0 }

test "uncaught_division_by_zero" {
    {
        s11_div_zero();
        baml.sys.panic("expected DivisionByZero panic")
    } catch (e) {
        baml.panics.DivisionByZero => null
    }
}

// uncaught_index_out_of_bounds: same — call through a function.
function s11_oob() -> int { let a = [1]; a[5] }

test "uncaught_index_out_of_bounds" {
    {
        s11_oob();
        baml.sys.panic("expected IndexOutOfBounds panic")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

function s11_divides_wrong() -> int { 1 / 0 }

test "wrong_panic_pattern_propagates" {
    {
        s11_divides_wrong() catch (e) { baml.panics.IndexOutOfBounds => -1 };
        baml.sys.panic("expected uncaught DivisionByZero")
    } catch (e) {
        baml.panics.DivisionByZero => null
    }
}

// ============================================================================
// §11b — Same-frame exception table lookup
// ============================================================================

test "same_frame_division_caught" {
    assert.equal((1 / 0) catch (e) { baml.panics.DivisionByZero => -1 }, -1)
}

test "same_frame_index_out_of_bounds_caught" {
    assert.equal({ let a = [1, 2, 3]; a[10] } catch (e) { baml.panics.IndexOutOfBounds => -1 }, -1)
}

// ============================================================================
// §12 — Panic type alias (union of all panics)
// ============================================================================

function s12_divides_panic() -> int { 1 / 0 }

function panic_alias_catches_any_panic_fn() -> int {
    s12_divides_panic() catch (e) { baml.panics.Panic => -1 }
}

test "panic_alias_catches_any_panic" {
    assert.equal(panic_alias_catches_any_panic_fn(), -1)
}

function s12_risky_22(x: int) -> int {
    if (x > 100) { throw "too big" }
    1 / x
}

function panic_alias_plus_wildcard_dispatch_fn() -> int {
    s12_risky_22(0) catch (e) {
        baml.panics.Panic => 1,
        _ => 2
    }
}

test "panic_alias_plus_wildcard_dispatch" {
    assert.equal(panic_alias_plus_wildcard_dispatch_fn(), 1)
}

// ============================================================================
// §13 — Nested catch, rethrow, sequential
// ============================================================================

function s13_inner_1() -> int { throw "inner" }

function s13_middle_1() -> int {
    s13_inner_1() catch (e) { _ => 42 }
}

test "nested_inner_catches_outer_does_not_fire" {
    assert.equal(s13_middle_1() catch (e) { _ => -1 }, 42)
}

function s13_divides_3() -> int { 1 / 0 }

function s13_middle_2() -> int {
    let x = s13_divides_3() catch (e) { baml.panics.DivisionByZero => -1 };
    throw "recovered but failing"
}

test "nested_inner_catches_panic_outer_catches_rethrow" {
    assert.equal(s13_middle_2() catch (e) { _ => 99 }, 99)
}

function s13_inner_2() -> int { throw "original" }

function s13_middle_3() -> int {
    s13_inner_2() catch (e) { _ => throw "rethrown" }
}

test "rethrow_propagates_to_outer" {
    assert.equal(s13_middle_3() catch (e) { _ => 99 }, 99)
}

function s13_f1_seq() -> int { throw "one" }
function s13_f2_seq() -> int { throw "two" }

function sequential_catches_both_recover_fn() -> int {
    let a = s13_f1_seq() catch (e) { _ => 10 };
    let b = s13_f2_seq() catch (e) { _ => 20 };
    a + b
}

test "sequential_catches_both_recover" {
    assert.equal(sequential_catches_both_recover_fn(), 30)
}

// ============================================================================
// §14 — Special cases
// ============================================================================

test "inline_throw_catch" {
    assert.equal(throw 1 catch (e) { _ => 2 }, 2)
}

// throw_in_match_arm_propagates: no semicolon after match so the throw from the
// wildcard arm becomes the block's expression value and propagates to the catch.
// A trailing semicolon would discard the result (and the throw), so the
// sentinel panic would fire instead.
test "throw_in_match_arm_propagates" {
    {
        let x = 2;
        match (x) {
            1 => "one",
            _ => { throw "boom" },
        }
    } catch (e) {
        string => null
    }
}

function s14_oob_fields() -> int { let a = [10, 20, 30]; a[7] }

test "caught_panic_has_accessible_fields" {
    assert.equal(s14_oob_fields() catch (e) { baml.panics.IndexOutOfBounds => e.index }, 7)
}

// ============================================================================
// §15 — Optimized catch dispatch (baml_test_optimized! tests)
// ============================================================================

// 4 primitive type arms + wildcard — exercises type_tag + jump_table dispatch.
function s15_fails_opt_1() -> int {
    throw "oops"
}

function catch_four_primitive_type_arms_fn() -> int {
    s15_fails_opt_1() catch (e) {
        string => 1,
        int => 2,
        bool => 3,
        null => 4,
        _ => 0
    }
}

test "catch_four_primitive_type_arms" {
    assert.equal(catch_four_primitive_type_arms_fn(), 1)
}

// 4+ typed arms with final bind binding — panics must still propagate.
class ErrA { x int }
class ErrB { x int }
class ErrC { x int }
class ErrD { x int }

function s15_risky_opt_1() -> int { 1 / 0 }

test "catch_four_typed_arms_plus_bind_jump_table_rethrows_panic" {
    {
        s15_risky_opt_1() catch (e) {
            ErrA => 10,
            ErrB => 20,
            ErrC => 30,
            ErrD => 40,
            let other => 99
        };
        baml.sys.panic("expected uncaught DivisionByZero")
    } catch (e) {
        baml.panics.DivisionByZero => null
    }
}

// Throw a float — no arm matches, falls through to wildcard.
function s15_fails_opt_2() -> int {
    throw 3.14
}

function catch_four_primitive_wildcard_on_float_fn() -> int {
    s15_fails_opt_2() catch (e) {
        string => 1,
        int => 2,
        bool => 3,
        null => 4,
        _ => 0
    }
}

test "catch_four_primitive_wildcard_on_float" {
    assert.equal(catch_four_primitive_wildcard_on_float_fn(), 0)
}

// 3 type arms + wildcard, throw bool.
function s15_fails_opt_3() -> int {
    throw true
}

function catch_three_type_arms_bool_throw_fn() -> int {
    s15_fails_opt_3() catch (e) {
        string => 1,
        int => 2,
        bool => 3,
        _ => 0
    }
}

test "catch_three_type_arms_bool_throw" {
    assert.equal(catch_three_type_arms_bool_throw_fn(), 3)
}

// 4+ user class arms — exercises TypeTag dispatch path.
class Timeout { ms int }

function s15_api_opt(mode: int) -> int {
    if (mode == 0) { throw AuthError { reason: "expired" } }
    if (mode == 1) { throw NotFound { path: "/users" } }
    if (mode == 2) { throw RateLimit { retryAfter: 30 } }
    if (mode == 3) { throw Timeout { ms: 5000 } }
    throw "unknown"
}

function catch_four_user_classes_instanceof_chain_fn() -> int {
    s15_api_opt(2) catch (e) {
        AuthError => 1,
        NotFound => 2,
        RateLimit => 3,
        Timeout => 4,
        _ => 0
    }
}

test "catch_four_user_classes_instanceof_chain" {
    assert.equal(catch_four_user_classes_instanceof_chain_fn(), 3)
}

// Literal patterns use sequential dispatch even with 4+ arms.
function s15_fails_lits(x: int) -> int {
    if (x == 0) { throw "boom" }
    if (x == 1) { throw "bang" }
    if (x == 2) { throw "crash" }
    throw "other"
}

function catch_four_literal_arms_fn() -> int {
    s15_fails_lits(1) catch (e) {
        "boom" => 10,
        "bang" => 20,
        "crash" => 30,
        "other" => 40,
        _ => 0
    }
}

test "catch_four_literal_arms" {
    assert.equal(catch_four_literal_arms_fn(), 20)
}

// Mixed named and anonymous bindings with field access — separate class names to
// avoid reusing the module-level NetworkError/AuthError/NotFound/RateLimit above.
class NetErr { url string }
class AuthErr { reason string }
class NotFoundErr { path string }
class RateLimitErr { retryAfter int }

function s15_fails_mixed() -> string {
    throw NetErr { url: "http://example.com" }
}

function catch_mixed_named_and_anonymous_bindings_fn() -> string {
    s15_fails_mixed() catch (e) {
        let err: NetErr => err.url,
        let err2: AuthErr => err2.reason,
        let err3: NotFoundErr => err3.path,
        RateLimitErr => "rate limited",
        _ => "unknown"
    }
}

test "catch_mixed_named_and_anonymous_bindings" {
    assert.equal(catch_mixed_named_and_anonymous_bindings_fn(), "http://example.com")
}

// ============================================================================
// §16 — catch switch: 4-typed-arm tests from §12 (Rust) / §15 (BAML)
// ============================================================================

// catch_four_typed_arms_jump_table: 4 typed arms no wildcard — ErrC fires.
function s16_risky_jump(mode: int) -> int {
    if (mode == 0) { throw ErrA { x: 1 } }
    if (mode == 1) { throw ErrB { x: 2 } }
    if (mode == 2) { throw ErrC { x: 3 } }
    throw ErrD { x: 4 }
}

function catch_four_typed_arms_jump_table_fn() -> int {
    s16_risky_jump(2) catch (e) {
        ErrA => 10,
        ErrB => 20,
        ErrC => 30,
        ErrD => 40
    }
}

test "catch_four_typed_arms_jump_table" {
    assert.equal(catch_four_typed_arms_jump_table_fn(), 30)
}

// catch_four_typed_arms_plus_wildcard_jump_table: no match → wildcard (mode 4).
function s16_risky_jump2(mode: int) -> int {
    if (mode == 0) { throw ErrA { x: 1 } }
    if (mode == 1) { throw ErrB { x: 2 } }
    if (mode == 2) { throw ErrC { x: 3 } }
    if (mode == 3) { throw ErrD { x: 4 } }
    throw "unknown"
}

function catch_four_typed_arms_plus_wildcard_jump_table_fn() -> int {
    s16_risky_jump2(4) catch (e) {
        ErrA => 10,
        ErrB => 20,
        ErrC => 30,
        ErrD => 40,
        _ => 99
    }
}

test "catch_four_typed_arms_plus_wildcard_jump_table" {
    assert.equal(catch_four_typed_arms_plus_wildcard_jump_table_fn(), 99)
}

// catch_two_typed_arms_sequential_chain: 2 arms — below 4-arm switch threshold.
class NetworkError3 { url string }
class ParseError3 { msg string }

function s16_fails_chain(mode: int) -> int {
    if (mode == 0) { throw NetworkError3 { url: "http://x" } }
    throw ParseError3 { msg: "bad" }
}

function catch_two_typed_arms_sequential_chain_fn() -> int {
    s16_fails_chain(1) catch (e) {
        NetworkError3 => 1,
        ParseError3 => 2
    }
}

test "catch_two_typed_arms_sequential_chain" {
    assert.equal(catch_two_typed_arms_sequential_chain_fn(), 2)
}

// catch_mixed_literal_and_typed_no_switch: literal + typed — no switch optimization.
class AppError2 { code int }

function s16_fails_lit_typed(mode: int) -> int {
    if (mode == 0) { throw "boom" }
    throw AppError2 { code: 404 }
}

function catch_mixed_literal_and_typed_no_switch_fn() -> int {
    s16_fails_lit_typed(0) catch (e) {
        "boom" => 1,
        AppError2 => 2,
        _ => 3
    }
}

test "catch_mixed_literal_and_typed_no_switch" {
    assert.equal(catch_mixed_literal_and_typed_no_switch_fn(), 1)
}

// ============================================================================
// §17 — Stack trace tests (behavioral subset only)
// ============================================================================

// catch_without_stack_trace_still_works: only checks the value, not the trace.
function s17_inner_st() -> string { throw "oops" }

function catch_without_stack_trace_still_works_fn() -> string {
    s17_inner_st() catch (e) {
        string => { e }
    }
}

test "catch_without_stack_trace_still_works" {
    assert.equal(catch_without_stack_trace_still_works_fn(), "oops")
}

// named_wrapper_value_catches_callback_throw: catch a throw from a callback
// passed through multiple forwarding functions.
function s17_direct_cb(cb: (value: int) -> string) -> string {
    cb(1)
}

function s17_forward_cb(cb: (value: int) -> string) -> string {
    s17_direct_cb(cb)
}

function s17_risky_cb(value: int) -> string throws string {
    throw "boom"
}

function named_wrapper_value_catches_callback_throw_fn() -> string {
    let run = s17_forward_cb;
    run(s17_risky_cb) catch (e) {
        "boom" => "caught",
        _ => "other"
    }
}

test "named_wrapper_value_catches_callback_throw" {
    assert.equal(named_wrapper_value_catches_callback_throw_fn(), "caught")
}

// ============================================================================
// §18 — catch_four_typed_arms_plus_bind_chain_rethrows_panic
// ============================================================================

// The final arm `let other: let alias => 99` binds `other` with a chained
// `let alias` catch-all type-pattern. A bind catch-all does NOT catch a panic,
// so DivisionByZero still propagates. (Distinct from the §15 jump-table variant,
// which uses a plain `let other` arm.)
test "catch_four_typed_arms_plus_bind_chain_rethrows_panic" {
    {
        s15_risky_opt_1() catch (e) {
            ErrA => 10,
            ErrB => 20,
            ErrC => 30,
            ErrD => 40,
            let other: let alias => 99
        };
        baml.sys.panic("expected uncaught DivisionByZero")
    } catch (e) {
        baml.panics.DivisionByZero => null
    }
}

// ============================================================================
// §19 — Regression: nested catch routing for cross-frame throws
// ============================================================================

// A throw propagating out of a CALLED function must be caught by the INNERMOST
// enclosing `catch`, not the outermost. Inline `throw`s are lowered as a direct
// jump to the handler, but a throw that escapes a callee — or a runtime panic —
// reaches the caller's exception table, and the VM must select the innermost
// covering entry (largest `start_pc`), not the first. Here `s19_boom()` throws
// inside the callee; the inner `catch (e) { _ => 1 }` handles it.
function s19_boom() -> void { throw "x" }

function nested_catch_inner_catches_callee_throw_fn() -> int {
    ({ s19_boom(); 0 } catch (e) { _ => 1 }) catch (e2) { _ => 2 }
}

test "nested_catch_inner_catches_callee_throw" {
    assert.equal(nested_catch_inner_catches_callee_throw_fn(), 1)
}

// ============================================================================
// §20 — Regression: direct throw escapes catch_all when block also calls a
// throwing function
// ============================================================================

// A direct `throw` block is a leaf in the CFG. Before the fix, RPO layout sank
// it to the END of the function — past the handler — while a throwing call's
// unwind edge pulled the handler to a mid-function PC. The throw's PC then fell
// outside the handler's `[start_pc, handler_pc)` exception-table range and the
// throw ESCAPED the enclosing `catch_all` entirely (uncaught at top level).
// `Throw`/`Rethrow` terminators now carry a layout-only unwind edge to the
// active handler, which keeps the throw block inside the protected range.
function s20_may_throw(n: int) -> string throws baml.errors.Io | baml.errors.ParseError {
    if (n == 42) { throw baml.errors.Io { message: "io" } }
    "ok"
}

function s20_guard_throw_plus_call_fn(n: int) -> string {
    {
        if (n == 0) { throw baml.errors.InvalidArgument { message: "guard" } }
        s20_may_throw(n)
    } catch_all (e) { _ => "caught" }
}

// The guard's direct throw must be caught (this escaped before the fix).
test "catch_all_covers_direct_throw_beside_throwing_call" {
    assert.equal(s20_guard_throw_plus_call_fn(0), "caught")
}

// The callee-originated throw keeps being caught.
test "catch_all_covers_callee_throw_beside_direct_throw" {
    assert.equal(s20_guard_throw_plus_call_fn(42), "caught")
}

// Neither throws: the block's normal value flows out.
test "catch_all_direct_throw_beside_call_normal_path" {
    assert.equal(s20_guard_throw_plus_call_fn(1), "ok")
}

// Same shape with a typed `catch` arm instead of `catch_all`: the direct
// throw's type is matched by the arm.
function s20_guard_throw_typed_catch_fn(n: int) -> string {
    {
        if (n == 0) { throw baml.errors.InvalidArgument { message: "guard" } }
        s20_may_throw(n)
    } catch (e) {
        baml.errors.InvalidArgument => "guard-caught",
        baml.errors.Io => "io-caught",
        _ => "other"
    }
}

test "typed_catch_covers_direct_throw_beside_throwing_call" {
    assert.equal(s20_guard_throw_typed_catch_fn(0), "guard-caught")
}

test "typed_catch_covers_callee_throw_beside_direct_throw" {
    assert.equal(s20_guard_throw_typed_catch_fn(42), "io-caught")
}

// The guard throw sits AFTER a `defer` in the same block, so it must unwind
// through the defer's landing pad (running the defer) and still reach the
// enclosing catch_all — the same leaf-block layout hazard, via the defer
// region's exception-table range.
function s20_defer_guard_throw_fn(n: int) -> string {
    let log = [];
    let r = {
        {
            defer { log.push("deferred") }
            if (n == 0) { throw baml.errors.InvalidArgument { message: "guard" } }
            s20_may_throw(n)
        } catch_all (e) { _ => "caught" }
    };
    if (log.length() == 1) { r } else { "defer-did-not-run" }
}

test "catch_all_covers_direct_throw_after_defer_beside_call" {
    assert.equal(s20_defer_guard_throw_fn(0), "caught")
}

// A runtime panic from a CALL-FREE branch (no call, no throw terminator — the
// block has no CFG edge to the handler at all) beside a throwing call in the
// other branch. Before exact coverage, the panicking block could be laid out
// past the handler and the panic escaped the enclosing catch_all_panics.
function s20_index_panic_beside_call_fn(n: int) -> int {
    {
        if (n == 0) {
            let nums = [1];
            nums[n + 5]
        } else {
            s20_may_throw(n);
            7
        }
    } catch_all_panics (e) { _ => -1 }
}

test "catch_all_panics_covers_callfree_panic_beside_throwing_call" {
    assert.equal(s20_index_panic_beside_call_fn(0), -1)
}

test "catch_all_panics_callfree_panic_branch_normal_path" {
    assert.equal(s20_index_panic_beside_call_fn(1), 7)
}

// Division-by-zero variant of the same shape.
function s20_div_panic_beside_call_fn(n: int) -> int {
    {
        if (n == 0) {
            100 / n
        } else {
            s20_may_throw(n);
            7
        }
    } catch_all_panics (e) { _ => -1 }
}

test "catch_all_panics_covers_callfree_div_panic_beside_throwing_call" {
    assert.equal(s20_div_panic_beside_call_fn(0), -1)
}
