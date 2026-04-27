//! Runtime regressions for lexical block scope and local shadowing.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

const LEXICAL_SCOPE_RUNTIME_REGRESSIONS: &str = r#"
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
"#;

async fn assert_lexical_scope_result(entry: &str, expected: i64) {
    let source = format!(
        r#"
        {LEXICAL_SCOPE_RUNTIME_REGRESSIONS}

        function main() -> int {{
            {entry}
        }}
        "#
    );
    let output = baml_test!(&source);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Int(expected)),
        "unexpected lexical scoping result for `{entry}`"
    );
}

#[tokio::test]
async fn lexical_scoping_runtime_regressions() {
    assert_lexical_scope_result("repeated_underscore()", 2).await;
    assert_lexical_scope_result("same_scope_shadow()", 2).await;
    assert_lexical_scope_result("outer_restored()", 1).await;
    assert_lexical_scope_result("initializer_uses_previous()", 2).await;
    assert_lexical_scope_result("shadow_param(4)", 5).await;
    assert_lexical_scope_result("for_loop_restores_outer()", 1).await;
    assert_lexical_scope_result("nested_outer_restored()", 1).await;
    assert_lexical_scope_result("capture_before_after_shadow()", 12).await;
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

#[tokio::test]
async fn lambdas_capture_match_and_catch_pattern_bindings() {
    let output = baml_test!(
        r#"
        function throw_string() -> string {
            throw "caught"
        }

        function capture_match_arm() -> string {
            match ("matched") {
                s: string => {
                    let f = () -> string { s }
                    f()
                }
            }
        }

        function capture_catch_clause_binding() -> string {
            throw_string() catch (e) {
                _: string => {
                    let f = () -> string { e }
                    f()
                }
            }
        }

        function capture_catch_arm_binding() -> string {
            throw_string() catch (e) {
                s: string => {
                    let f = () -> string { s }
                    f()
                }
            }
        }

        function main() -> string {
            capture_match_arm() + ":" + capture_catch_clause_binding() + ":" + capture_catch_arm_binding()
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "matched:caught:caught".to_string()
        ))
    );
}

#[tokio::test]
async fn match_and_catch_pattern_bindings_restore_outer_locals() {
    let output = baml_test!(
        r#"
        function throw_string(s: string) -> string {
            throw s
        }

        function match_post_match_restores_outer() -> int {
            let x = 10
            let _matched = match (1) {
                x: int => x
            }
            x
        }

        function match_later_arm_uses_outer() -> int {
            let x = 20
            match ("value") {
                x: int => 0,
                _: string => x
            }
        }

        function catch_base_uses_outer_lambda() -> int {
            let e = () -> string { "base" }
            let caught = throw_string(e()) catch (e) {
                _: string => e
            }
            match (caught) {
                "base" => 3,
                _ => 0
            }
        }

        function main() -> int {
            match_post_match_restores_outer()
                + match_later_arm_uses_outer() * 100
                + catch_base_uses_outer_lambda() * 10000
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(32_010)));
}
