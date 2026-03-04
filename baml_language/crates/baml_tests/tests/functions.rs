//! Unified tests for function calls, parameters, and returns.
//!
//! Merges bytecode compilation checks (insta snapshots) with VM execution checks
//! (BexExternalValue assertions) into a single test per scenario.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn return_literal_int() {
    let output = baml_test!(
        "
        function main() -> int {
            42
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
        function main() -> int {
            load_const 42
            return
        }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn return_literal_bool() {
    let output = baml_test!(
        "
        function main() -> bool {
            true
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
        function main() -> bool {
            load_const true
            return
        }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn return_literal_string() {
    let output = baml_test!(
        r#"
        function main() -> string {
            "hello"
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
        function main() -> string {
            load_const "hello"
            return
        }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello".to_string()))
    );
}

#[tokio::test]
async fn return_function_call() {
    let output = baml_test!(
        "
        function one() -> int {
            1
        }

        function main() -> int {
            one()
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
        function main() -> int {
            call one
            return
        }

        function one() -> int {
            load_const 1
            return
        }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn call_function_assign_to_variable() {
    let output = baml_test!(
        "
        function two() -> int {
            2
        }

        function main() -> int {
            let a = two();
            a + 1
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
        function main() -> int {
            call two
            load_const 1
            bin_op +
            return
        }

        function two() -> int {
            load_const 2
            return
        }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn mutable_variables() {
    let output = baml_test!(
        "
        function main() -> int {
            let y = 3;
            y = 5;
            y
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
        function main() -> int {
            load_const 3
            store_var y
            load_const 5
            store_var y
            load_var y
            return
        }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn call_with_arguments() {
    let output = baml_test!(
        "
        function one_of(a: int, b: int) -> int {
            a
        }

        function main() -> int {
            let v = one_of(1, 2);
            v
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
        function main() -> int {
            load_const 1
            load_const 2
            call one_of
            return
        }

        function one_of(a: int, b: int) -> int {
            load_var a
            return
        }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn unused_variable_does_not_affect_result() {
    let output = baml_test!(
        r#"
        function get_greeting() -> string {
            "Hello"
        }

        function main() -> string {
            let greeting = get_greeting();
            let name = "World";
            greeting
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
        function get_greeting() -> string {
            load_const "Hello"
            return
        }

        function main() -> string {
            call get_greeting
            load_const "World"
            store_var name
            return
        }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello".to_string()))
    );
}

#[tokio::test]
async fn early_return() {
    let output = baml_test! {
        baml: r#"
            function early_return(x: int) -> int {
                if (x == 42) { return 1; }
                x + 5
            }
        "#,
        entry: "early_return",
        args: { "x" => BexExternalValue::Int(42) },
    };

    insta::assert_snapshot!(output.bytecode, @r"
        function early_return(x: int) -> int {
            load_var x
            load_const 42
            cmp_op ==
            pop_jump_if_false L0
            jump L1

          L0:
            load_var x
            load_const 5
            bin_op +
            return

          L1:
            load_const 1
            return
        }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn early_return_from_nested_scopes() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let a = 1;

            if (a == 0) { return 0; }

            {
                let b = 1;
                if (a != b) {
                    return 0;
                }
            }

            {
                let c = 2;
                let b = 3;
                while (b != c) {
                    if (true) {
                        return 0;
                    }
                }
            }

            7
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
        function main() -> int {
            load_const 1
            load_const 0
            cmp_op ==
            pop_jump_if_false L0
            jump L5

          L0:
            load_const 1
            load_const 1
            cmp_op !=
            pop_jump_if_false L1
            jump L4

          L1:
            load_const 3
            load_const 2
            cmp_op !=
            pop_jump_if_false L2
            jump L3

          L2:
            load_const 7
            return

          L3:
            load_const true
            pop_jump_if_false L1
            load_const 0
            return

          L4:
            load_const 0
            return

          L5:
            load_const 0
            return
        }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn recursion() {
    let output = baml_test!(
        r#"
        function fib(n: int) -> int {
            if (n <= 1) {
                n
            } else {
                fib(n - 1) + fib(n - 2)
            }
        }

        function main() -> int {
            fib(3)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
        function fib(n: int) -> int {
            load_var n
            load_const 1
            cmp_op <=
            pop_jump_if_false L0
            jump L1

          L0:
            load_var n
            load_const 1
            bin_op -
            call fib
            store_var _5
            load_var n
            load_const 2
            bin_op -
            call fib
            store_var _10
            load_var _5
            load_var _10
            bin_op +
            jump L2

          L1:
            load_var n

          L2:
            return
        }

        function main() -> int {
            load_const 3
            call fib
            return
        }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn function_as_value() {
    let output = baml_test!(
        r#"
        function add(a: int, b: int) -> int {
            a + b
        }

        function call_twice(f: (int, int) -> int, x: int, y: int) -> int {
            f(x, y) + f(x, y)
        }

        function main() -> int {
            let f = add;
            call_twice(f, 20, 1)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function add(a: int, b: int) -> int {
        load_var a
        load_var b
        bin_op +
        return
    }

    function call_twice(f: (int, int) -> int, x: int, y: int) -> int {
        load_var x
        load_var y
        load_var f
        call_indirect
        store_var _4
        load_var x
        load_var y
        load_var f
        call_indirect
        store_var _8
        load_var _4
        load_var _8
        bin_op +
        return
    }

    function main() -> int {
        load_global add
        load_const 20
        load_const 1
        call call_twice
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}
