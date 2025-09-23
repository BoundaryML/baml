//! Common test utilities for compiler tests.

use baml_vm::{BamlVmProgram, EvalStack, Instruction, Object};

/// Helper struct for testing bytecode compilation.
pub struct Program {
    pub source: &'static str,
    pub expected: Vec<(&'static str, Vec<Instruction>)>,
}

/// Helper function to assert that source code compiles to expected bytecode
/// instructions.
#[track_caller]
pub fn assert_compiles(input: Program) -> anyhow::Result<()> {
    let ast = baml_compiler::test::ast(input.source)?;

    let BamlVmProgram {
        objects, globals, ..
    } = baml_compiler::compile(&ast)?;

    // Create a map of function name to function for easy lookup
    let functions: std::collections::HashMap<&str, &baml_vm::Function> = objects
        .iter()
        .filter_map(|obj| match obj {
            Object::Function(f) => Some((f.name.as_str(), f)),
            _ => None,
        })
        .collect();

    // Check each expected function
    for (function_name, expected_instructions) in input.expected {
        let function = functions
            .get(function_name)
            .ok_or_else(|| anyhow::anyhow!("function '{}' not found", function_name))?;

        eprintln!(
            "---- fn {function_name}() ----\n{}",
            baml_vm::debug::display_bytecode(function, &EvalStack::new(), &objects, &globals, true)
        );

        assert_eq!(
            function.bytecode.instructions, expected_instructions,
            "Bytecode mismatch for function '{function_name}'"
        );
    }

    Ok(())
}
