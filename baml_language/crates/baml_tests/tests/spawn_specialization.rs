//! Spawn bytecode specialization regressions.
//!
//! Basic spawn/await execution cases live in `baml_src/ns_spawn_basic` after
//! the test-speedup refactor. These stay as Rust tests because they assert on
//! emitted bytecode.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn non_spawn_captured_int_arithmetic_keeps_specialized_op() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let counter = 0;
            let bump = () -> int {
                counter += 1;
                counter
            };
            let _ = bump();
            counter + 1
        }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_var ?1
        make_cell
        store_var ?1
        load_const 0
        store_deref ?1
        load_var counter
        make_closure .<lambda(main, 0)>, 1
        call_indirect
        pop 1
        load_deref ?1
        load_const 1
        add_int
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn captured_int_arithmetic_uses_generic_binop() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let value = 1;
            let f = spawn { value };
            let _ = await f;
            value + 1
        }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_var ?1
        make_cell
        store_var ?1
        load_const 1
        store_deref ?1
        load_var value
        make_closure .<lambda(main, 0)>, 1
        load_const null
        load_const null
        load_type int
        load_type never
        spawn
        store_var _4
        load_var _4
        await
        pop 1
        load_deref ?1
        load_const 1
        bin_op +
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn spawned_closure_capture_marks_transitive_cells() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let counter = 1;
            let bump = () -> int { counter };
            let f = spawn { bump() };
            let _ = await f;
            counter + 1
        }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_var ?1
        make_cell
        store_var ?1
        load_var ?2
        make_cell
        store_var ?2
        load_const 1
        store_deref ?1
        load_var counter
        make_closure .<lambda(main, 0)>, 1
        store_deref ?2
        load_var bump
        make_closure .<lambda(main, 1)>, 1
        load_const null
        load_const null
        load_type int
        load_type never
        spawn
        store_var _5
        load_var _5
        await
        pop 1
        load_deref ?1
        load_const 1
        bin_op +
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn captured_float_array_element_arithmetic_uses_generic_binop() {
    let output = baml_test!(
        r#"
        function main() -> float {
            let values: float[] = [1.0];
            let f = spawn { values.length() };
            let _ = await f;
            values[0] + 1.0
        }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> float {
        load_var ?1
        make_cell
        store_var ?1
        load_const 1.0
        load_type float
        alloc_array 1
        store_deref ?1
        load_var values
        make_closure .<lambda(main, 0)>, 1
        load_const null
        load_const null
        load_type int
        load_type never
        spawn
        store_var _4
        load_var _4
        await
        pop 1
        load_deref ?1
        load_const 0
        load_array_element
        load_const 1.0
        bin_op +
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Float(2.0)));
}
