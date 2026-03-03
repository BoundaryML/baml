//! Unified tests for assert statements.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn assert_ok() {
    let output = baml_test!(
        "
        function main() -> int {
            assert 2 + 2 == 4;
            3
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 2
        load_const 2
        bin_op +
        load_const 4
        cmp_op ==
        assert
        load_const 3
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn assert_not_ok() {
    let output = baml_test!(
        "
        function main() -> int {
            assert 3 == 1;
            2
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 3
        load_const 1
        cmp_op ==
        assert
        load_const 2
        return
    }
    ");

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::VmError(
            bex_vm::errors::VmError::RuntimeError(bex_vm::errors::RuntimeError::AssertionError)
        ))
    );
}
