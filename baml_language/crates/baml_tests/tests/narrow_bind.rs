use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn typed_pattern_tests_and_binds_the_same_value() {
    let output = baml_test!(
        r#"
class Foo { field: int }

function pick(x: Foo | int) -> int {
  match (x) {
    let foo: Foo => foo.field,
    let n: int => n,
  }
}

function main() -> int {
  pick(Foo { field: 1 }) + pick(2)
}
"#
    );

    assert!(
        output.bytecode.contains("narrow_bind"),
        "{}",
        output.bytecode
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}
