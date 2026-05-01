//! Unified tests for error handling and stack traces.

use baml_tests::baml_test;

#[tokio::test]
async fn error_stack_trace() {
    // Call chain: main -> one -> two -> three, where three throws
    let output = baml_test!(
        r#"
function three() -> int {
  throw "boom"
}

function two() -> int {
  three()
}

function one() -> int {
  two()
}

function main() -> int {
  one()
}
"#
    );

    let err = output.result.unwrap_err();
    insta::assert_snapshot!(err, @r#"
    Traceback (most recent call last):
      File "test.baml", line 15, in user.main
      File "test.baml", line 11, in user.one
      File "test.baml", line 7, in user.two
      File "test.baml", line 3, in user.three
    uncaught throw: String("boom")
    "#);
}
