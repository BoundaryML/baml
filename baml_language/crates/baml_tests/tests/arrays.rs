//! Unified tests for array construction and methods.

use baml_tests::baml_test;
use baml_type::Ty;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn array_literal() {
    let output = baml_test!(
        "
        function main() -> int[] {
            [1, 2, 3]
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int[] {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(1),
                BexExternalValue::Int(2),
                BexExternalValue::Int(3),
            ],
        })
    );
}

#[tokio::test]
async fn array_assign_to_variable() {
    let output = baml_test!(
        "
        function main() -> int[] {
            let a = [1, 2, 3];
            a
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int[] {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(1),
                BexExternalValue::Int(2),
                BexExternalValue::Int(3),
            ],
        })
    );
}

#[tokio::test]
#[ignore = "compiler2: array method dispatch via indirect call fails at runtime (TypeError: expected Map, got Array)"]
async fn array_push() {
    let output = baml_test!(
        "
        function main() -> int[] {
            let a: int[] = [1, 2, 3];
            a.push(4);
            a
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int[] {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        store_var a
        load_const 4
        load_var a
        load_var _5
        load_map_element
        call_indirect
        pop 1
        load_var a
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(1),
                BexExternalValue::Int(2),
                BexExternalValue::Int(3),
                BexExternalValue::Int(4),
            ],
        })
    );
}
