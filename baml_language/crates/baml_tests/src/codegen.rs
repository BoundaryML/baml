//! Common test utilities for compiler tests.

#![allow(clippy::print_stderr)] // Tests use eprintln! for debugging output

use std::collections::HashMap;

use crate::{
    bytecode::TestDatabase,
    vm::{Instruction, Value},
};

/// Helper struct for testing bytecode compilation.
pub struct Program {
    pub source: &'static str,
    pub expected: Vec<(&'static str, Vec<Instruction>)>,
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
        .ok_or_else(|| anyhow::anyhow!("No scope ID for instruction at index {inst_idx}"))?;

    // Get the locals for this scope
    let scope_locals = function
        .locals_in_scope
        .get(*scope_id)
        .ok_or_else(|| anyhow::anyhow!("No locals for scope {scope_id}"))?;

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
    inst: &baml_vm::Instruction,
    inst_idx: usize,
    constants: &[baml_vm::Value],
    objects: &[baml_vm::Object],
    globals: &HashMap<String, usize>,
    function: &baml_vm::Function,
) -> anyhow::Result<Instruction> {
    // Build reverse lookup for globals (index -> name)
    let globals_by_index: HashMap<usize, &str> = globals
        .iter()
        .map(|(name, idx)| (*idx, name.as_str()))
        .collect();

    Ok(match inst {
        baml_vm::Instruction::LoadConst(idx) => {
            let value = &constants[*idx];
            let test_value = convert_value(value, objects)?;
            Instruction::LoadConst(test_value)
        }
        baml_vm::Instruction::LoadVar(idx) => {
            let var_name = resolve_var_name(*idx, inst_idx, function)?;
            Instruction::LoadVar(var_name)
        }
        baml_vm::Instruction::StoreVar(idx) => {
            let var_name = resolve_var_name(*idx, inst_idx, function)?;
            Instruction::StoreVar(var_name)
        }
        baml_vm::Instruction::LoadGlobal(global_idx) => {
            let name = globals_by_index
                .get(&global_idx.raw())
                .map(|s| (*s).to_string())
                .unwrap_or_else(|| format!("global_{global_idx}"));
            Instruction::LoadGlobal(Value::function(&name))
        }
        baml_vm::Instruction::StoreGlobal(global_idx) => {
            let name = globals_by_index
                .get(&global_idx.raw())
                .map(|s| (*s).to_string())
                .unwrap_or_else(|| format!("global_{global_idx}"));
            Instruction::StoreGlobal(Value::function(&name))
        }
        baml_vm::Instruction::LoadField(idx) => Instruction::LoadField(*idx),
        baml_vm::Instruction::StoreField(idx) => Instruction::StoreField(*idx),
        baml_vm::Instruction::Pop(n) => Instruction::Pop(*n),
        baml_vm::Instruction::Copy(idx) => Instruction::Copy(*idx),
        baml_vm::Instruction::PopReplace(n) => Instruction::PopReplace(*n),
        baml_vm::Instruction::Jump(offset) => Instruction::Jump(*offset),
        baml_vm::Instruction::JumpIfFalse(offset) => Instruction::JumpIfFalse(*offset),
        baml_vm::Instruction::BinOp(op) => Instruction::BinOp(*op),
        baml_vm::Instruction::CmpOp(op) => Instruction::CmpOp(*op),
        baml_vm::Instruction::UnaryOp(op) => Instruction::UnaryOp(*op),
        baml_vm::Instruction::AllocArray(n) => Instruction::AllocArray(*n),
        baml_vm::Instruction::AllocMap(n) => Instruction::AllocMap(*n),
        baml_vm::Instruction::LoadArrayElement => Instruction::LoadArrayElement,
        baml_vm::Instruction::LoadMapElement => Instruction::LoadMapElement,
        baml_vm::Instruction::StoreArrayElement => Instruction::StoreArrayElement,
        baml_vm::Instruction::StoreMapElement => Instruction::StoreMapElement,
        baml_vm::Instruction::AllocInstance(obj_idx) => {
            let obj = objects.get(obj_idx.raw()).ok_or_else(|| {
                anyhow::anyhow!(
                    "Object index {obj_idx} not found for AllocInstance (have {} objects)",
                    objects.len()
                )
            })?;
            match obj {
                baml_vm::Object::Class(class) => {
                    Instruction::AllocInstance(Value::class(&class.name))
                }
                _ => anyhow::bail!("Expected Class object for AllocInstance, got {obj:?}"),
            }
        }
        baml_vm::Instruction::AllocVariant(obj_idx) => {
            let obj = objects.get(obj_idx.raw()).ok_or_else(|| {
                anyhow::anyhow!("Object index {obj_idx} not found for AllocVariant")
            })?;
            match obj {
                baml_vm::Object::Enum(enm) => Instruction::AllocVariant(Value::enm(&enm.name)),
                _ => anyhow::bail!("Expected Enum object for AllocVariant, got {obj:?}"),
            }
        }
        baml_vm::Instruction::DispatchFuture(n) => Instruction::DispatchFuture(*n),
        baml_vm::Instruction::Await => Instruction::Await,
        baml_vm::Instruction::Watch(idx) => Instruction::Watch(*idx),
        baml_vm::Instruction::Notify(idx) => Instruction::Notify(*idx),
        baml_vm::Instruction::Call(n) => Instruction::Call(*n),
        baml_vm::Instruction::Return => Instruction::Return,
        baml_vm::Instruction::Assert => Instruction::Assert,
        baml_vm::Instruction::NotifyBlock(idx) => Instruction::NotifyBlock(*idx),
    })
}

/// Convert a runtime Value to a test Value by resolving object indices.
fn convert_value(value: &baml_vm::Value, objects: &[baml_vm::Object]) -> anyhow::Result<Value> {
    Ok(match value {
        baml_vm::Value::Null => Value::Null,
        baml_vm::Value::Int(i) => Value::Int(*i),
        baml_vm::Value::Float(f) => Value::Float(*f),
        baml_vm::Value::Bool(b) => Value::Bool(*b),
        baml_vm::Value::Object(obj_idx) => {
            let obj = objects
                .get(obj_idx.raw())
                .ok_or_else(|| anyhow::anyhow!("Object index {obj_idx} not found"))?;
            match obj {
                baml_vm::Object::String(s) => Value::string(s),
                baml_vm::Object::Function(f) => Value::function(&f.name),
                baml_vm::Object::Class(c) => Value::class(&c.name),
                baml_vm::Object::Enum(e) => Value::enm(&e.name),
                _ => anyhow::bail!("Unsupported object type in constant pool: {obj:?}"),
            }
        }
    })
}

/// Compiled function with its objects.
struct CompiledFunction {
    function: baml_vm::Function,
    /// All objects from the program - indices in bytecode constants reference this.
    objects: baml_vm::ObjectPool,
}

/// Result of compiling source code.
type CompileResult = (Vec<(String, CompiledFunction)>, HashMap<String, usize>);

/// Compile BAML source and return compiled functions with their object pools.
///
/// Uses the production `compile_files` function to ensure tests match real behavior.
fn compile_source(source: &str) -> CompileResult {
    let mut db = TestDatabase::new();
    let file = db.add_file("test.baml", source);
    db.set_project(vec![file]);

    // Use the production compile_files function
    let program = baml_codegen::compile_files(&db, &[file]);

    // Extract functions from the program
    let mut functions = Vec::new();
    for (name, obj_idx) in &program.function_indices {
        if let Some(baml_vm::Object::Function(func)) = program.objects.get(*obj_idx) {
            functions.push((
                name.clone(),
                CompiledFunction {
                    function: func.clone(),
                    // All objects are in the program's object pool
                    objects: program.objects.clone(),
                },
            ));
        }
    }

    // Build globals map: function_name -> global_idx
    // This reconstructs the mapping from the program's globals list
    // Include both user-defined functions and builtins
    let mut globals: HashMap<String, usize> = HashMap::new();
    for (global_idx, value) in program.globals.iter().enumerate() {
        if let baml_vm::Value::Object(obj_idx) = value {
            // First check user-defined functions
            let mut found = false;
            for (name, fn_obj_idx) in &program.function_indices {
                if *fn_obj_idx == obj_idx.raw() {
                    globals.insert(name.clone(), global_idx);
                    found = true;
                    break;
                }
            }
            // If not found in user functions, check if it's a builtin function
            if !found
                && let Some(baml_vm::Object::Function(func)) = program.objects.get(obj_idx.raw())
            {
                globals.insert(func.name.clone(), global_idx);
            }
        }
    }

    (functions, globals)
}

/// Helper function to assert that source code compiles to expected bytecode instructions.
#[track_caller]
pub fn assert_compiles(input: Program) -> anyhow::Result<()> {
    let (functions, globals) = compile_source(input.source);

    // Create a map of function name to compiled function for easy lookup
    let functions_map: HashMap<&str, &CompiledFunction> = functions
        .iter()
        .map(|(name, compiled)| (name.as_str(), compiled))
        .collect();

    // Check each expected function
    for (function_name, expected_instructions) in input.expected {
        let compiled = functions_map
            .get(function_name)
            .ok_or_else(|| anyhow::anyhow!("function '{function_name}' not found"))?;

        let function = &compiled.function;
        let objects = &compiled.objects;

        eprintln!("---- fn {function_name}() ----");
        for (i, inst) in function.bytecode.instructions.iter().enumerate() {
            eprintln!("  {i:3}: {inst}");
        }
        eprintln!();

        // Convert runtime instructions to test instructions
        let actual_instructions: Vec<Instruction> = function
            .bytecode
            .instructions
            .iter()
            .enumerate()
            .map(|(inst_idx, inst)| {
                convert_instruction(
                    inst,
                    inst_idx,
                    &function.bytecode.constants,
                    objects,
                    &globals,
                    function,
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        assert_eq!(
            actual_instructions, expected_instructions,
            "Bytecode mismatch for function '{function_name}'"
        );
    }

    Ok(())
}
