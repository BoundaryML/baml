//! Runtime regressions for lexical block scope and local shadowing.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

const LEXICAL_SCOPE_RUNTIME_REGRESSIONS: &str = r#"
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

// Rule 1: a `let` declared inside a while body must not leak past the
// loop. After the loop, `x` resolves to the outer binding.
function rule1_while_no_leakage() -> int {
    let x = 1
    let once = true
    while (once) {
        let x = 99
        once = false
    }
    x
}

// Observe the inner shadow's value to rule out optimizer-induced false
// positives in `rule1_while_no_leakage`. If outer x and inner x were
// conflated (a shadowing bug), `observed` would still see 99 but the
// return would be `99 + 99 = 198`, not `1 + 99 = 100`.
function rule1_while_observed_inner_shadow() -> int {
    let x = 1
    let once = true
    let observed = 0
    while (once) {
        let x = 99
        observed = observed + x
        once = false
    }
    x + observed
}

// Rule 2: outer-binding mutation in an inner block escapes.
function rule2_block_outer_mutation_escapes() -> int {
    let x = 1
    {
        x = 2
    }
    x
}

// Rule 2: outer-binding mutation in a `for` body escapes.
function rule2_for_outer_mutation_escapes() -> int {
    let x = 1
    for (let _ in [1]) {
        x = 2
    }
    x
}

// Rule 3: a block-local shadow's mutation must NOT escape. Without
// binding-identity-keyed assignment tracking, the inner `x = 3` would
// conflate with an outer `x` mutation and propagate.
function rule3_block_shadow_then_assign_inner_does_not_escape() -> int {
    let x = 1
    {
        let x = 2
        x = 3
    }
    x
}

// Composite test: a pre-shadow outer mutation escapes; a post-shadow
// inner mutation does not. A name-keyed assignment tracker cannot
// distinguish these — the binding-identity-keyed tracker can.
function rule2_pre_shadow_mutation_escapes_post_shadow_does_not() -> int {
    let x = 1
    {
        x = 2
        let x = 3
        x = 4
    }
    x
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
    assert_lexical_scope_result("same_scope_shadow()", 2).await;
    assert_lexical_scope_result("outer_restored()", 1).await;
    assert_lexical_scope_result("initializer_uses_previous()", 2).await;
    assert_lexical_scope_result("shadow_param(4)", 5).await;
    assert_lexical_scope_result("for_loop_restores_outer()", 1).await;
    assert_lexical_scope_result("nested_outer_restored()", 1).await;
    assert_lexical_scope_result("capture_before_after_shadow()", 12).await;
    assert_lexical_scope_result("rule1_while_no_leakage()", 1).await;
    assert_lexical_scope_result("rule1_while_observed_inner_shadow()", 100).await;
    assert_lexical_scope_result("rule2_block_outer_mutation_escapes()", 2).await;
    assert_lexical_scope_result("rule2_for_outer_mutation_escapes()", 2).await;
    assert_lexical_scope_result("rule3_block_shadow_then_assign_inner_does_not_escape()", 1).await;
    assert_lexical_scope_result(
        "rule2_pre_shadow_mutation_escapes_post_shadow_does_not()",
        2,
    )
    .await;
}

#[tokio::test]
async fn declared_type_restored_across_scope() {
    // Verifies that a typed outer binding is restored after an inner scope
    // shadows it with a different declared type. The inner `let x: int = 1`
    // exists only inside the block; after the block, `x` resolves back to
    // the outer `string` binding.
    //
    // (The previous `let _ = ...; _` half of this test was removed: under
    // the new patterns backend (PR BoundaryML/baml#3417, "implement new
    // patterns backend without parser support"), `_` is canonicalized to
    // a wildcard at AST construction and is no longer a referenceable name.)
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
                let s: string => {
                    let f = () -> string { s }
                    f()
                }
            }
        }

        function capture_catch_clause_binding() -> string {
            throw_string() catch (e) {
                string => {
                    let f = () -> string { e }
                    f()
                }
            }
        }

        function capture_catch_arm_binding() -> string {
            throw_string() catch (e) {
                let s: string => {
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
                let x: int => x
            }
            x
        }

        function match_later_arm_uses_outer() -> int {
            let x = 20
            let v: string = "value"
            match (v) {
                let x: "specific" => 0,
                _ => x
            }
        }

        function catch_base_uses_outer_lambda() -> int {
            let e = () -> string { "base" }
            let caught = throw_string(e()) catch (e) {
                string => e
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

#[tokio::test]
async fn multi_clause_catch_uses_clause_local_binding() {
    let output = baml_test!(
        r#"
        function fail() -> int {
            throw 7
        }

        function main() -> int {
            fail() catch (first) {
                string => 1
            } catch (second) {
                int => second
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}
