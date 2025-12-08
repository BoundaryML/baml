//! Common test utilities for compiler tests.

use baml_types::TypeIR;
use baml_vm::{test, BamlVmProgram, EvalStack, GlobalPool, Instruction, Object, ObjectPool, Value};

/// Helper struct for testing bytecode compilation.
pub struct Program {
    pub source: &'static str,
    pub expected: Vec<(&'static str, Vec<test::Instruction>)>,
}

/// Resolve a variable index to its name using scope information.
fn resolve_var_name(
    var_idx: usize,
    inst_idx: usize,
    function: &baml_vm::Function,
) -> anyhow::Result<String> {
    // Get the scope ID for this instruction
    let scope_id = function
        .bytecode
        .scopes
        .get(inst_idx)
        .ok_or_else(|| anyhow::anyhow!("No scope ID for instruction at index {}", inst_idx))?;

    // Get the locals for this scope
    let scope_locals = function
        .locals_in_scope
        .get(*scope_id)
        .ok_or_else(|| anyhow::anyhow!("No locals for scope {}", scope_id))?;

    // Direct lookup: the Vec is indexed by variable index
    scope_locals.get(var_idx).cloned().ok_or_else(|| {
        anyhow::anyhow!(
            "Variable index {} not found in scope {} (scope has {} variables)",
            var_idx,
            scope_id,
            scope_locals.len()
        )
    })
}

/// Convert a runtime Instruction to a test Instruction by resolving indices to values.
fn convert_instruction(
    inst: &Instruction,
    inst_idx: usize,
    constants: &[Value],
    objects: &ObjectPool,
    globals: &GlobalPool,
    function: &baml_vm::Function,
) -> anyhow::Result<test::Instruction> {
    Ok(match inst {
        Instruction::LoadConst(idx) => {
            let value = &constants[*idx];
            let test_value = convert_value(value, objects)?;
            test::Instruction::LoadConst(test_value)
        }
        Instruction::LoadVar(idx) => {
            let var_name = resolve_var_name(*idx, inst_idx, function)?;
            test::Instruction::LoadVar(var_name)
        }
        Instruction::StoreVar(idx) => {
            let var_name = resolve_var_name(*idx, inst_idx, function)?;
            test::Instruction::StoreVar(var_name)
        }
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
        Instruction::VizEnter(idx) => test::Instruction::VizEnter(*idx),
        Instruction::VizExit(idx) => test::Instruction::VizExit(*idx),
        Instruction::Call(n) => test::Instruction::Call(*n),
        Instruction::Return => test::Instruction::Return,
        Instruction::Assert => test::Instruction::Assert,
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
        let actual_with_idx: Vec<(usize, test::Instruction)> = function
            .bytecode
            .instructions
            .iter()
            .enumerate()
            .map(|(inst_idx, inst)| {
                convert_instruction(
                    inst,
                    inst_idx,
                    &function.bytecode.constants,
                    &objects,
                    &globals,
                    function,
                )
                .map(|instr| (inst_idx, instr))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let instr_len = actual_with_idx.len();
        let mut mapping = vec![None; instr_len];

        let is_viz_flags: Vec<bool> = actual_with_idx
            .iter()
            .map(|(_, instr)| {
                matches!(
                    instr,
                    test::Instruction::VizEnter(_) | test::Instruction::VizExit(_)
                )
            })
            .collect();

        let is_viz_idx = |idx: usize| is_viz_flags[idx];

        for (new_idx, (orig_idx, _instr)) in actual_with_idx
            .iter()
            .filter(|(_, instr)| {
                !matches!(
                    instr,
                    test::Instruction::VizEnter(_) | test::Instruction::VizExit(_)
                )
            })
            .enumerate()
        {
            mapping[*orig_idx] = Some(new_idx);
        }

        let remap_offset = |orig_idx: usize, offset: isize| -> anyhow::Result<isize> {
            let new_current = mapping[orig_idx]
                .ok_or_else(|| anyhow::anyhow!("No mapping for instruction {orig_idx}"))?;

            let mut target = orig_idx as isize + offset;
            let step = if offset >= 0 { 1 } else { -1 };

            while target >= 0 && (target as usize) < instr_len && mapping[target as usize].is_none()
            {
                target += step;
            }

            let new_target = mapping
                .get(target as usize)
                .and_then(|m| *m)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Unable to remap jump target from {orig_idx} with offset {offset}"
                    )
                })?;

            Ok(new_target as isize - new_current as isize)
        };

        let actual_instructions: Vec<test::Instruction> = actual_with_idx
            .into_iter()
            .filter(|(_, instr)| {
                !matches!(
                    instr,
                    test::Instruction::VizEnter(_) | test::Instruction::VizExit(_)
                )
            })
            .map(|(orig_idx, instr)| {
                let adjusted = match instr {
                    test::Instruction::Jump(offset) => {
                        test::Instruction::Jump(remap_offset(orig_idx, offset)?)
                    }
                    test::Instruction::JumpIfFalse(offset) => {
                        test::Instruction::JumpIfFalse(remap_offset(orig_idx, offset)?)
                    }
                    other => other,
                };

                Ok(adjusted)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        assert_eq!(
            actual_instructions, expected_instructions,
            "Bytecode mismatch for function '{function_name}'"
        );
    }

    Ok(())
}
