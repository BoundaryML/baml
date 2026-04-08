//! Catch dispatch optimization tests: jump tables, binary search, if-else chains
//! for catch arms dispatching on type tags.
//!
//! Mirrors `match_optimization.rs` — same strategy selection algorithm applies:
//! - 4+ dense type-tag arms → jump table
//! - 4+ sparse arms → binary search
//! - <4 arms → if-else chain
//!
//! All tests use `baml_test_optimized!` which compiles at `OptLevel::Two`,
//! activating the catch-to-switch lowering path.

use baml_tests::baml_test_optimized;
use bex_engine::BexExternalValue;

// ============================================================================
// Jump Table Tests (4+ dense type-tag arms)
// ============================================================================

#[tokio::test]
async fn catch_jump_table_four_primitive_types() {
    // 4 primitive type arms: int(0), string(1), bool(2), null(3) — dense range,
    // triggers jump table dispatch.
    let output = baml_test_optimized!(
        r#"
        function fails() -> int {
            throw "oops"
        }

        function main() -> int {
            fails() catch (e) {
                _: string => 1,
                _: int => 2,
                _: bool => 3,
                _: null => 4,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        load_const "oops"
        throw
    }

    function main() -> int {
        call user.fails
        jump L8
        load_var e
        type_tag
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        jump L7

      L0:
        load_var e
        type_tag
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        jump L6

      L1:
        load_var e
        type_tag
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L5

      L2:
        load_var e
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L3
        jump L4

      L3:
        load_var e
        throw_if_panic
        load_const 0
        jump L8

      L4:
        load_const 4
        jump L8

      L5:
        load_const 3
        jump L8

      L6:
        load_const 2
        jump L8

      L7:
        load_const 1

      L8:
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn catch_jump_table_hits_int_arm() {
    let output = baml_test_optimized!(
        r#"
        function fails() -> int {
            throw 42
        }

        function main() -> int {
            fails() catch (e) {
                _: string => 1,
                _: int => 2,
                _: bool => 3,
                _: null => 4,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function fails() -> int {
        load_const 42
        throw
    }

    function main() -> int {
        call user.fails
        jump L8
        load_var e
        type_tag
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        jump L7

      L0:
        load_var e
        type_tag
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        jump L6

      L1:
        load_var e
        type_tag
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L5

      L2:
        load_var e
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L3
        jump L4

      L3:
        load_var e
        throw_if_panic
        load_const 0
        jump L8

      L4:
        load_const 4
        jump L8

      L5:
        load_const 3
        jump L8

      L6:
        load_const 2
        jump L8

      L7:
        load_const 1

      L8:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn catch_jump_table_hits_wildcard() {
    // Throw a float — no arm matches, falls through to wildcard.
    let output = baml_test_optimized!(
        r#"
        function fails() -> int {
            throw 3.14
        }

        function main() -> int {
            fails() catch (e) {
                _: string => 1,
                _: int => 2,
                _: bool => 3,
                _: null => 4,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function fails() -> int {
        load_const 3.14
        throw
    }

    function main() -> int {
        call user.fails
        jump L8
        load_var e
        type_tag
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        jump L7

      L0:
        load_var e
        type_tag
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        jump L6

      L1:
        load_var e
        type_tag
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L5

      L2:
        load_var e
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L3
        jump L4

      L3:
        load_var e
        throw_if_panic
        load_const 0
        jump L8

      L4:
        load_const 4
        jump L8

      L5:
        load_const 3
        jump L8

      L6:
        load_const 2
        jump L8

      L7:
        load_const 1

      L8:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

// ============================================================================
// If-Else Chain Tests (<4 type-tag arms)
// ============================================================================

#[tokio::test]
async fn catch_if_else_two_type_arms() {
    // Only 2 type arms — below the jump table threshold, uses if-else chain.
    let output = baml_test_optimized!(
        r#"
        function fails() -> int {
            throw "oops"
        }

        function main() -> int {
            fails() catch (e) {
                _: string => 1,
                _: int => 2,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        load_const "oops"
        throw
    }

    function main() -> int {
        call user.fails
        jump L4
        load_var e
        type_tag
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        type_tag
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw_if_panic
        load_const 0
        jump L4

      L2:
        load_const 2
        jump L4

      L3:
        load_const 1

      L4:
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn catch_if_else_three_type_arms() {
    // 3 type arms — still below the 4-arm jump table threshold.
    let output = baml_test_optimized!(
        r#"
        function fails() -> int {
            throw true
        }

        function main() -> int {
            fails() catch (e) {
                _: string => 1,
                _: int => 2,
                _: bool => 3,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function fails() -> int {
        load_const true
        throw
    }

    function main() -> int {
        call user.fails
        jump L6
        load_var e
        type_tag
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        jump L5

      L0:
        load_var e
        type_tag
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        jump L4

      L1:
        load_var e
        type_tag
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_var e
        throw_if_panic
        load_const 0
        jump L6

      L3:
        load_const 3
        jump L6

      L4:
        load_const 2
        jump L6

      L5:
        load_const 1

      L6:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// User Class Dispatch (instanceof chains — no type-tag switch)
// ============================================================================

#[tokio::test]
async fn catch_user_classes_instanceof_chain() {
    // User class arms use instanceof checks, not type-tag dispatch.
    // Even with 4+ arms, these remain sequential instanceof chains.
    let output = baml_test_optimized!(
        r#"
        class AuthError { reason: string }
        class NotFound { path: string }
        class RateLimit { retryAfter: int }
        class Timeout { ms: int }

        function api(mode: int) -> int {
            if (mode == 0) { throw AuthError { reason: "expired" } }
            if (mode == 1) { throw NotFound { path: "/users" } }
            if (mode == 2) { throw RateLimit { retryAfter: 30 } }
            if (mode == 3) { throw Timeout { ms: 5000 } }
            throw "unknown"
        }

        function main() -> int {
            api(2) catch (e) {
                _: AuthError => 1,
                _: NotFound => 2,
                _: RateLimit => 3,
                _: Timeout => 4,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function api(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L7

      L0:
        load_var mode
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        jump L6

      L1:
        load_var mode
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L5

      L2:
        load_var mode
        load_const 3
        cmp_op ==
        pop_jump_if_false L3
        jump L4

      L3:
        load_const "unknown"
        throw

      L4:
        alloc_instance Timeout
        load_const 5000
        init_field .ms
        throw

      L5:
        alloc_instance RateLimit
        load_const 30
        init_field .retryAfter
        throw

      L6:
        alloc_instance NotFound
        load_const "/users"
        init_field .path
        throw

      L7:
        alloc_instance AuthError
        load_const "expired"
        init_field .reason
        throw
    }

    function main() -> int {
        load_const 2
        call user.api
        jump L8
        load_var e
        load_const AuthError
        cmp_op instanceof
        pop_jump_if_false L0
        jump L7

      L0:
        load_var e
        load_const NotFound
        cmp_op instanceof
        pop_jump_if_false L1
        jump L6

      L1:
        load_var e
        load_const RateLimit
        cmp_op instanceof
        pop_jump_if_false L2
        jump L5

      L2:
        load_var e
        load_const Timeout
        cmp_op instanceof
        pop_jump_if_false L3
        jump L4

      L3:
        load_var e
        throw_if_panic
        load_const 0
        jump L8

      L4:
        load_const 4
        jump L8

      L5:
        load_const 3
        jump L8

      L6:
        load_const 2
        jump L8

      L7:
        load_const 1

      L8:
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// Literal Patterns Prevent Switch Dispatch
// ============================================================================

#[tokio::test]
async fn catch_literal_prevents_switch() {
    // Literal patterns are not type-tag eligible — falls back to sequential dispatch
    // even with 4+ arms. This is the same codegen as unoptimized.
    let output = baml_test_optimized!(
        r#"
        function fails(x: int) -> int {
            if (x == 0) { throw "boom" }
            if (x == 1) { throw "bang" }
            if (x == 2) { throw "crash" }
            throw "other"
        }

        function main() -> int {
            fails(1) catch (e) {
                "boom" => 10,
                "bang" => 20,
                "crash" => 30,
                "other" => 40,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(x: int) -> int {
        load_var x
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L5

      L0:
        load_var x
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        jump L4

      L1:
        load_var x
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_const "other"
        throw

      L3:
        load_const "crash"
        throw

      L4:
        load_const "bang"
        throw

      L5:
        load_const "boom"
        throw
    }

    function main() -> int {
        load_const 1
        call user.fails
        jump L8
        load_var e
        load_const "boom"
        cmp_op ==
        pop_jump_if_false L0
        jump L7

      L0:
        load_var e
        load_const "bang"
        cmp_op ==
        pop_jump_if_false L1
        jump L6

      L1:
        load_var e
        load_const "crash"
        cmp_op ==
        pop_jump_if_false L2
        jump L5

      L2:
        load_var e
        load_const "other"
        cmp_op ==
        pop_jump_if_false L3
        jump L4

      L3:
        load_var e
        throw_if_panic
        load_const 0
        jump L8

      L4:
        load_const 40
        jump L8

      L5:
        load_const 30
        jump L8

      L6:
        load_const 20
        jump L8

      L7:
        load_const 10

      L8:
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(20)));
}

// ============================================================================
// Named Binding With Type-Tag Dispatch
// ============================================================================

#[tokio::test]
async fn catch_named_binding_with_field_access() {
    // Named typed binding (err: ClassName) allows accessing fields in the arm body.
    // User classes use instanceof chains (not type-tag dispatch).
    let output = baml_test_optimized!(
        r#"
        class NetworkError { url: string }
        class AuthError { reason: string }
        class NotFound { path: string }
        class RateLimit { retryAfter: int }

        function fails() -> string {
            throw NetworkError { url: "http://example.com" }
        }

        function main() -> string {
            fails() catch (e) {
                err: NetworkError => err.url,
                err: AuthError => err.reason,
                err: NotFound => err.path,
                _: RateLimit => "rate limited",
                _ => "unknown"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> string {
        alloc_instance NetworkError
        load_const "http://example.com"
        init_field .url
        throw
    }

    function main() -> string {
        call user.fails
        jump L8
        load_var e
        load_const NetworkError
        cmp_op instanceof
        pop_jump_if_false L0
        jump L7

      L0:
        load_var e
        load_const AuthError
        cmp_op instanceof
        pop_jump_if_false L1
        jump L6

      L1:
        load_var e
        load_const NotFound
        cmp_op instanceof
        pop_jump_if_false L2
        jump L5

      L2:
        load_var e
        load_const RateLimit
        cmp_op instanceof
        pop_jump_if_false L3
        jump L4

      L3:
        load_var e
        throw_if_panic
        load_const "unknown"
        jump L8

      L4:
        load_const "rate limited"
        jump L8

      L5:
        load_var e
        load_field .path
        jump L8

      L6:
        load_var e
        load_field .reason
        jump L8

      L7:
        load_var e
        load_field .url

      L8:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("http://example.com".into()))
    );
}
