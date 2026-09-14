use baml_tests::{
    engine::{OptLevel, run_test},
    stdlib_prefix::compile_source_with_opt,
};
use bex_external_types::BexExternalValue;
use bex_vm_types::{Instruction, Object};
use indexmap::IndexMap;

const SOURCE: &str = include_str!("../baml_src/ns_optimizer_stack/optimizer_stack.baml");

#[tokio::test]
async fn optimizer_preserves_behavior_at_each_level() {
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let output = run_test(SOURCE, "verify_all", IndexMap::new(), opt).await;
        assert_eq!(output.result, Ok(BexExternalValue::Bool(true)), "{opt:?}");
    }
}

#[test]
fn optimized_instruction_and_local_counts() {
    let program = compile_source_with_opt(SOURCE, OptLevel::Two);
    for (name, instructions, locals) in [
        ("and_value", 6, 0),
        ("or_value", 6, 0),
        ("coalesce", 4, 0),
        ("array_mixed", 8, 0),
        ("virtual_result", 5, 0),
        ("constants", 2, 0),
        ("overwritten", 6, 0),
    ] {
        let index = program.function_index(&format!("user.{name}")).unwrap();
        let Some(Object::Function(function)) = program.objects.get(index) else {
            panic!("expected function")
        };
        assert_eq!(
            function.bytecode.instructions.len(),
            instructions,
            "{name}: {:?}",
            function.bytecode.instructions
        );
        assert_eq!(function.real_local_count, locals, "{name}");
    }
    let index = program.function_index("user.and_value").unwrap();
    let Some(Object::Function(function)) = program.objects.get(index) else {
        panic!("expected function")
    };
    assert_eq!(
        function
            .bytecode
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction, Instruction::JumpIfFalseOrPop(_)))
            .count(),
        2
    );
}
