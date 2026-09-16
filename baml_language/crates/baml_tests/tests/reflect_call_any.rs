//! Runtime semantics of BEP-062's AnyFunction slice: `reflect.AnyFunction`
//! coercion carried to runtime, `reflect.signature`, and `reflect.call_any`
//! (argument checking, callee defaults, error propagation).

use baml_tests::{
    baml_test,
    engine::{IndexMap, OptLevel, compile_source_with_opt, run_compiled},
};
use bex_engine::BexExternalValue;
use bex_vm_types::{ConstValue, Instruction, Object};

#[tokio::test]
async fn call_any_inferred_class_return_crosses_sys_op_and_host_boundaries() {
    let output = baml_test!(
        r#"
        class Result {
            value string
        }

        function main() -> Result throws never {
            reflect.call_any(baml.sap.parse<Result>, { "text": `{"value":"hello"}` }) catch (e) {
                _ => Result { value: "error" }
            }
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::instance(
            "user.Result",
            [("value", BexExternalValue::String("hello".into()))]
                .into_iter()
                .collect(),
        ))
    );
}

#[tokio::test]
async fn call_any_rejects_a_return_outside_inferred_r() {
    let mut program = compile_source_with_opt(
        r#"
        function lie() -> string throws never {
            "declared string"
        }

        function main() -> string throws never {
            reflect.call_any(lie, {}) catch_all (e) {
                reflect.InvalidArgumentError => {
                    return e.argument + "|" + e.expected.to_string() + "|" + e.got.to_string()
                },
                _ => return "unexpected error",
            }
            "mismatch was accepted"
        }
        "#,
        OptLevel::One,
    );

    // Preserve `lie`'s declared `string` signature while deliberately making
    // its bytecode return an `int`. This models a faulty dynamic/host callee
    // without allowing another boundary to reject the value first.
    let lie_idx = program
        .function_index("user.lie")
        .expect("user.lie should exist");
    let Object::Function(lie) = program
        .objects
        .get_mut(lie_idx)
        .expect("user.lie object should exist")
    else {
        panic!("user.lie should be a function");
    };
    lie.bytecode.instructions = vec![Instruction::LoadConst(0), Instruction::Return];
    lie.bytecode.constants = vec![ConstValue::Int(42)];
    lie.bytecode.compact = None;

    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "reflect.call_any return value|string|int".into()
        ))
    );
}
