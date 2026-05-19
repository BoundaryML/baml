//! CR-1 regression: spawn name expression must not consume the body's
//! brace as an object-literal constructor.
//!
//! Before the fix, `parse_spawn_expr` parsed the optional name via
//! `parse_expr_bp(1)` which allowed the postfix `{ ... }` to be treated
//! as a struct literal. So `spawn nm { ... }` where the body content
//! looked like fields (`<word>:`) consumed the body braces, leaving the
//! parser with nothing for the body and emitting "Expected '{' after
//! spawn". After the fix, `parse_spawn_expr` sets
//! `suppress_object_literal_depth` so the body brace stays available.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Body contains a class constructor whose `<word> {` shape would have
/// tripped the broken parser when the spawn name is a bare identifier.
#[tokio::test]
async fn spawn_name_then_struct_body_parses_correctly() {
    let output = baml_test!(
        r#"
        class B { x int }
        function main() -> int {
            let nm = "task";
            let f = spawn nm { B { x: 7 } };
            (await f).x
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

/// Body's tail is a map literal `{ "y": 1 }` whose first content tokens
/// are `<string> :` — the brace-content-looks-like-fields signature
/// that would have tripped the broken parser.
#[tokio::test]
async fn spawn_name_then_map_literal_body() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let nm = "task";
            let f = spawn nm { { "y": 1 }["y"] };
            await f
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}
