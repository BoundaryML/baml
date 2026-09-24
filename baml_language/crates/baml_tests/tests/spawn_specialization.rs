//! Spawn bytecode specialization regressions.
//!
//! Basic spawn/await execution cases live in `baml_src/ns_spawn_basic` after
//! the test-speedup refactor. These stay as Rust tests because they assert on
//! emitted bytecode.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use num_bigint::BigInt;

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

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const null
        make_cell
        store_var counter
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
    ");
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
        load_const null
        make_cell
        store_var value
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
        load_const null
        make_cell
        store_var counter
        load_const 1
        store_deref ?1
        load_const null
        make_cell
        store_var bump
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
        load_const null
        make_cell
        store_var values
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

#[tokio::test]
async fn spawned_closure_can_add_captured_bigint_field() {
    let output = baml_test!(
        r#"
        class Config {
            page_budget_ms: bigint
        }

        function broken(config: Config) -> bigint {
            let pending = spawn { config.page_budget_ms + 1n };
            await pending
        }

        function main() -> bigint {
            broken(Config { page_budget_ms: 10n })
        }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(11)))
    );
}

#[tokio::test]
async fn spawn_shared_bigint_subtraction_uses_generic_binop() {
    let output = baml_test!(
        r#"
        function spawn_bigint_sub() -> bigint {
            let x = 10n;
            let f = spawn { x = 5n; 0n };
            let _ = await f;
            let y = x - 1n;
            y
        }

        function main() -> bigint {
            spawn_bigint_sub()
        }
        "#
    );

    assert!(output.bytecode.contains("bin_op -"));
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(4))));
}

#[tokio::test]
async fn spawn_shared_bigint_mixed_with_int_uses_generic_binop() {
    let output = baml_test!(
        r#"
        function spawn_bigint_mixed(n: int) -> bigint {
            let x = 10n;
            let f = spawn { x = 6n; 0n };
            let _ = await f;
            let a = x - n;
            let b = n - x;
            let c = x << n;
            a + b + c
        }

        function main() -> bigint {
            spawn_bigint_mixed(2)
        }
        "#
    );

    assert!(output.bytecode.contains("bin_op -"));
    assert!(output.bytecode.contains("bin_op <<"));
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(24)))
    );
}
