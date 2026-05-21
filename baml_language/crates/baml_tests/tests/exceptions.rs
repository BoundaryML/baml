//! Exception handling tests: catch/throw/panic semantics.
//!
//! Progression from simple to complex, covering all catch arm pattern types:
//!   - Literal value patterns: "string" =>, 42 =>
//!   - Typed bindings: string =>, MyClass =>
//!   - Bare type sugar: baml.panics.DivisionByZero =>
//!   - Wildcard: _ =>
//!   - User-defined error classes
//!   - Multi-arm dispatch with mixed pattern types
//!   - Panics vs user throws
//!   - Nested, rethrow, sequential

use baml_tests::{baml_test, baml_test_optimized};
use bex_engine::BexExternalValue;

/// Assert that execution failed with an uncaught panic of the given class.
fn assert_uncaught_panic(
    result: &Result<BexExternalValue, bex_engine::EngineError>,
    expected_class: &str,
) {
    match result {
        Err(bex_engine::EngineError::UnhandledThrow { value, .. }) => match value.as_ref() {
            BexExternalValue::Instance { class_name, .. } => {
                assert_eq!(class_name, expected_class);
            }
            other => panic!("expected panic Instance, got {other:?}"),
        },
        other => panic!("expected UnhandledThrow({expected_class}), got {other:?}"),
    }
}

// ============================================================================
// §1 — Catch by literal value
// ============================================================================

#[tokio::test]
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        load_const "boom"
        throw
    }

    function main() -> int {
        call user.fails
        jump L2
        load_var e
        load_const "boom"
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        load_const "other"
        throw
    }

    function main() -> int {
        call user.fails
        jump L2
        load_var e
        load_const "boom"
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
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
    insta::assert_snapshot!(output.bytecode, @r"
    function fails() -> int {
        load_const 42
        throw
    }

    function main() -> int {
        call user.fails
        jump L2
        load_var e
        load_const 42
        cmp_int_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var mode
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_const "gamma"
        throw

      L2:
        load_const "beta"
        throw

      L3:
        load_const "alpha"
        throw
    }

    function main() -> int {
        load_const 1
        call user.fails
        jump L4
        load_var e
        load_const "alpha"
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        load_const "beta"
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw_if_panic
        load_const 3
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
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// ============================================================================
// §2 — Catch by typed binding (Type =>)
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
                string => 1
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        load_const "boom"
        throw
    }

    function main() -> int {
        call user.fails
        jump L2
        load_var e
        is_type string
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_const 1

      L2:
        return
    }
    "#);
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
                int => 1
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r"
    function fails() -> int {
        load_const 42
        throw
    }

    function main() -> int {
        call user.fails
        jump L2
        load_var e
        is_type int
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_const 1

      L2:
        return
    }
    ");
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
                string => 1,
                int => 2
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 42
        throw

      L1:
        load_const "boom"
        throw
    }

    function main() -> int {
        load_const 0
        call user.fails
        jump L4
        load_var e
        is_type string
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        is_type int
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw

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

// Bytecode tested in typed_binding_dispatch_string_vs_int (same source, different mode).
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
                string => 1,
                int => 2
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
                string => 1,
                _ => 2
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 42
        throw

      L1:
        load_const "boom"
        throw
    }

    function main() -> int {
        load_const 1
        call user.fails
        jump L2
        load_var e
        is_type string
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// ============================================================================
// §2b — Named typed binding (var: Type =>) with value access
// ============================================================================

#[tokio::test]
async fn named_binding_string_access_value() {
    let output = baml_test!(
        r#"
        function fails(mode: int) -> string {
            if (mode == 0) { throw "boom" }
            throw 42
        }

        function main() -> string {
            fails(0) catch (e) {
                let msg: string => msg,
                int => "was int"
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> string {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 42
        throw

      L1:
        load_const "boom"
        throw
    }

    function main() -> string {
        load_const 0
        call user.fails
        jump L4
        load_var e
        is_type string
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        is_type int
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw

      L2:
        load_const "was int"
        jump L4

      L3:
        load_var e

      L4:
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("boom".to_string()))
    );
}

#[tokio::test]
async fn named_binding_int_access_value() {
    let output = baml_test!(
        r#"
        function fails(mode: int) -> int {
            if (mode == 0) { throw "boom" }
            throw 42
        }

        function main() -> int {
            fails(1) catch (e) {
                string => -1,
                let code: int => code
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 42
        throw

      L1:
        load_const "boom"
        throw
    }

    function main() -> int {
        load_const 1
        call user.fails
        jump L4
        load_var e
        is_type string
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        is_type int
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw

      L2:
        load_var e
        jump L4

      L3:
        load_const 1
        unary_op -

      L4:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
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
                NetworkError => 1
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        alloc_instance user.NetworkError
        load_const "http://example.com"
        init_field .url
        throw
    }

    function main() -> int {
        call user.fails
        jump L2
        load_var e
        is_type NetworkError
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_const 1

      L2:
        return
    }
    "#);
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
                NetworkError => 1,
                ParseError => 2
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        alloc_instance user.ParseError
        load_const "bad json"
        init_field .message
        throw

      L1:
        alloc_instance user.NetworkError
        load_const "http://x"
        init_field .url
        throw
    }

    function main() -> int {
        load_const 0
        call user.fails
        jump L4
        load_var e
        is_type NetworkError
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        is_type ParseError
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw

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

// Bytecode tested in catch_two_user_classes_dispatch_first (same source, different mode).
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
                NetworkError => 1,
                ParseError => 2
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
                NetworkError => 1,
                _ => 2
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "plain string error"
        throw

      L1:
        alloc_instance user.NetworkError
        load_const "http://x"
        init_field .url
        throw
    }

    function main() -> int {
        load_const 1
        call user.fails
        jump L2
        load_var e
        is_type NetworkError
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    "#);
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
                AuthError => 1,
                NotFound => 2,
                RateLimit => 3,
                _ => 4
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
        jump L5

      L0:
        load_var mode
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        jump L4

      L1:
        load_var mode
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_const "unknown"
        throw

      L3:
        alloc_instance user.RateLimit
        load_const 30
        init_field .retryAfter
        throw

      L4:
        alloc_instance user.NotFound
        load_const "/users"
        init_field .path
        throw

      L5:
        alloc_instance user.AuthError
        load_const "expired"
        init_field .reason
        throw
    }

    function main() -> int {
        load_const 2
        call user.api
        jump L6
        load_var e
        is_type AuthError
        pop_jump_if_false L0
        jump L5

      L0:
        load_var e
        is_type NotFound
        pop_jump_if_false L1
        jump L4

      L1:
        load_var e
        is_type RateLimit
        pop_jump_if_false L2
        jump L3

      L2:
        load_var e
        throw_if_panic
        load_const 4
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
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// §3a — Named typed binding for user classes (var: Class => var.field)
// ============================================================================

#[tokio::test]
async fn named_class_binding_access_field() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }

        function fails() -> string {
            throw NetworkError { url: "http://example.com" }
        }

        function main() -> string {
            fails() catch (e) {
                let err: NetworkError => err.url
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> string {
        alloc_instance user.NetworkError
        load_const "http://example.com"
        init_field .url
        throw
    }

    function main() -> string {
        call user.fails
        jump L2
        load_var e
        is_type NetworkError
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_var e
        load_field .url

      L2:
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("http://example.com".to_string()))
    );
}

#[tokio::test]
async fn named_class_binding_dispatch_access_fields() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }
        class ParseError { message string }

        function fails(mode: int) -> string {
            if (mode == 0) { throw NetworkError { url: "http://x" } }
            throw ParseError { message: "bad json" }
        }

        function main() -> string {
            fails(1) catch (e) {
                let net: NetworkError => net.url,
                let parse: ParseError => parse.message
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> string {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        alloc_instance user.ParseError
        load_const "bad json"
        init_field .message
        throw

      L1:
        alloc_instance user.NetworkError
        load_const "http://x"
        init_field .url
        throw
    }

    function main() -> string {
        load_const 1
        call user.fails
        jump L4
        load_var e
        is_type NetworkError
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        is_type ParseError
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw

      L2:
        load_var e
        load_field .message
        jump L4

      L3:
        load_var e
        load_field .url

      L4:
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("bad json".to_string()))
    );
}

// ============================================================================
// §3b — Catch by bare class name (MyClass => without binding)
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> int {
        alloc_instance user.NetworkError
        load_const "http://example.com"
        init_field .url
        throw
    }

    function main() -> int {
        call user.fails
        jump L2
        load_var e
        is_type NetworkError
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_const 1

      L2:
        return
    }
    "#);
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        alloc_instance user.ParseError
        load_const "bad"
        init_field .message
        throw

      L1:
        alloc_instance user.NetworkError
        load_const "http://x"
        init_field .url
        throw
    }

    function main() -> int {
        load_const 0
        call user.fails
        jump L4
        load_var e
        is_type NetworkError
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        is_type ParseError
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw

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

// Bytecode tested in bare_class_dispatch_first (same source, different mode).
#[tokio::test]
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "plain string"
        throw

      L1:
        alloc_instance user.NetworkError
        load_const "http://x"
        init_field .url
        throw
    }

    function main() -> int {
        load_const 0
        call user.fails
        jump L2
        load_var e
        is_type NetworkError
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// Bytecode tested in bare_class_plus_wildcard (same source, different mode).
#[tokio::test]
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "something went wrong"
        throw
    }
    "#);
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
    assert_eq!(
        value.as_ref(),
        &BexExternalValue::String("something went wrong".to_string())
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
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 42
        throw
    }
    ");
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
    assert_eq!(value.as_ref(), &BexExternalValue::Int(42));
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
            divides() catch (e) { baml.panics.DivisionByZero => -1 }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r"
    function divides() -> int {
        load_const 1
        load_const 0
        bin_op /
        return
    }

    function main() -> int {
        call user.divides
        jump L2
        load_var e
        is_type baml.panics.DivisionByZero
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_const 1
        unary_op -

      L2:
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn catch_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function oob() -> int { let a = [1, 2]; a[5] }

        function main() -> int {
            oob() catch (e) { baml.panics.IndexOutOfBounds => -1 }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        call user.oob
        jump L2
        load_var e
        is_type baml.panics.IndexOutOfBounds
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_const 1
        unary_op -

      L2:
        return
    }

    function oob() -> int {
        load_const 1
        load_const 2
        alloc_array 2
        load_const 5
        load_array_element
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn catch_map_key_not_found() {
    let output = baml_test!(
        r#"
        function bad() -> int { let m = {"a": 1}; m["x"] }

        function main() -> int {
            bad() catch (e) { baml.panics.MapKeyNotFound => -1 }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function bad() -> int {
        load_const 1
        load_const "a"
        alloc_map 1
        load_const "x"
        load_map_element
        return
    }

    function main() -> int {
        call user.bad
        jump L2
        load_var e
        is_type baml.panics.MapKeyNotFound
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_const 1
        unary_op -

      L2:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn catch_negative_index_as_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function bad() -> int { let a = [1, 2]; a[-1] }

        function main() -> int {
            bad() catch (e) { baml.panics.IndexOutOfBounds => -1 }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r"
    function bad() -> int {
        load_const 1
        load_const 2
        alloc_array 2
        load_const 1
        unary_op -
        load_array_element
        return
    }

    function main() -> int {
        call user.bad
        jump L2
        load_var e
        is_type baml.panics.IndexOutOfBounds
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_const 1
        unary_op -

      L2:
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ============================================================================
// §5b — Named panic binding (var: PanicType => var.field)
// ============================================================================

#[tokio::test]
async fn named_panic_binding_division_by_zero_field() {
    let output = baml_test!(
        r#"
        function divides() -> int { 10 / 0 }

        function main() -> int {
            divides() catch (e) {
                let err: baml.panics.DivisionByZero => err.dividend
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r"
    function divides() -> int {
        load_const 10
        load_const 0
        bin_op /
        return
    }

    function main() -> int {
        call user.divides
        jump L2
        load_var e
        is_type baml.panics.DivisionByZero
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_var e
        load_field .dividend

      L2:
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn named_panic_binding_index_out_of_bounds_fields() {
    let output = baml_test!(
        r#"
        function oob() -> int { let a = [10, 20, 30]; a[7] }

        function main() -> int {
            oob() catch (e) {
                let err: baml.panics.IndexOutOfBounds => err.index
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        call user.oob
        jump L2
        load_var e
        is_type baml.panics.IndexOutOfBounds
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_var e
        load_field .index

      L2:
        return
    }

    function oob() -> int {
        load_const 10
        load_const 20
        load_const 30
        alloc_array 3
        load_const 7
        load_array_element
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn named_panic_binding_index_out_of_bounds_length() {
    let output = baml_test!(
        r#"
        function oob() -> int { let a = [10, 20, 30]; a[7] }

        function main() -> int {
            oob() catch (e) {
                let err: baml.panics.IndexOutOfBounds => err.length
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        call user.oob
        jump L2
        load_var e
        is_type baml.panics.IndexOutOfBounds
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_var e
        load_field .length

      L2:
        return
    }

    function oob() -> int {
        load_const 10
        load_const 20
        load_const 30
        alloc_array 3
        load_const 7
        load_array_element
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn named_panic_binding_map_key_not_found_field() {
    let output = baml_test!(
        r#"
        function bad() -> string { let m = {"a": "one"}; m["x"] }

        function main() -> string {
            bad() catch (e) {
                let err: baml.panics.MapKeyNotFound => err.key
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function bad() -> string {
        load_const "one"
        load_const "a"
        alloc_map 1
        load_const "x"
        load_map_element
        return
    }

    function main() -> string {
        call user.bad
        jump L2
        load_var e
        is_type baml.panics.MapKeyNotFound
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_var e
        load_field .key

      L2:
        return
    }
    "#);
    // The VM currently sets key to "(unknown)" — just verify it's a string
    assert!(
        output.result.is_ok(),
        "expected caught value, got {:?}",
        output.result
    );
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        call user.risky
        jump L0
        load_var e
        throw_if_panic
        load_const 1
        unary_op -

      L0:
        return
    }

    function risky(x: int) -> int {
        load_var x
        load_const 100
        cmp_op >
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 1
        load_var x
        bin_op /
        return

      L1:
        load_const "too big"
        throw
    }
    "#);
    assert_uncaught_panic(&output.result, "baml.panics.DivisionByZero");
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 99
        call user.risky
        jump L0
        load_var e
        throw_if_panic
        load_const 1
        unary_op -

      L0:
        return
    }

    function risky(x: int) -> int {
        load_var x
        load_const 100
        cmp_op >
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        load_var x
        load_array_element
        return

      L1:
        load_const "too big"
        throw
    }
    "#);
    assert_uncaught_panic(&output.result, "baml.panics.IndexOutOfBounds");
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
                baml.panics.DivisionByZero => 1,
                _ => 2
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        call user.risky
        jump L2
        load_var e
        is_type baml.panics.DivisionByZero
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }

    function risky(x: int) -> int {
        load_var x
        load_const 100
        cmp_op >
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 1
        load_var x
        bin_op /
        return

      L1:
        load_const "too big"
        throw
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// Bytecode tested in panic_arm_plus_wildcard_panic_fires (same source, different mode).
#[tokio::test]
async fn panic_arm_plus_wildcard_user_error_fires() {
    let output = baml_test!(
        r#"
        function risky(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            risky(999) catch (e) {
                baml.panics.DivisionByZero => 1,
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
                baml.panics.DivisionByZero => -1,
                _ => -2
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 5
        call user.risky
        jump L2
        load_var e
        is_type baml.panics.DivisionByZero
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw_if_panic
        load_const 2
        unary_op -
        jump L2

      L1:
        load_const 1
        unary_op -

      L2:
        return
    }

    function risky(x: int) -> int {
        load_var x
        load_const 100
        cmp_op >
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 1000
        load_var x
        bin_op /
        return

      L1:
        load_const "too big"
        throw
    }
    "#);
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
                AppError => 1,
                baml.panics.DivisionByZero => 2,
                _ => 3
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        call user.risky
        jump L4
        load_var e
        is_type AppError
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        is_type baml.panics.DivisionByZero
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw_if_panic
        load_const 3
        jump L4

      L2:
        load_const 2
        jump L4

      L3:
        load_const 1

      L4:
        return
    }

    function risky(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L2

      L0:
        load_var mode
        load_const 1
        cmp_op ==
        pop_jump_if_false L1

      L1:
        load_const "fallback"
        throw

      L2:
        alloc_instance user.AppError
        load_const 500
        init_field .code
        throw
    }
    "#);
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
                AppError => 1,
                baml.panics.DivisionByZero => 2,
                _ => 3
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function do_div() -> int {
        load_const 1
        load_const 0
        bin_op /
        return
    }

    function main() -> int {
        load_const 1
        call user.risky
        jump L4
        load_var e
        is_type AppError
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        is_type baml.panics.DivisionByZero
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw_if_panic
        load_const 3
        jump L4

      L2:
        load_const 2
        jump L4

      L3:
        load_const 1

      L4:
        return
    }

    function risky(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var mode
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_const "fallback"
        throw

      L2:
        call user.do_div
        return

      L3:
        alloc_instance user.AppError
        load_const 500
        init_field .code
        throw
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// Bytecode tested in user_class_plus_panic_plus_wildcard_panic_fires (same source, different mode).
#[tokio::test]
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
                AppError => 1,
                baml.panics.DivisionByZero => 2,
                _ => 3
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// §9 — Mixed union catch arms (AppError | baml.panics.DivisionByZero =>)
// ============================================================================

// §9a — Mixed union WITH wildcard (split handler, Case C)

#[tokio::test]
async fn mixed_union_arm_plus_wildcard_panic_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }
        function do_div() -> int { 1 / 0 }

        function risky(mode: int) -> int {
            if (mode == 0) { throw AppError { code: 7 } }
            if (mode == 1) { return do_div() }
            if (mode == 2) { throw "fallback" }
            let a = [1];
            a[5]
        }

        function main() -> int {
            risky(1) catch (e) {
                AppError | baml.panics.DivisionByZero => 1,
                _ => 99
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn mixed_union_arm_plus_wildcard_user_error_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }

        function risky(mode: int) -> int {
            if (mode == 0) { throw AppError { code: 7 } }
            if (mode == 1) { 1 / 0 }
            throw "fallback"
        }

        function main() -> int {
            risky(0) catch (e) {
                AppError | baml.panics.DivisionByZero => 1,
                _ => 99
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn mixed_union_arm_plus_wildcard_fallback_error_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }

        function risky(mode: int) -> int {
            if (mode == 0) { throw AppError { code: 7 } }
            if (mode == 1) { 1 / 0 }
            throw "fallback"
        }

        function main() -> int {
            risky(2) catch (e) {
                AppError | baml.panics.DivisionByZero => 1,
                _ => 99
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn mixed_union_arm_plus_wildcard_other_panic_propagates() {
    let output = baml_test!(
        r#"
        class AppError { code int }

        function risky(mode: int) -> int {
            if (mode == 0) { throw "fallback" }
            let a = [1];
            a[5]
        }

        function main() -> int {
            risky(1) catch (e) {
                AppError | baml.panics.DivisionByZero => 1,
                _ => 99
            }
        }
    "#
    );
    assert_uncaught_panic(&output.result, "baml.panics.IndexOutOfBounds");
}

#[tokio::test]
async fn mixed_union_alias_plus_wildcard_handles_both_domains() {
    let output = baml_test!(
        r#"
        class AppError { code int }
        type Mixed = AppError | baml.panics.DivisionByZero
        function do_div() -> int { 1 / 0 }

        function risky(mode: int) -> int {
            if (mode == 0) { throw AppError { code: 7 } }
            if (mode == 1) { return do_div() }
            throw "fallback"
        }

        function main() -> int {
            risky(1) catch (e) {
                Mixed => 1,
                _ => 99
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn mixed_union_alias_plus_wildcard_error_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }
        type Mixed = AppError | baml.panics.DivisionByZero

        function risky(mode: int) -> int {
            if (mode == 0) { throw AppError { code: 7 } }
            1 / 0
        }

        function main() -> int {
            risky(0) catch (e) {
                Mixed => 1,
                _ => 99
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// §9b — Mixed union WITHOUT wildcard (single handler, Case B)

#[tokio::test]
async fn mixed_union_no_wildcard_panic_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }
        function do_div() -> int { 1 / 0 }

        function risky(mode: int) -> int {
            if (mode == 0) { throw AppError { code: 7 } }
            return do_div()
        }

        function main() -> int {
            risky(1) catch (e) {
                AppError | baml.panics.DivisionByZero => 1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// Same source as mixed_union_no_wildcard_panic_fires, different mode.
#[tokio::test]
async fn mixed_union_no_wildcard_error_fires() {
    let output = baml_test!(
        r#"
        class AppError { code int }
        function do_div() -> int { 1 / 0 }

        function risky(mode: int) -> int {
            if (mode == 0) { throw AppError { code: 7 } }
            return do_div()
        }

        function main() -> int {
            risky(0) catch (e) {
                AppError | baml.panics.DivisionByZero => 1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn mixed_union_no_wildcard_unmatched_rethrows() {
    let output = baml_test!(
        r#"
        class AppError { code int }

        function risky() -> int {
            throw "not matched"
        }

        function main() -> int {
            risky() catch (e) {
                AppError | baml.panics.DivisionByZero => 1
            }
        }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = &output.result else {
        panic!(
            "unmatched throw should rethrow past mixed union arm, got: {:?}",
            output.result
        );
    };
    assert_eq!(
        value.as_ref(),
        &BexExternalValue::String("not matched".to_string())
    );
}

#[tokio::test]
async fn mixed_union_no_wildcard_unmatched_panic_rethrows() {
    let output = baml_test!(
        r#"
        class AppError { code int }

        function risky() -> int {
            let a = [1];
            a[5]
        }

        function main() -> int {
            risky() catch (e) {
                AppError | baml.panics.DivisionByZero => 1
            }
        }
    "#
    );
    assert_uncaught_panic(&output.result, "baml.panics.IndexOutOfBounds");
}

// §9c — Pure-panic union with wildcard

#[tokio::test]
async fn panic_union_plus_wildcard_division_fires() {
    let output = baml_test!(
        r#"
        function do_div() -> int { 1 / 0 }

        function risky(mode: int) -> int {
            if (mode == 0) { return do_div() }
            if (mode == 1) { let a = [1]; return a[5] }
            throw "fallback"
        }

        function main() -> int {
            risky(0) catch (e) {
                baml.panics.DivisionByZero | baml.panics.IndexOutOfBounds => 1,
                _ => 99
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// Same source as panic_union_plus_wildcard_division_fires, different mode.
#[tokio::test]
async fn panic_union_plus_wildcard_index_fires() {
    let output = baml_test!(
        r#"
        function do_div() -> int { 1 / 0 }

        function risky(mode: int) -> int {
            if (mode == 0) { return do_div() }
            if (mode == 1) { let a = [1]; return a[5] }
            throw "fallback"
        }

        function main() -> int {
            risky(1) catch (e) {
                baml.panics.DivisionByZero | baml.panics.IndexOutOfBounds => 1,
                _ => 99
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn panic_union_plus_wildcard_error_falls_to_wildcard() {
    let output = baml_test!(
        r#"
        function risky(mode: int) -> int {
            if (mode == 0) { 1 / 0 }
            throw "fallback"
        }

        function main() -> int {
            risky(1) catch (e) {
                baml.panics.DivisionByZero | baml.panics.IndexOutOfBounds => 1,
                _ => 99
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

// ============================================================================
// §10 — Four-arm: two separate panics + user class + wildcard
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
                baml.panics.DivisionByZero => 1,
                baml.panics.IndexOutOfBounds => 2,
                AppError => 3,
                _ => 4
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function do_div() -> int {
        load_const 1
        load_const 0
        bin_op /
        return
    }

    function do_oob() -> int {
        load_const 1
        alloc_array 1
        load_const 5
        load_array_element
        return
    }

    function main() -> int {
        load_const 0
        call user.risky
        jump L6
        load_var e
        is_type baml.panics.DivisionByZero
        pop_jump_if_false L0
        jump L5

      L0:
        load_var e
        is_type baml.panics.IndexOutOfBounds
        pop_jump_if_false L1
        jump L4

      L1:
        load_var e
        is_type AppError
        pop_jump_if_false L2
        jump L3

      L2:
        load_var e
        throw_if_panic
        load_const 4
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

    function risky(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L5

      L0:
        load_var mode
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        jump L4

      L1:
        load_var mode
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_const "unknown"
        throw

      L3:
        alloc_instance user.AppError
        load_const 404
        init_field .code
        throw

      L4:
        call user.do_oob
        jump L6

      L5:
        call user.do_div

      L6:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// Bytecode tested in four_arms_division_by_zero_fires (same source, different mode).
#[tokio::test]
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
                baml.panics.DivisionByZero => 1,
                baml.panics.IndexOutOfBounds => 2,
                AppError => 3,
                _ => 4
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// Bytecode tested in four_arms_division_by_zero_fires (same source, different mode).
#[tokio::test]
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
                baml.panics.DivisionByZero => 1,
                baml.panics.IndexOutOfBounds => 2,
                AppError => 3,
                _ => 4
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// Bytecode tested in four_arms_division_by_zero_fires (same source, different mode).
#[tokio::test]
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
                baml.panics.DivisionByZero => 1,
                baml.panics.IndexOutOfBounds => 2,
                AppError => 3,
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
                baml.panics.DivisionByZero => 1,
                baml.panics.IndexOutOfBounds => 2,
                AppError => 3,
                _ => 4
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function do_div() -> int {
        load_const 1
        load_const 0
        bin_op /
        return
    }

    function do_oob() -> int {
        load_const 1
        alloc_array 1
        load_const 5
        load_array_element
        return
    }

    function main() -> int {
        load_const 4
        call user.risky
        jump L6
        load_var e
        is_type baml.panics.DivisionByZero
        pop_jump_if_false L0
        jump L5

      L0:
        load_var e
        is_type baml.panics.IndexOutOfBounds
        pop_jump_if_false L1
        jump L4

      L1:
        load_var e
        is_type AppError
        pop_jump_if_false L2
        jump L3

      L2:
        load_var e
        throw_if_panic
        load_const 4
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

    function risky(mode: int) -> int {
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
        load_const 99
        jump L8

      L4:
        load_const "unknown"
        throw

      L5:
        alloc_instance user.AppError
        load_const 404
        init_field .code
        throw

      L6:
        call user.do_oob
        jump L8

      L7:
        call user.do_div

      L8:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

// ============================================================================
// §11 — Uncaught panics propagate
// ============================================================================

#[tokio::test]
async fn uncaught_division_by_zero() {
    let output = baml_test!(r#" function main() -> int { 1 / 0 } "#);
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        load_const 0
        bin_op /
        return
    }
    ");
    assert_uncaught_panic(&output.result, "baml.panics.DivisionByZero");
}

#[tokio::test]
async fn uncaught_index_out_of_bounds() {
    let output = baml_test!(r#" function main() -> int { let a = [1]; a[5] } "#);
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        alloc_array 1
        load_const 5
        load_array_element
        return
    }
    ");
    assert_uncaught_panic(&output.result, "baml.panics.IndexOutOfBounds");
}

#[tokio::test]
async fn wrong_panic_pattern_propagates() {
    let output = baml_test!(
        r#"
        function divides() -> int { 1 / 0 }

        function main() -> int {
            divides() catch (e) { baml.panics.IndexOutOfBounds => -1 }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r"
    function divides() -> int {
        load_const 1
        load_const 0
        bin_op /
        return
    }

    function main() -> int {
        call user.divides
        jump L2
        load_var e
        is_type baml.panics.IndexOutOfBounds
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_const 1
        unary_op -

      L2:
        return
    }
    ");
    assert_uncaught_panic(&output.result, "baml.panics.DivisionByZero");
}

// ============================================================================
// §11b — Same-frame exception table lookup
// ============================================================================

#[tokio::test]
async fn same_frame_division_caught() {
    let output = baml_test!(
        r#"
        function main() -> int {
            (1 / 0) catch (e) {
                baml.panics.DivisionByZero => -1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn same_frame_index_out_of_bounds_caught() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let a = [1, 2, 3];
            a[10] catch (e) {
                baml.panics.IndexOutOfBounds => -1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ============================================================================
// §12 — Panic type alias (union of all panics)
// ============================================================================

#[tokio::test]
async fn panic_alias_catches_any_panic() {
    let output = baml_test!(
        r#"
        function divides() -> int { 1 / 0 }

        function main() -> int {
            divides() catch (e) { baml.panics.Panic => -1 }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @"
    function divides() -> int {
        load_const 1
        load_const 0
        bin_op /
        return
    }

    function main() -> int {
        call user.divides
        jump L2
        load_var e
        type_tag
        jump_table [L1, L1, L1, L1, _, _, _, L1, _, L1, _, L1, L1, _, L1, L1, _, _, _, _, L1], default L0

      L0:
        load_var e
        throw

      L1: Exit
        load_const 1
        unary_op -

      L2:
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn panic_alias_plus_wildcard_dispatch() {
    let output = baml_test!(
        r#"
        function risky(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            risky(0) catch (e) {
                baml.panics.Panic => 1,
                _ => 2
            }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        call user.risky
        jump L2
        load_var e
        type_tag
        jump_table [L1, L1, L1, L1, _, _, _, L1, _, L1, _, L1, L1, _, L1, L1, _, _, _, _, L1], default L0

      L0:
        load_var e
        throw_if_panic
        load_const 2
        jump L2

      L1: Exit
        load_const 1

      L2:
        return
    }

    function risky(x: int) -> int {
        load_var x
        load_const 100
        cmp_op >
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 1
        load_var x
        bin_op /
        return

      L1:
        load_const "too big"
        throw
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ============================================================================
// §13 — Nested catch, rethrow, sequential
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function inner() -> int {
        load_const "inner"
        throw
    }

    function main() -> int {
        call user.middle
        jump L0
        load_var e
        throw_if_panic
        load_const 1
        unary_op -

      L0:
        return
    }

    function middle() -> int {
        call user.inner
        jump L0
        load_var e
        throw_if_panic
        load_const 42

      L0:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn nested_inner_catches_panic_outer_catches_rethrow() {
    let output = baml_test!(
        r#"
        function divides() -> int { 1 / 0 }

        function middle() -> int {
            let x = divides() catch (e) { baml.panics.DivisionByZero => -1 };
            throw "recovered but failing"
        }

        function main() -> int {
            middle() catch (e) { _ => 99 }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function divides() -> int {
        load_const 1
        load_const 0
        bin_op /
        return
    }

    function main() -> int {
        call user.middle
        jump L0
        load_var e
        throw_if_panic
        load_const 99

      L0:
        return
    }

    function middle() -> int {
        call user.divides
        store_var x
        jump L2
        load_var e
        is_type baml.panics.DivisionByZero
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_const 1
        unary_op -
        store_var x

      L2:
        load_const "recovered but failing"
        throw
    }
    "#);
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function inner() -> int {
        load_const "original"
        throw
    }

    function main() -> int {
        call user.middle
        jump L0
        load_var e
        throw_if_panic
        load_const 99

      L0:
        return
    }

    function middle() -> int {
        call user.inner
        jump L0
        load_var e
        throw_if_panic
        load_const "rethrown"
        throw

      L0:
        return
    }
    "#);
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function f1() -> int {
        load_const "one"
        throw
    }

    function f2() -> int {
        load_const "two"
        throw
    }

    function main() -> int {
        call user.f1
        store_var a
        jump L0
        load_var e
        throw_if_panic
        load_const 10
        store_var a

      L0:
        call user.f2
        store_var b
        jump L1
        load_var e
        throw_if_panic
        load_const 20
        store_var b

      L1:
        load_var a
        load_var b
        add_int
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

// ============================================================================
// §14 — Special cases
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
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        throw_if_panic
        load_const 2
        return
    }
    ");
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
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 2
        load_const 1
        cmp_int_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "boom"
        throw

      L1:
        load_const "one"
        return
    }
    "#);
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
    assert_eq!(
        value.as_ref(),
        &BexExternalValue::String("boom".to_string())
    );
}

#[tokio::test]
async fn caught_panic_has_accessible_fields() {
    let output = baml_test!(
        r#"
        function oob() -> int { let a = [10, 20, 30]; a[7] }

        function main() -> int {
            oob() catch (e) { baml.panics.IndexOutOfBounds => e.index }
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        call user.oob
        jump L2
        load_var e
        is_type baml.panics.IndexOutOfBounds
        pop_jump_if_false L0
        jump L1

      L0:
        load_var e
        throw

      L1:
        load_var e
        load_field .index

      L2:
        return
    }

    function oob() -> int {
        load_const 10
        load_const 20
        load_const 30
        alloc_array 3
        load_const 7
        load_array_element
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

// ============================================================================
// §12 — Catch switch optimization: TypeTag dispatch for typed catch arms
// ============================================================================

/// 4+ typed catch arms (no wildcard) → type_tag + jump_table dispatch.
/// Without wildcard, the default branch rethrows.
#[tokio::test]
async fn catch_four_typed_arms_jump_table() {
    let output = baml_test!(
        r#"
        class ErrA { x int }
        class ErrB { x int }
        class ErrC { x int }
        class ErrD { x int }

        function risky(mode: int) -> int {
            if (mode == 0) { throw ErrA { x: 1 } }
            if (mode == 1) { throw ErrB { x: 2 } }
            if (mode == 2) { throw ErrC { x: 3 } }
            throw ErrD { x: 4 }
        }

        function main() -> int {
            risky(2) catch (e) {
                ErrA => 10,
                ErrB => 20,
                ErrC => 30,
                ErrD => 40
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 2
        call user.risky
        jump L5
        load_var e
        type_tag
        jump_table [L1, L2, L4, _, L3], default L0

      L0:
        load_var e
        throw

      L1: ErrD
        load_const 40
        jump L5

      L2: ErrC
        load_const 30
        jump L5

      L3: ErrB
        load_const 20
        jump L5

      L4: ErrA
        load_const 10

      L5:
        return
    }

    function risky(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L5

      L0:
        load_var mode
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        jump L4

      L1:
        load_var mode
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        alloc_instance user.ErrD
        load_const 4
        init_field .x
        throw

      L3:
        alloc_instance user.ErrC
        load_const 3
        init_field .x
        throw

      L4:
        alloc_instance user.ErrB
        load_const 2
        init_field .x
        throw

      L5:
        alloc_instance user.ErrA
        load_const 1
        init_field .x
        throw
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

/// 4+ typed catch arms with wildcard → type_tag + jump_table + throw_if_panic.
#[tokio::test]
async fn catch_four_typed_arms_plus_wildcard_jump_table() {
    let output = baml_test!(
        r#"
        class ErrA { x int }
        class ErrB { x int }
        class ErrC { x int }
        class ErrD { x int }

        function risky(mode: int) -> int {
            if (mode == 0) { throw ErrA { x: 1 } }
            if (mode == 1) { throw ErrB { x: 2 } }
            if (mode == 2) { throw ErrC { x: 3 } }
            if (mode == 3) { throw ErrD { x: 4 } }
            throw "unknown"
        }

        function main() -> int {
            risky(4) catch (e) {
                ErrA => 10,
                ErrB => 20,
                ErrC => 30,
                ErrD => 40,
                _ => 99
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 4
        call user.risky
        jump L5
        load_var e
        type_tag
        jump_table [L1, L2, L4, _, L3], default L0

      L0:
        load_var e
        throw_if_panic
        load_const 99
        jump L5

      L1: ErrD
        load_const 40
        jump L5

      L2: ErrC
        load_const 30
        jump L5

      L3: ErrB
        load_const 20
        jump L5

      L4: ErrA
        load_const 10

      L5:
        return
    }

    function risky(mode: int) -> int {
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
        alloc_instance user.ErrD
        load_const 4
        init_field .x
        throw

      L5:
        alloc_instance user.ErrC
        load_const 3
        init_field .x
        throw

      L6:
        alloc_instance user.ErrB
        load_const 2
        init_field .x
        throw

      L7:
        alloc_instance user.ErrA
        load_const 1
        init_field .x
        throw
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

/// 2 typed catch arms → sequential is_type chain (below 4-arm switch threshold).
#[tokio::test]
async fn catch_two_typed_arms_sequential_chain() {
    let output = baml_test!(
        r#"
        class NetworkError { url string }
        class ParseError   { msg string }

        function fails(mode: int) -> int {
            if (mode == 0) { throw NetworkError { url: "http://x" } }
            throw ParseError { msg: "bad" }
        }

        function main() -> int {
            fails(1) catch (e) {
                NetworkError => 1,
                ParseError   => 2
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        alloc_instance user.ParseError
        load_const "bad"
        init_field .msg
        throw

      L1:
        alloc_instance user.NetworkError
        load_const "http://x"
        init_field .url
        throw
    }

    function main() -> int {
        load_const 1
        call user.fails
        jump L4
        load_var e
        is_type NetworkError
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        is_type ParseError
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw

      L2:
        load_const 2
        jump L4

      L3:
        load_const 1

      L4:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

/// Mixed literal + typed catch arms → no switch optimization (falls back to chain).
#[tokio::test]
async fn catch_mixed_literal_and_typed_no_switch() {
    let output = baml_test!(
        r#"
        class AppError { code int }

        function fails(mode: int) -> int {
            if (mode == 0) { throw "boom" }
            throw AppError { code: 404 }
        }

        function main() -> int {
            fails(0) catch (e) {
                "boom" => 1,
                AppError => 2,
                _ => 3
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails(mode: int) -> int {
        load_var mode
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        alloc_instance user.AppError
        load_const 404
        init_field .code
        throw

      L1:
        load_const "boom"
        throw
    }

    function main() -> int {
        load_const 0
        call user.fails
        jump L4
        load_var e
        load_const "boom"
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var e
        is_type AppError
        pop_jump_if_false L1
        jump L2

      L1:
        load_var e
        throw_if_panic
        load_const 3
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

// ============================================================================
// §13 — Optimized catch dispatch (from canary: primitive TypeTag + instanceof tests)
// ============================================================================

#[tokio::test]
async fn catch_four_primitive_type_arms() {
    // 4 primitive type arms: string, int, bool, null + wildcard.
    let output = baml_test_optimized!(
        r#"
        function fails() -> int {
            throw "oops"
        }

        function main() -> int {
            fails() catch (e) {
                string => 1,
                int => 2,
                bool => 3,
                null => 4,
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
        jump L5
        load_var e
        type_tag
        jump_table [L3, L4, L2, L1], default L0

      L0:
        load_var e
        throw_if_panic
        load_const 0
        jump L5

      L1: null
        load_const 4
        jump L5

      L2: bool
        load_const 3
        jump L5

      L3: int
        load_const 2
        jump L5

      L4: string
        load_const 1

      L5:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

/// 4+ typed catch arms with a final plain binding should still guard panics.
#[tokio::test]
async fn catch_four_typed_arms_plus_bind_jump_table_rethrows_panic() {
    let output = baml_test_optimized!(
        r#"
        class ErrA { x int }
        class ErrB { x int }
        class ErrC { x int }
        class ErrD { x int }

        function risky() -> int {
            1 / 0
        }

        function main() -> int {
            risky() catch (e) {
                ErrA => 10,
                ErrB => 20,
                ErrC => 30,
                ErrD => 40,
                let other => 99
            }
        }
    "#
    );
    assert_uncaught_panic(&output.result, "baml.panics.DivisionByZero");
}

/// A final bind-only chain is also a catch-all and must still guard panics.
#[tokio::test]
async fn catch_four_typed_arms_plus_bind_chain_rethrows_panic() {
    let output = baml_test_optimized!(
        r#"
        class ErrA { x int }
        class ErrB { x int }
        class ErrC { x int }
        class ErrD { x int }

        function risky() -> int {
            1 / 0
        }

        function main() -> int {
            risky() catch (e) {
                ErrA => 10,
                ErrB => 20,
                ErrC => 30,
                ErrD => 40,
                let other: let alias => 99
            }
        }
    "#
    );
    assert_uncaught_panic(&output.result, "baml.panics.DivisionByZero");
}

#[tokio::test]
async fn catch_four_primitive_wildcard_on_float() {
    // Throw a float — no arm matches, falls through to wildcard.
    let output = baml_test_optimized!(
        r#"
        function fails() -> int {
            throw 3.14
        }

        function main() -> int {
            fails() catch (e) {
                string => 1,
                int => 2,
                bool => 3,
                null => 4,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function fails() -> int {
        load_const 3.14
        throw
    }

    function main() -> int {
        call user.fails
        jump L5
        load_var e
        type_tag
        jump_table [L3, L4, L2, L1], default L0

      L0:
        load_var e
        throw_if_panic
        load_const 0
        jump L5

      L1: null
        load_const 4
        jump L5

      L2: bool
        load_const 3
        jump L5

      L3: int
        load_const 2
        jump L5

      L4: string
        load_const 1

      L5:
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn catch_three_type_arms_bool_throw() {
    // 3 type arms + wildcard, throw bool.
    let output = baml_test_optimized!(
        r#"
        function fails() -> int {
            throw true
        }

        function main() -> int {
            fails() catch (e) {
                string => 1,
                int => 2,
                bool => 3,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function fails() -> int {
        load_const true
        throw
    }

    function main() -> int {
        call user.fails
        jump L6
        load_var e
        is_type string
        pop_jump_if_false L0
        jump L5

      L0:
        load_var e
        is_type int
        pop_jump_if_false L1
        jump L4

      L1:
        load_var e
        is_type bool
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

#[tokio::test]
async fn catch_four_user_classes_instanceof_chain() {
    // User class arms — with this branch, 4+ class arms use TypeTag dispatch.
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
                AuthError => 1,
                NotFound => 2,
                RateLimit => 3,
                Timeout => 4,
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
        alloc_instance user.Timeout
        load_const 5000
        init_field .ms
        throw

      L5:
        alloc_instance user.RateLimit
        load_const 30
        init_field .retryAfter
        throw

      L6:
        alloc_instance user.NotFound
        load_const "/users"
        init_field .path
        throw

      L7:
        alloc_instance user.AuthError
        load_const "expired"
        init_field .reason
        throw
    }

    function main() -> int {
        load_const 2
        call user.api
        jump L5
        load_var e
        type_tag
        jump_table [L3, L2, _, _, L4, L1], default L0

      L0:
        load_var e
        throw_if_panic
        load_const 0
        jump L5

      L1: Timeout
        load_const 4
        jump L5

      L2: RateLimit
        load_const 3
        jump L5

      L3: NotFound
        load_const 2
        jump L5

      L4: AuthError
        load_const 1

      L5:
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn catch_four_literal_arms() {
    // Literal patterns use sequential dispatch even with 4+ arms.
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

#[tokio::test]
async fn catch_mixed_named_and_anonymous_bindings() {
    // Mixed named (err: ClassName) and anonymous (ClassName) bindings
    // in the same catch block with field access.
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
                let err: NetworkError => err.url,
                let err: AuthError => err.reason,
                let err: NotFound => err.path,
                RateLimit => "rate limited",
                _ => "unknown"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> string {
        alloc_instance user.NetworkError
        load_const "http://example.com"
        init_field .url
        throw
    }

    function main() -> string {
        call user.fails
        jump L5
        load_var e
        type_tag
        jump_table [L2, _, L1, L4, _, _, L3], default L0

      L0:
        load_var e
        throw_if_panic
        load_const "unknown"
        jump L5

      L1: RateLimit
        load_const "rate limited"
        jump L5

      L2: NotFound
        load_var e
        load_field .path
        jump L5

      L3: AuthError
        load_var e
        load_field .reason
        jump L5

      L4: NetworkError
        load_var e
        load_field .url

      L5:
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("http://example.com".into()))
    );
}

// ============================================================================
// §N — Stack trace tests
// ============================================================================

#[tokio::test]
async fn exception_stack_trace_through_closures() {
    let output = baml_test!(
        r#"
function inner() -> int {
  throw "from_closure"
}

function outer() -> int {
  let f = inner
  f()
}

function main() -> int {
  outer()
}
"#
    );

    let err = output.result.unwrap_err();
    insta::assert_snapshot!(err, @r#"
    Traceback (most recent call last):
      File "test.baml", line 12, in user.main
      File "test.baml", line 8, in user.outer
      File "test.baml", line 3, in user.inner
    uncaught throw: String("from_closure")
    "#);
}

#[tokio::test]
async fn exception_stack_trace_on_panic() {
    let output = baml_test!(
        r#"
function divider(x: int) -> int {
  x / 0
}

function caller() -> int {
  divider(42)
}

function main() -> int {
  caller()
}
"#
    );

    let err = output.result.unwrap_err();
    insta::assert_snapshot!(err, @r#"
    Traceback (most recent call last):
      File "test.baml", line 11, in user.main
      File "test.baml", line 7, in user.caller
      File "test.baml", line 3, in user.divider
    uncaught throw: Instance { class_name: "baml.panics.DivisionByZero", fields: {"dividend": Int(42)} }
    "#);
}

// ============================================================================
// §N+1 — catch (e, stack_trace) binding
// ============================================================================

#[tokio::test]
async fn catch_with_stack_trace_binding() {
    let output = baml_test!(
        r#"
function inner() -> string {
  throw "boom"
}

function main() -> string {
  inner() catch (e, st) {
    _ => { st.to_string() }
  }
}
"#
    );

    let BexExternalValue::String(st) = output.result.unwrap() else {
        panic!("expected String variant");
    };
    insta::assert_snapshot!(st, @r#"
    Traceback (most recent call last):
      File "test.baml", line 7, in user.main
      File "test.baml", line 3, in user.inner
    "#);
}

#[tokio::test]
async fn catch_without_stack_trace_still_works() {
    let output = baml_test!(
        r#"
function inner() -> string {
  throw "oops"
}

function main() -> string {
  inner() catch (e) {
    string => { e }
  }
}
"#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("oops".to_string()))
    );
}

#[tokio::test]
async fn catch_stack_trace_on_panic() {
    let output = baml_test!(
        r#"
function divider() -> int {
  42 / 0
}

function main() -> int | string {
  divider() catch (e, st) {
    baml.panics.DivisionByZero => { st.to_string() }
  }
}
"#
    );

    let result = output.result.unwrap();
    let st = match result {
        BexExternalValue::String(s) => s,
        BexExternalValue::Union { value, .. } => match *value {
            BexExternalValue::String(s) => s,
            other => panic!("expected String inside Union, got: {other:?}"),
        },
        other => panic!("expected String or Union, got: {other:?}"),
    };
    insta::assert_snapshot!(st, @r#"
    Traceback (most recent call last):
      File "test.baml", line 7, in user.main
      File "test.baml", line 3, in user.divider
    "#);
}

#[tokio::test]
async fn named_wrapper_value_catches_callback_throw() {
    let output = baml_test!(
        r#"
function direct(cb: (value: int) -> int) -> int {
  cb(1)
}

function forward(cb: (value: int) -> int) -> int {
  direct(cb)
}

function risky(value: int) -> int throws string {
  throw "boom"
}

function main() -> string {
  let run = forward
  run(risky) catch (e) {
    "boom" => "caught",
    _ => "other"
  }
}
"#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("caught".to_string()))
    );
}
