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
        call user.one
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
        call user.two
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
        call user.one_of
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
        call user.get_greeting
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
        add_int
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
            jump L6

          L3:
            load_const true
            pop_jump_if_false L1
            load_const 0
            jump L6

          L4:
            load_const 0
            jump L6

          L5:
            load_const 0

          L6:
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
        call user.fib
        store_var _3
        load_var n
        load_const 2
        bin_op -
        call user.fib
        store_var _5
        load_var _3
        load_var _5
        add_int
        jump L2

      L1:
        load_var n

      L2:
        return
    }

    function main() -> int {
        load_const 3
        call user.fib
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

    function call_twice(f: (int, int) -> int throws never, x: int, y: int) -> int {
        load_var x
        load_var y
        load_var f
        call_indirect
        store_var _4
        load_var x
        load_var y
        load_var f
        call_indirect
        store_var _5
        load_var _4
        load_var _5
        bin_op +
        return
    }

    function main() -> int {
        load_global user.add
        load_const 20
        load_const 1
        call user.call_twice
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

/// TS-style instantiation expression: bind type args at the value site.
///
/// `let cb = identity<string>;` produces an
/// `Object::InstantiatedFunction` with `bound_type_args = [string]`.
/// At call time the VM seeds `frame.type_args = [string]` so any
/// `reflect.type_of<T>()` (or other `TypeArgRef(N)` use) inside
/// `identity` resolves correctly.
#[tokio::test]
async fn instantiation_expression_runs() {
    let output = baml_test!(
        "
        function identity<T>(x: T) -> T { x }
        function main() -> string {
            let cb = identity<string>;
            cb(\"hi\")
        }
    "
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hi".to_string()))
    );
}

/// Instantiation applied to a parenthesized generic lambda.
///
/// The parser produces a uniform `EXPR_WITH_TYPE_ARGS` wrapper around the
/// `PAREN_EXPR` containing the lambda, so the standalone-value branch of
/// AST lowering wraps the lambda expression in `Expr::Instantiation`.
/// MIR currently falls back to lowering the base expression directly
/// when the base isn't a resolvable free-function `ItemRef` (a lambda
/// isn't), so the type args are dropped at runtime — but generics in
/// BAML are largely erased at the value level, so the call still
/// returns the right result.
#[tokio::test]
async fn parenthesized_lambda_instantiation_runs() {
    let output = baml_test!(
        "
        function main() -> string {
            let cb = (<T>(x: T) -> { x })<string>;
            cb(\"hi\")
        }
    "
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hi".to_string()))
    );
}

/// Instantiation as a function-call argument.  `g(f<string>)` lowers the
/// inner `f<string>` to an `InstantiatedFunction` value that `g` receives
/// and can invoke.
#[tokio::test]
async fn instantiation_as_call_argument_runs() {
    let output = baml_test!(
        "
        function identity<T>(x: T) -> T { x }
        function apply(cb: (x: string) -> string) -> string {
            cb(\"called\")
        }
        function main() -> string {
            apply(identity<string>)
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("called".to_string()))
    );
}

/// Instantiation in return position: `return f<string>;`.  The returned
/// value is invokable by the caller.
#[tokio::test]
async fn instantiation_in_return_position_runs() {
    let output = baml_test!(
        "
        function identity<T>(x: T) -> T { x }
        function get_callback() -> (x: string) -> string {
            return identity<string>;
        }
        function main() -> string {
            let cb = get_callback();
            cb(\"returned\")
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("returned".to_string()))
    );
}

/// Instantiation inside an array literal.  Stores two instantiated function
/// values and invokes the first one.
#[tokio::test]
async fn instantiation_in_array_runs() {
    let output = baml_test!(
        "
        function identity<T>(x: T) -> T { x }
        function main() -> string {
            let fns = [identity<string>, identity<string>];
            fns[0](\"arr\")
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("arr".to_string()))
    );
}

/// Multi-arg instantiation.  `pair<int, string>` binds both `A` and `B`
/// into the function's signature; calling it then returns the right shape.
#[tokio::test]
async fn instantiation_with_multiple_type_args_runs() {
    let output = baml_test!(
        "
        function pair<A, B>(a: A, b: B) -> string { \"ok\" }
        function main() -> string {
            let p = pair<int, string>;
            p(1, \"two\")
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("ok".to_string()))
    );
}

/// Calling an instantiated callback twice in a row exercises the
/// no-shared-state guarantee — `bound_type_args` shouldn't be consumed by
/// the first call.
#[tokio::test]
async fn instantiated_callback_can_be_invoked_twice() {
    let output = baml_test!(
        "
        function identity<T>(x: T) -> T { x }
        function main() -> string {
            let cb = identity<string>;
            cb(\"once\")
            cb(\"twice\")
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("twice".to_string()))
    );
}

// Wrong-arity diagnostic is exercised at the TIR layer; see
// `instantiation_expression_wrong_arity_errors` in
// `compiler2_tir::explicit_type_args`. `baml_test!` panics on TIR-level
// compile errors, so it isn't a great fit for asserting on them.

/// Regression: assigning an instantiation expression into an indexed
/// slot routes through `walk_projection_store`, which delegates to
/// `walk_rvalue_pull` for the RHS.  Before the fix, that path
/// `unreachable!`d on `Rvalue::MakeInstantiatedFunction` instead of
/// dispatching like the direct emit path.
#[tokio::test]
async fn instantiation_assigned_to_array_index_does_not_panic() {
    let output = baml_test!(
        "
        function identity<T>(x: T) -> T { x }
        function main() -> string {
            let arr: ((x: string) -> string)[] = [identity<string>];
            arr[0] = identity<string>;
            arr[0](\"slot\")
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("slot".to_string()))
    );
}
