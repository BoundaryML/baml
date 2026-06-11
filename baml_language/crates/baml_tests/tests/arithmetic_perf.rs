use baml_tests::engine::{OptLevel, run_test};
use bex_external_types::BexExternalValue;
use indexmap::IndexMap;

#[tokio::test]
async fn arithmetic_loop_uses_int_local_update_superinstructions() {
    let output = run_test(
        r#"
        function work(lo: int, hi: int) -> int {
          let acc = 0;
          for (let i = lo; i < hi; i += 1) {
            let m = i % 65536;
            acc += (m * m) % 1000003;
          }
          acc
        }

        function main() -> int {
          work(0, 4)
        }
        "#,
        "main",
        IndexMap::new(),
        OptLevel::One,
    )
    .await;

    assert_eq!(output.result, Ok(BexExternalValue::Int(14)));
    assert!(
        output.bytecode.contains("add_int_store_var"),
        "expected acc += rhs to use add_int_store_var:\n{}",
        output.bytecode
    );
    assert!(
        output.bytecode.contains("add_int_small_store_var"),
        "expected i += 1 to use add_int_small_store_var:\n{}",
        output.bytecode
    );
}
