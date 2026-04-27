//! Runtime regressions for lexical block scope and local shadowing.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn lexical_scoping_runtime_regressions() {
    let output = baml_test!(
        "
        function main() -> int {
            repeated_underscore()
            + same_scope_shadow() * 10
            + outer_restored() * 100
            + initializer_uses_previous() * 1000
            + shadow_param(4) * 10000
            + for_loop_restores_outer() * 100000
            + nested_outer_restored() * 1000000
            + capture_before_after_shadow() * 10000000
        }

        function repeated_underscore() -> int {
            let _ = 1
            let _ = 2
            _
        }

        function same_scope_shadow() -> int {
            let x = 1
            let x = 2
            x
        }

        function outer_restored() -> int {
            let x = 1
            {
                let x = 2
            }
            x
        }

        function initializer_uses_previous() -> int {
            let x = 1
            let x = x + 1
            x
        }

        function shadow_param(x: int) -> int {
            let x = x + 1
            x
        }

        function for_loop_restores_outer() -> int {
            let x = 1
            for (let x in [2, 3]) {
                x
            }
            x
        }

        function nested_outer_restored() -> int {
            let x = 1
            {
                let x = 2
                {
                    let x = 3
                    x
                }
                x
            }
            x
        }

        function capture_before_after_shadow() -> int {
            let x = 1
            let g = () -> int { x }
            let x = 2
            let f = () -> int { x }
            g() * 10 + f()
        }
    "
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(121_152_122)));
}

#[tokio::test]
async fn declared_type_and_for_underscore_restore_outer_binding() {
    let string_output = baml_test!(
        r#"
        function main() -> string {
            let x: string = "outer"
            {
                let x: int = 1
            }
            x
        }
    "#
    );

    assert_eq!(
        string_output.result,
        Ok(BexExternalValue::String("outer".to_string()))
    );

    let underscore_output = baml_test!(
        "
        function main() -> int {
            let _ = 1
            for (let _ in [2, 3]) {
                _
            }
            _
        }
    "
    );

    assert_eq!(underscore_output.result, Ok(BexExternalValue::Int(1)));
}
