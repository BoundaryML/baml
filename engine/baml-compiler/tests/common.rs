//! Common test utilities for compiler tests.

use baml_types::TypeIR;
use baml_vm::{test, BamlVmProgram, EvalStack, GlobalPool, Instruction, Object, ObjectPool, Value};

/// Helper struct for testing bytecode compilation.
pub struct Program {
    pub source: &'static str,
    pub expected: Vec<(&'static str, Vec<test::Instruction>)>,
}

/// Convert a runtime Instruction to a test Instruction by resolving indices to values.
fn convert_instruction(
    inst: &Instruction,
    constants: &[Value],
    objects: &ObjectPool,
    globals: &GlobalPool,
) -> anyhow::Result<test::Instruction> {
    Ok(match inst {
        Instruction::LoadConst(idx) => {
            let value = &constants[*idx];
            let test_value = convert_value(value, objects)?;
            test::Instruction::LoadConst(test_value)
        }
        Instruction::LoadVar(idx) => test::Instruction::LoadVar(*idx),
        Instruction::StoreVar(idx) => test::Instruction::StoreVar(*idx),
        Instruction::LoadGlobal(global_idx) => {
            let value = &globals[*global_idx];
            let test_value = convert_value(value, objects)?;
            test::Instruction::LoadGlobal(test_value)
        }
        Instruction::StoreGlobal(global_idx) => {
            let value = &globals[*global_idx];
            let test_value = convert_value(value, objects)?;
            test::Instruction::StoreGlobal(test_value)
        }
        Instruction::LoadField(idx) => test::Instruction::LoadField(*idx),
        Instruction::StoreField(idx) => test::Instruction::StoreField(*idx),
        Instruction::Pop(n) => test::Instruction::Pop(*n),
        Instruction::Copy(idx) => test::Instruction::Copy(*idx),
        Instruction::PopReplace(n) => test::Instruction::PopReplace(*n),
        Instruction::Jump(offset) => test::Instruction::Jump(*offset),
        Instruction::JumpIfFalse(offset) => test::Instruction::JumpIfFalse(*offset),
        Instruction::BinOp(op) => test::Instruction::BinOp(*op),
        Instruction::CmpOp(op) => test::Instruction::CmpOp(*op),
        Instruction::UnaryOp(op) => test::Instruction::UnaryOp(*op),
        Instruction::AllocArray(n) => test::Instruction::AllocArray(*n),
        Instruction::AllocMap(n) => test::Instruction::AllocMap(*n),
        Instruction::LoadArrayElement => test::Instruction::LoadArrayElement,
        Instruction::LoadMapElement => test::Instruction::LoadMapElement,
        Instruction::StoreArrayElement => test::Instruction::StoreArrayElement,
        Instruction::StoreMapElement => test::Instruction::StoreMapElement,
        Instruction::AllocInstance(obj_idx) => {
            let obj = &objects[*obj_idx];
            match obj {
                Object::Class(class) => test::Instruction::AllocInstance(test::Value::Object(
                    test::Object::class(&class.name),
                )),
                _ => anyhow::bail!("Expected Class object for AllocInstance, got {:?}", obj),
            }
        }
        Instruction::AllocVariant(obj_idx) => {
            let obj = &objects[*obj_idx];
            match obj {
                Object::Enum(enm) => test::Instruction::AllocVariant(test::Value::Object(
                    test::Object::enm(&enm.name),
                )),
                _ => anyhow::bail!("Expected Enum object for AllocVariant, got {:?}", obj),
            }
        }
        Instruction::DispatchFuture(n) => test::Instruction::DispatchFuture(*n),
        Instruction::Await => test::Instruction::Await,
        Instruction::Watch(idx) => test::Instruction::Watch(*idx),
        Instruction::Notify(idx) => test::Instruction::Notify(*idx),
        Instruction::Call(n) => test::Instruction::Call(*n),
        Instruction::Return => test::Instruction::Return,
        Instruction::Assert => test::Instruction::Assert,
        Instruction::NotifyBlock(notification) => test::Instruction::NotifyBlock(*notification),
    })
}

/// Convert a runtime Value to a test Value by resolving object indices.
fn convert_value(value: &Value, objects: &ObjectPool) -> anyhow::Result<test::Value> {
    Ok(match value {
        Value::Null => test::Value::Null,
        Value::Int(i) => test::Value::Int(*i),
        Value::Float(f) => test::Value::Float(*f),
        Value::Bool(b) => test::Value::Bool(*b),
        Value::Object(obj_idx) => {
            let obj = &objects[*obj_idx];
            let test_obj = match obj {
                Object::String(s) => test::Object::string(s),
                Object::Function(f) => test::Object::function(&f.name),
                Object::Class(c) => test::Object::class(&c.name),
                Object::Enum(e) => test::Object::enm(&e.name),
                Object::BamlType(baml_type) => {
                    // BamlType represents a type parameter (e.g., <DummyJsonTodo> in baml.fetch_as<T>)
                    // Extract the class name from the type
                    match baml_type {
                        TypeIR::Class { name, .. } => test::Object::class(name),
                        TypeIR::Enum { name, .. } => test::Object::enm(name),
                        _ => {
                            anyhow::bail!("Unsupported BamlType in constant pool: {:?}", baml_type)
                        }
                    }
                }
                _ => anyhow::bail!("Unsupported object type in constant pool: {:?}", obj),
            };
            test::Value::Object(test_obj)
        }
    })
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

        // Convert runtime instructions to test instructions
        let actual_instructions: Vec<test::Instruction> = function
            .bytecode
            .instructions
            .iter()
            .map(|inst| convert_instruction(inst, &function.bytecode.constants, &objects, &globals))
            .collect::<anyhow::Result<Vec<_>>>()?;

        assert_eq!(
            actual_instructions, expected_instructions,
            "Bytecode mismatch for function '{function_name}'"
        );
    }

    Ok(())
}
