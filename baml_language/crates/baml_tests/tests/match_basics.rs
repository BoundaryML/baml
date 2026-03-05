//! Unified tests for match basics: catch-all, literals (int/string/bool/float/null/negative),
//! unions, expressions, nested, edge cases, optional with null, complex scrutinee expressions,
//! and exhaustive bool.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// Basic Catch-All Tests
// ============================================================================

#[tokio::test]
async fn match_catch_all_underscore() {
    let output = baml_test!(
        "
        function main() -> int {
            match (42) {
                _ => 100
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 100
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

#[tokio::test]
async fn match_catch_all_named_binding() {
    let output = baml_test!(
        "
        function main() -> int {
            match (42) {
                x => x + 1
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 42
        load_const 1
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(43)));
}

#[tokio::test]
async fn match_catch_all_with_variable() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 42;
            match (x) {
                y => y + 1
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 42
        load_const 1
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(43)));
}

// ============================================================================
// Literal Pattern Tests
// ============================================================================

#[tokio::test]
async fn match_literal_int_first_arm() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 1;
            match (x) {
                1 => 100,
                2 => 200,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const 2
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const 0
        jump L4

      L2:
        load_const 200
        jump L4

      L3:
        load_const 100

      L4:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

#[tokio::test]
async fn match_literal_int_second_arm() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 2;
            match (x) {
                1 => 100,
                2 => 200,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 2
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const 2
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const 0
        jump L4

      L2:
        load_const 200
        jump L4

      L3:
        load_const 100

      L4:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(200)));
}

#[tokio::test]
async fn match_literal_int_fallback() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 999;
            match (x) {
                1 => 100,
                2 => 200,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 999
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const 2
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const 0
        jump L4

      L2:
        load_const 200
        jump L4

      L3:
        load_const 100

      L4:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn match_literal_int_single_arm() {
    let output = baml_test!(
        "
        function main() -> int {
            match (1) {
                1 => 100,
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        pop 1
        load_const 100
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

#[tokio::test]
async fn match_literal_null() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let x = null;
            match (x) {
                null => "was null",
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "was null"
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("was null".to_string()))
    );
}

#[tokio::test]
async fn match_literal_bool_true() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let b = true;
            match (b) {
                true => "yes",
                false => "no"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const true
        load_const true
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "no"
        jump L2

      L1:
        load_const "yes"

      L2:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("yes".to_string()))
    );
}

#[tokio::test]
async fn match_literal_bool_false() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let b = false;
            match (b) {
                true => "yes",
                false => "no"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const false
        load_const true
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "no"
        jump L2

      L1:
        load_const "yes"

      L2:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("no".to_string()))
    );
}

#[tokio::test]
async fn match_literal_bool_exhaustive_constant() {
    // Constant propagation: scrutinee true is inlined, should optimize
    let output = baml_test!(
        r#"
        function main() -> string {
            match (true) {
                true => "yes",
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "yes"
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("yes".to_string()))
    );
}

// ============================================================================
// Union Literal Pattern Tests
// ============================================================================

#[tokio::test]
async fn match_union_literal_first() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let code = 200;
            match (code) {
                200 | 201 => "success",
                400 | 404 => "client error",
                _ => "other"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 200
        copy 0
        load_const 400
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L4

      L0:
        copy 0
        load_const 400
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 200
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L5

      L1:
        copy 0
        load_const 201
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L5

      L2:
        copy 0
        load_const 404
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const "other"
        jump L6

      L4:
        load_const "client error"
        jump L6

      L5:
        load_const "success"

      L6:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("success".to_string()))
    );
}

#[tokio::test]
async fn match_union_literal_second() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let code = 201;
            match (code) {
                200 | 201 => "success",
                400 | 404 => "client error",
                _ => "other"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 201
        copy 0
        load_const 400
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L4

      L0:
        copy 0
        load_const 400
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const 200
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L5

      L1:
        copy 0
        load_const 201
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L5

      L2:
        copy 0
        load_const 404
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L4

      L3:
        pop 1
        load_const "other"
        jump L6

      L4:
        load_const "client error"
        jump L6

      L5:
        load_const "success"

      L6:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("success".to_string()))
    );
}

#[tokio::test]
async fn match_union_large() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let code = 204;
            match (code) {
                200 | 201 | 202 | 204 => "success",
                400 | 401 | 403 | 404 => "client error",
                500 | 501 | 502 | 503 => "server error",
                _ => "other"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 204
        copy 0
        load_const 403
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L13

      L0:
        copy 0
        load_const 403
        cmp_op <
        pop_jump_if_false L6
        copy 0
        load_const 204
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L14

      L1:
        copy 0
        load_const 204
        cmp_op <
        pop_jump_if_false L4
        copy 0
        load_const 201
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L14

      L2:
        copy 0
        load_const 201
        cmp_op <
        pop_jump_if_false L3
        copy 0
        load_const 200
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L14

      L3:
        copy 0
        load_const 202
        cmp_op ==
        pop_jump_if_false L4
        pop 1
        jump L14

      L4:
        copy 0
        load_const 400
        cmp_op ==
        pop_jump_if_false L5
        pop 1
        jump L13

      L5:
        copy 0
        load_const 401
        cmp_op ==
        pop_jump_if_false L6
        pop 1
        jump L13

      L6:
        copy 0
        load_const 501
        cmp_op ==
        pop_jump_if_false L7
        pop 1
        jump L12

      L7:
        copy 0
        load_const 501
        cmp_op <
        pop_jump_if_false L9
        copy 0
        load_const 404
        cmp_op ==
        pop_jump_if_false L8
        pop 1
        jump L13

      L8:
        copy 0
        load_const 500
        cmp_op ==
        pop_jump_if_false L9
        pop 1
        jump L12

      L9:
        copy 0
        load_const 502
        cmp_op ==
        pop_jump_if_false L10
        pop 1
        jump L12

      L10:
        copy 0
        load_const 503
        cmp_op ==
        pop_jump_if_false L11
        pop 1
        jump L12

      L11:
        pop 1
        load_const "other"
        jump L15

      L12:
        load_const "server error"
        jump L15

      L13:
        load_const "client error"
        jump L15

      L14:
        load_const "success"

      L15:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("success".to_string()))
    );
}

#[tokio::test]
async fn match_union_client_error() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let code = 404;
            match (code) {
                200 | 201 | 202 | 204 => "success",
                400 | 401 | 403 | 404 => "client error",
                500 | 501 | 502 | 503 => "server error",
                _ => "other"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 404
        copy 0
        load_const 403
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L13

      L0:
        copy 0
        load_const 403
        cmp_op <
        pop_jump_if_false L6
        copy 0
        load_const 204
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L14

      L1:
        copy 0
        load_const 204
        cmp_op <
        pop_jump_if_false L4
        copy 0
        load_const 201
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L14

      L2:
        copy 0
        load_const 201
        cmp_op <
        pop_jump_if_false L3
        copy 0
        load_const 200
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L14

      L3:
        copy 0
        load_const 202
        cmp_op ==
        pop_jump_if_false L4
        pop 1
        jump L14

      L4:
        copy 0
        load_const 400
        cmp_op ==
        pop_jump_if_false L5
        pop 1
        jump L13

      L5:
        copy 0
        load_const 401
        cmp_op ==
        pop_jump_if_false L6
        pop 1
        jump L13

      L6:
        copy 0
        load_const 501
        cmp_op ==
        pop_jump_if_false L7
        pop 1
        jump L12

      L7:
        copy 0
        load_const 501
        cmp_op <
        pop_jump_if_false L9
        copy 0
        load_const 404
        cmp_op ==
        pop_jump_if_false L8
        pop 1
        jump L13

      L8:
        copy 0
        load_const 500
        cmp_op ==
        pop_jump_if_false L9
        pop 1
        jump L12

      L9:
        copy 0
        load_const 502
        cmp_op ==
        pop_jump_if_false L10
        pop 1
        jump L12

      L10:
        copy 0
        load_const 503
        cmp_op ==
        pop_jump_if_false L11
        pop 1
        jump L12

      L11:
        pop 1
        load_const "other"
        jump L15

      L12:
        load_const "server error"
        jump L15

      L13:
        load_const "client error"
        jump L15

      L14:
        load_const "success"

      L15:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("client error".to_string()))
    );
}

#[tokio::test]
async fn match_union_with_duplicates() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let x = 1;
            match (x) {
                1 | 1 | 2 => "one or two",
                3 | 3 => "three",
                _ => "other"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L4

      L0:
        copy 0
        load_const 2
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L4

      L1:
        copy 0
        load_const 3
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L3

      L2:
        pop 1
        load_const "other"
        jump L5

      L3:
        load_const "three"
        jump L5

      L4:
        load_const "one or two"

      L5:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one or two".to_string()))
    );
}

// ============================================================================
// Expression Context Tests
// ============================================================================

#[tokio::test]
async fn match_as_expression_in_arithmetic() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 2;
            match (x) {
                1 => 10,
                2 => 20,
                _ => 0
            } + 1
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 2
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const 2
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const 0
        jump L4

      L2:
        load_const 20
        jump L4

      L3:
        load_const 10

      L4:
        load_const 1
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(21)));
}

#[tokio::test]
async fn match_nested() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let outer = 1;
            let inner = 2;
            match (outer) {
                1 => match (inner) {
                    1 => 11,
                    2 => 12,
                    _ => 10
                },
                2 => 20,
                _ => 0
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const 2
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const 0
        jump L8

      L2:
        load_const 20
        jump L8

      L3:
        load_const 2
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L4
        pop 1
        jump L7

      L4:
        copy 0
        load_const 2
        cmp_op ==
        pop_jump_if_false L5
        pop 1
        jump L6

      L5:
        pop 1
        load_const 10
        jump L8

      L6:
        load_const 12
        jump L8

      L7:
        load_const 11

      L8:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

// ============================================================================
// String Literal Tests (should NOT use jump table - use if-else chain)
// ============================================================================

#[tokio::test]
async fn match_string_literal_first_arm() {
    let output = baml_test!(
        r#"
        function classify(s: string) -> int {
            match (s) {
                "hello" => 100,
                "world" => 200,
                _ => 0
            }
        }
        function main() -> int {
            classify("hello")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(s: string) -> int {
        load_var s
        load_const "hello"
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var s
        load_const "world"
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 0
        jump L4

      L2:
        load_const 200
        jump L4

      L3:
        load_const 100

      L4:
        return
    }

    function main() -> int {
        load_const "hello"
        call classify
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

#[tokio::test]
async fn match_string_literal_second_arm() {
    let output = baml_test!(
        r#"
        function classify(s: string) -> int {
            match (s) {
                "hello" => 100,
                "world" => 200,
                _ => 0
            }
        }
        function main() -> int {
            classify("world")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(s: string) -> int {
        load_var s
        load_const "hello"
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var s
        load_const "world"
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 0
        jump L4

      L2:
        load_const 200
        jump L4

      L3:
        load_const 100

      L4:
        return
    }

    function main() -> int {
        load_const "world"
        call classify
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(200)));
}

#[tokio::test]
async fn match_string_literal_fallback() {
    let output = baml_test!(
        r#"
        function classify(s: string) -> int {
            match (s) {
                "hello" => 100,
                "world" => 200,
                _ => 0
            }
        }
        function main() -> int {
            classify("other")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(s: string) -> int {
        load_var s
        load_const "hello"
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var s
        load_const "world"
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 0
        jump L4

      L2:
        load_const 200
        jump L4

      L3:
        load_const 100

      L4:
        return
    }

    function main() -> int {
        load_const "other"
        call classify
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn match_string_four_arms() {
    // 4+ string arms should NOT use jump table (strings can't be hashed efficiently)
    let output = baml_test!(
        r#"
        function classify(s: string) -> int {
            match (s) {
                "a" => 1,
                "b" => 2,
                "c" => 3,
                "d" => 4,
                _ => 0
            }
        }
        function main() -> int {
            classify("c")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(s: string) -> int {
        load_var s
        load_const "a"
        cmp_op ==
        pop_jump_if_false L0
        jump L7

      L0:
        load_var s
        load_const "b"
        cmp_op ==
        pop_jump_if_false L1
        jump L6

      L1:
        load_var s
        load_const "c"
        cmp_op ==
        pop_jump_if_false L2
        jump L5

      L2:
        load_var s
        load_const "d"
        cmp_op ==
        pop_jump_if_false L3
        jump L4

      L3:
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

    function main() -> int {
        load_const "c"
        call classify
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn match_string_literal_with_typed_fallback() {
    let output = baml_test!(
        r#"
        function classify(s: string) -> int {
            match (s) {
                "ok" => 200,
                "error" => 500,
                _: string => 0
            }
        }
        function main() -> int {
            classify("unknown")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(s: string) -> int {
        load_var s
        load_const "ok"
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var s
        load_const "error"
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_const 0
        jump L4

      L2:
        load_const 500
        jump L4

      L3:
        load_const 200

      L4:
        return
    }

    function main() -> int {
        load_const "unknown"
        call classify
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

// ============================================================================
// Catch-All Binding with Integer Patterns
// ============================================================================

#[tokio::test]
async fn match_catch_all_binding_with_int_patterns() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 99;
            match (x) {
                0 => 0,
                1 => 1,
                2 => 2,
                3 => 3,
                other => other * 10
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 99
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 99
        load_const 10
        bin_op *
        jump L5

      L1:
        load_const 3
        jump L5

      L2:
        load_const 2
        jump L5

      L3:
        load_const 1
        jump L5

      L4:
        load_const 0

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(990)));
}

// ============================================================================
// Float Literal Tests (should NOT use jump table)
// ============================================================================

#[tokio::test]
async fn match_float_literal() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let x = 1.5;
            match (x) {
                1.0 => "one",
                1.5 => "one point five",
                2.0 => "two",
                _ => "other"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1.5
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        jump L5

      L0:
        load_const 1.5
        load_const 1.5
        cmp_op ==
        pop_jump_if_false L1
        jump L4

      L1:
        load_const 1.5
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_const "other"
        jump L6

      L3:
        load_const "two"
        jump L6

      L4:
        load_const "one point five"
        jump L6

      L5:
        load_const "one"

      L6:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one point five".to_string()))
    );
}

// ============================================================================
// Negative Literal Pattern Tests
// ============================================================================

#[tokio::test]
async fn match_negative_int_first_arm() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let x: int = -1;
            match (x) {
                -1 => "negative one",
                0 => "zero",
                1 => "one",
                _ => "other"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1
        unary_op -
        copy 0
        load_const -1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L4

      L1:
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L3

      L2:
        pop 1
        load_const "other"
        jump L6

      L3:
        load_const "one"
        jump L6

      L4:
        load_const "zero"
        jump L6

      L5:
        load_const "negative one"

      L6:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("negative one".to_string()))
    );
}

#[tokio::test]
async fn match_negative_int_fallback() {
    let output = baml_test!(
        r#"
        function main() -> string {
            match (5) {
                -1 => "negative one",
                0 => "zero",
                1 => "one",
                _ => "other"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 5
        copy 0
        load_const -1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L4

      L1:
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L3

      L2:
        pop 1
        load_const "other"
        jump L6

      L3:
        load_const "one"
        jump L6

      L4:
        load_const "zero"
        jump L6

      L5:
        load_const "negative one"

      L6:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("other".to_string()))
    );
}

#[tokio::test]
async fn match_negative_int_with_variable() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let x = -1;
            match (x) {
                -1 => "negative one",
                0 => "zero",
                _ => "other"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1
        unary_op -
        copy 0
        load_const -1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const "other"
        jump L4

      L2:
        load_const "zero"
        jump L4

      L3:
        load_const "negative one"

      L4:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("negative one".to_string()))
    );
}

#[tokio::test]
async fn match_negative_float_pattern() {
    let output = baml_test!(
        r#"
        function main() -> string {
            match (-1.5) {
                -1.5 => "negative one point five",
                0.0 => "zero",
                1.5 => "one point five",
                _ => "other"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1.5
        unary_op -
        store_var _1
        load_var _1
        load_const -1.5
        cmp_op ==
        pop_jump_if_false L0
        jump L5

      L0:
        load_var _1
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        jump L4

      L1:
        load_var _1
        load_const 1.5
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_const "other"
        jump L6

      L3:
        load_const "one point five"
        jump L6

      L4:
        load_const "zero"
        jump L6

      L5:
        load_const "negative one point five"

      L6:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "negative one point five".to_string()
        ))
    );
}

#[tokio::test]
async fn match_multiple_negative_patterns() {
    let output = baml_test!(
        r#"
        function main() -> string {
            match (-2) {
                -3 => "negative three",
                -2 => "negative two",
                -1 => "negative one",
                _ => "other"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 2
        unary_op -
        copy 0
        load_const -3
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L5

      L0:
        copy 0
        load_const -2
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L4

      L1:
        copy 0
        load_const -1
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L3

      L2:
        pop 1
        load_const "other"
        jump L6

      L3:
        load_const "negative one"
        jump L6

      L4:
        load_const "negative two"
        jump L6

      L5:
        load_const "negative three"

      L6:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("negative two".to_string()))
    );
}

#[tokio::test]
async fn match_negative_in_union_pattern() {
    let output = baml_test!(
        r#"
        function main() -> string {
            match (-1) {
                -1 | 0 | 1 => "small",
                _ => "large"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1
        unary_op -
        copy 0
        load_const -1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L3

      L1:
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L3

      L2:
        pop 1
        load_const "large"
        jump L4

      L3:
        load_const "small"

      L4:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("small".to_string()))
    );
}

// ============================================================================
// Three-Level Nested Match
// ============================================================================

#[tokio::test]
async fn match_three_levels_nested() {
    let output = baml_test!(
        r#"
        function classify(x: int, y: int, z: int) -> string {
            match (x) {
                0 => match (y) {
                    0 => match (z) {
                        0 => "all zero",
                        _ => "z nonzero"
                    },
                    _ => "y nonzero"
                },
                _ => "x nonzero"
            }
        }
        function main() -> string {
            classify(0, 0, 0)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int, y: int, z: int) -> string {
        load_var x
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L1

      L0:
        pop 1
        load_const "x nonzero"
        jump L6

      L1:
        load_var y
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L3

      L2:
        pop 1
        load_const "y nonzero"
        jump L6

      L3:
        load_var z
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L4
        pop 1
        jump L5

      L4:
        pop 1
        load_const "z nonzero"
        jump L6

      L5:
        load_const "all zero"

      L6:
        return
    }

    function main() -> string {
        load_const 0
        load_const 0
        load_const 0
        call classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("all zero".to_string()))
    );
}

#[tokio::test]
async fn match_three_levels_nested_middle() {
    let output = baml_test!(
        r#"
        function classify(x: int, y: int, z: int) -> string {
            match (x) {
                0 => match (y) {
                    0 => match (z) {
                        0 => "all zero",
                        _ => "z nonzero"
                    },
                    _ => "y nonzero"
                },
                _ => "x nonzero"
            }
        }
        function main() -> string {
            classify(0, 1, 0)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int, y: int, z: int) -> string {
        load_var x
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L1

      L0:
        pop 1
        load_const "x nonzero"
        jump L6

      L1:
        load_var y
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L3

      L2:
        pop 1
        load_const "y nonzero"
        jump L6

      L3:
        load_var z
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L4
        pop 1
        jump L5

      L4:
        pop 1
        load_const "z nonzero"
        jump L6

      L5:
        load_const "all zero"

      L6:
        return
    }

    function main() -> string {
        load_const 0
        load_const 1
        load_const 0
        call classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("y nonzero".to_string()))
    );
}

// ============================================================================
// Optional with Null Pattern
// ============================================================================

#[tokio::test]
async fn match_optional_null_pattern() {
    let output = baml_test!(
        r#"
        function process(x: int?) -> string {
            match (x) {
                null => "none",
                n: int => "some"
            }
        }
        function main() -> string {
            process(null)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const null
        call process
        return
    }

    function process(x: int?) -> string {
        load_var x
        load_const null
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "some"
        jump L2

      L1:
        load_const "none"

      L2:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("none".to_string()))
    );
}

#[tokio::test]
async fn match_optional_value_pattern() {
    let output = baml_test!(
        r#"
        function process(x: int?) -> string {
            match (x) {
                null => "none",
                n: int => "some"
            }
        }
        function main() -> string {
            process(42)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 42
        call process
        return
    }

    function process(x: int?) -> string {
        load_var x
        load_const null
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "some"
        jump L2

      L1:
        load_const "none"

      L2:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("some".to_string()))
    );
}

#[tokio::test]
async fn match_optional_with_literal_and_typed() {
    let output = baml_test!(
        r#"
        function process(x: int?) -> string {
            match (x) {
                null => "none",
                0 => "zero",
                n: int => "other"
            }
        }
        function main() -> string {
            process(0)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 0
        call process
        return
    }

    function process(x: int?) -> string {
        load_var x
        load_const null
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var x
        load_const 0
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_const "other"
        jump L4

      L2:
        load_const "zero"
        jump L4

      L3:
        load_const "none"

      L4:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("zero".to_string()))
    );
}

// ============================================================================
// Complex Scrutinee Expressions
// ============================================================================

#[tokio::test]
async fn match_arithmetic_scrutinee() {
    let output = baml_test!(
        r#"
        function classify(a: int, b: int) -> string {
            match (a + b) {
                0 => "zero",
                1 => "one",
                _ => "other"
            }
        }
        function main() -> string {
            classify(2, -1)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(a: int, b: int) -> string {
        load_var a
        load_var b
        bin_op +
        copy 0
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const "other"
        jump L4

      L2:
        load_const "one"
        jump L4

      L3:
        load_const "zero"

      L4:
        return
    }

    function main() -> string {
        load_const 2
        load_const 1
        unary_op -
        call classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one".to_string()))
    );
}

#[tokio::test]
async fn match_function_call_scrutinee() {
    let output = baml_test!(
        r#"
        function helper() -> int {
            42
        }

        function classify() -> string {
            match (helper()) {
                42 => "answer",
                _ => "other"
            }
        }
        function main() -> string {
            classify()
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify() -> string {
        call helper
        copy 0
        load_const 42
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L1

      L0:
        pop 1
        load_const "other"
        jump L2

      L1:
        load_const "answer"

      L2:
        return
    }

    function helper() -> int {
        load_const 42
        return
    }

    function main() -> string {
        call classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("answer".to_string()))
    );
}

#[tokio::test]
async fn match_computed_discriminant() {
    let output = baml_test!(
        r#"
        function get_value() -> int {
            2
        }

        function main() -> int {
            match (get_value()) {
                0 => 100,
                1 => 101,
                2 => 102,
                3 => 103,
                _ => 999
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function get_value() -> int {
        load_const 2
        return
    }

    function main() -> int {
        call get_value
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 999
        jump L5

      L1:
        load_const 103
        jump L5

      L2:
        load_const 102
        jump L5

      L3:
        load_const 101
        jump L5

      L4:
        load_const 100

      L5:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(102)));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn match_in_loop() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let sum = 0;
            let i = 0;
            while (i < 5) {
                sum = sum + match (i) {
                    0 => 10,
                    1 => 20,
                    2 => 30,
                    3 => 40,
                    _ => 50
                };
                i = i + 1;
            }
            sum
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 0
        store_var sum
        load_const 0
        store_var i

      L0:
        load_var i
        load_const 5
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        load_var sum
        return

      L2:
        load_var sum
        store_var _10
        load_var i
        jump_table [L7, L6, L5, L4], default L3

      L3:
        load_const 50
        store_var _11
        jump L8

      L4:
        load_const 40
        store_var _11
        jump L8

      L5:
        load_const 30
        store_var _11
        jump L8

      L6:
        load_const 20
        store_var _11
        jump L8

      L7:
        load_const 10
        store_var _11

      L8:
        load_var _10
        load_var _11
        bin_op +
        store_var sum
        load_var i
        load_const 1
        bin_op +
        store_var i
        jump L0
    }
    ");

    // 10 + 20 + 30 + 40 + 50 = 150
    assert_eq!(output.result, Ok(BexExternalValue::Int(150)));
}

#[tokio::test]
async fn match_dense_with_catch_all_via_call() {
    let output = baml_test!(
        r#"
        function classify(n: int) -> int {
            match (n) {
                0 => 0,
                1 => 1,
                2 => 2,
                3 => 3,
                _ => -1
            }
        }

        function main() -> int {
            classify(2)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function classify(n: int) -> int {
        load_var n
        jump_table [L4, L3, L2, L1], default L0

      L0:
        load_const 1
        unary_op -
        jump L5

      L1:
        load_const 3
        jump L5

      L2:
        load_const 2
        jump L5

      L3:
        load_const 1
        jump L5

      L4:
        load_const 0

      L5:
        return
    }

    function main() -> int {
        load_const 2
        call classify
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn match_mixed_instanceof_and_literal() {
    let output = baml_test!(
        r#"
        class Result {
            code int
        }

        function main() -> int {
            let x = Result { code: 200 };
            match (x) {
                r: Result => r.code,
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Result
        copy 0
        load_const 200
        store_field .code
        store_var x
        load_var x
        load_field .code
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(200)));
}

// ============================================================================
// Exhaustive Bool with Variable Scrutinee Tests
// ============================================================================

/// Exhaustive bool match with variable scrutinee (not constant).
#[tokio::test]
async fn match_bool_variable_exhaustive() {
    let output = baml_test! {
        baml: r#"
            function check(flag: bool) -> string {
                match (flag) {
                    true => "yes",
                    false => "no"
                }
            }
        "#,
        entry: "check",
        args: { "flag" => BexExternalValue::Bool(true) },
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function check(flag: bool) -> string {
        load_var flag
        load_const true
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "no"
        jump L2

      L1:
        load_const "yes"

      L2:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("yes".to_string()))
    );
}

// ============================================================================
// String Patterns with 4+ Arms Tests
// ============================================================================

/// String patterns with 4+ arms should still use if-else chain (not jump table).
#[tokio::test]
async fn match_string_many_arms() {
    let output = baml_test! {
        baml: r#"
            function classify(s: string) -> int {
                match (s) {
                    "alpha" => 1,
                    "beta" => 2,
                    "gamma" => 3,
                    "delta" => 4,
                    _ => 0
                }
            }
        "#,
        entry: "classify",
        args: { "s" => BexExternalValue::String("gamma".to_string()) },
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(s: string) -> int {
        load_var s
        load_const "alpha"
        cmp_op ==
        pop_jump_if_false L0
        jump L7

      L0:
        load_var s
        load_const "beta"
        cmp_op ==
        pop_jump_if_false L1
        jump L6

      L1:
        load_var s
        load_const "gamma"
        cmp_op ==
        pop_jump_if_false L2
        jump L5

      L2:
        load_var s
        load_const "delta"
        cmp_op ==
        pop_jump_if_false L3
        jump L4

      L3:
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
