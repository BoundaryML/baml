use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn let_and_const_bindings_execute_the_same() {
    let source = r#"
        function WithLet() -> int {
            let sum = 0;
            for (let i = 0; i < 4; i += 1) {
                sum += i;
            }
            sum
        }

        function WithConst() -> int {
            const sum = 0;
            for (const i = 0; i < 4; i += 1) {
                sum += i;
            }
            sum
        }
    "#;

    let with_let = baml_test! {
        baml: source,
        entry: "WithLet",
    };
    let with_const = baml_test! {
        baml: source,
        entry: "WithConst",
    };

    assert_eq!(with_let.result, Ok(BexExternalValue::Int(6)));
    assert_eq!(with_const.result, Ok(BexExternalValue::Int(6)));
}
