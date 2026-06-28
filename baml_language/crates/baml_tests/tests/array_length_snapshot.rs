use baml_tests::{baml_test, engine::OptLevel};
use bex_engine::BexExternalValue;

#[tokio::test]
async fn length_binding_snapshots_before_mutation() {
    let source = r#"
        function bug() -> int {
            let a: int[] = [];
            let n = a.length();
            a.push(9);
            n
        }
    "#;

    let output_o1 = baml_test! {
        baml: source,
        entry: "bug",
        opt: OptLevel::One,
    };
    assert_eq!(output_o1.result, Ok(BexExternalValue::Int(0)));

    let output_o2 = baml_test! {
        baml: source,
        entry: "bug",
        opt: OptLevel::Two,
    };
    assert_eq!(output_o2.result, Ok(BexExternalValue::Int(0)));
}
