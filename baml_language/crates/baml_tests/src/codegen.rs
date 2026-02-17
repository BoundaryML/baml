//! Common test utilities for compiler tests.

#![allow(clippy::print_stderr)] // Tests use eprintln! for debugging output

use std::collections::HashMap;

use crate::{
    bytecode::{assert_no_diagnostic_errors, setup_test_db},
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
    function: &bex_vm_types::Function,
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
    inst: &bex_vm_types::Instruction,
    inst_idx: usize,
    constants: &[bex_vm_types::ConstValue],
    objects: &bex_vm_types::ObjectPool,
    globals: &HashMap<String, usize>,
    function: &bex_vm_types::Function,
) -> anyhow::Result<Instruction> {
    // Build reverse lookup for globals (index -> name)
    let globals_by_index: HashMap<usize, &str> = globals
        .iter()
        .map(|(name, idx)| (*idx, name.as_str()))
        .collect();

    Ok(match inst {
        bex_vm_types::Instruction::LoadConst(idx) => {
            let value = &constants[*idx];
            let test_value = convert_value(value, objects)?;
            Instruction::LoadConst(test_value)
        }
        bex_vm_types::Instruction::LoadVar(idx) => {
            let var_name = resolve_var_name(*idx, inst_idx, function)?;
            Instruction::LoadVar(var_name)
        }
        bex_vm_types::Instruction::StoreVar(idx) => {
            let var_name = resolve_var_name(*idx, inst_idx, function)?;
            Instruction::StoreVar(var_name)
        }
        bex_vm_types::Instruction::LoadGlobal(global_idx) => {
            let name = globals_by_index
                .get(&global_idx.raw())
                .map(|s| (*s).to_string())
                .unwrap_or_else(|| format!("global_{global_idx}"));
            Instruction::LoadGlobal(Value::function(&name))
        }
        bex_vm_types::Instruction::StoreGlobal(global_idx) => {
            let name = globals_by_index
                .get(&global_idx.raw())
                .map(|s| (*s).to_string())
                .unwrap_or_else(|| format!("global_{global_idx}"));
            Instruction::StoreGlobal(Value::function(&name))
        }
        bex_vm_types::Instruction::LoadField(idx) => Instruction::LoadField(*idx),
        bex_vm_types::Instruction::StoreField(idx) => Instruction::StoreField(*idx),
        bex_vm_types::Instruction::Pop(n) => Instruction::Pop(*n),
        bex_vm_types::Instruction::Copy(idx) => Instruction::Copy(*idx),
        bex_vm_types::Instruction::Jump(offset) => Instruction::Jump(*offset),
        bex_vm_types::Instruction::PopJumpIfFalse(offset) => Instruction::PopJumpIfFalse(*offset),
        bex_vm_types::Instruction::BinOp(op) => Instruction::BinOp(*op),
        bex_vm_types::Instruction::CmpOp(op) => Instruction::CmpOp(*op),
        bex_vm_types::Instruction::UnaryOp(op) => Instruction::UnaryOp(*op),
        bex_vm_types::Instruction::AllocArray(n) => Instruction::AllocArray(*n),
        bex_vm_types::Instruction::AllocMap(n) => Instruction::AllocMap(*n),
        bex_vm_types::Instruction::LoadArrayElement => Instruction::LoadArrayElement,
        bex_vm_types::Instruction::LoadMapElement => Instruction::LoadMapElement,
        bex_vm_types::Instruction::StoreArrayElement => Instruction::StoreArrayElement,
        bex_vm_types::Instruction::StoreMapElement => Instruction::StoreMapElement,
        bex_vm_types::Instruction::AllocInstance(obj_idx) => {
            let obj = objects.get(obj_idx.raw()).ok_or_else(|| {
                anyhow::anyhow!(
                    "Object index {obj_idx} not found for AllocInstance (have {} objects)",
                    objects.len()
                )
            })?;
            match obj {
                bex_vm_types::Object::Class(class) => {
                    Instruction::AllocInstance(Value::class(&class.name))
                }
                _ => anyhow::bail!("Expected Class object for AllocInstance, got {obj:?}"),
            }
        }
        bex_vm_types::Instruction::AllocVariant(obj_idx) => {
            let obj = objects.get(obj_idx.raw()).ok_or_else(|| {
                anyhow::anyhow!("Object index {obj_idx} not found for AllocVariant")
            })?;
            match obj {
                bex_vm_types::Object::Enum(enm) => Instruction::AllocVariant(Value::enm(&enm.name)),
                _ => anyhow::bail!("Expected Enum object for AllocVariant, got {obj:?}"),
            }
        }
        bex_vm_types::Instruction::DispatchFuture(n) => Instruction::DispatchFuture(*n),
        bex_vm_types::Instruction::Await => Instruction::Await,
        bex_vm_types::Instruction::Watch(idx) => Instruction::Watch(*idx),
        bex_vm_types::Instruction::Unwatch(idx) => Instruction::Unwatch(*idx),
        bex_vm_types::Instruction::Notify(idx) => Instruction::Notify(*idx),
        bex_vm_types::Instruction::Call(n) => Instruction::Call(*n),

        bex_vm_types::Instruction::Return => Instruction::Return,
        bex_vm_types::Instruction::Assert => Instruction::Assert,
        bex_vm_types::Instruction::NotifyBlock(idx) => Instruction::NotifyBlock(*idx),
        bex_vm_types::Instruction::VizEnter(idx) => Instruction::VizEnter(*idx),
        bex_vm_types::Instruction::VizExit(idx) => Instruction::VizExit(*idx),
        bex_vm_types::Instruction::InitLocals(n) => Instruction::InitLocals(*n),
        bex_vm_types::Instruction::JumpTable { table_idx, default } => Instruction::JumpTable {
            table_idx: *table_idx,
            default: *default,
        },
        bex_vm_types::Instruction::Discriminant => Instruction::Discriminant,
        bex_vm_types::Instruction::TypeTag => Instruction::TypeTag,
        bex_vm_types::Instruction::Unreachable => Instruction::Unreachable,
    })
}

/// Convert a compile-time ConstValue to a test Value by resolving object indices.
fn convert_value(
    value: &bex_vm_types::ConstValue,
    objects: &bex_vm_types::ObjectPool,
) -> anyhow::Result<Value> {
    Ok(match value {
        bex_vm_types::ConstValue::Null => Value::Null,
        bex_vm_types::ConstValue::Int(i) => Value::Int(*i),
        bex_vm_types::ConstValue::Float(f) => Value::Float(*f),
        bex_vm_types::ConstValue::Bool(b) => Value::Bool(*b),
        bex_vm_types::ConstValue::Object(obj_idx) => {
            let obj = objects
                .get(obj_idx.raw())
                .ok_or_else(|| anyhow::anyhow!("Object index {obj_idx} not found"))?;
            match obj {
                bex_vm_types::Object::String(s) => Value::string(s),
                bex_vm_types::Object::Function(f) => Value::function(&f.name),
                bex_vm_types::Object::Class(c) => Value::class(&c.name),
                bex_vm_types::Object::Enum(e) => Value::enm(&e.name),
                _ => anyhow::bail!("Unsupported object type in constant pool: {obj:?}"),
            }
        }
    })
}

/// Compiled function with its objects.
struct CompiledFunction {
    function: bex_vm_types::Function,
    /// All objects from the program - indices in bytecode constants reference this.
    objects: bex_vm_types::ObjectPool,
}

/// Result of compiling source code.
type CompileResult = (Vec<(String, CompiledFunction)>, HashMap<String, usize>);

/// Compile BAML source and return compiled functions with their object pools.
///
/// Uses the production `compile_files` function to ensure tests match real behavior.
/// Also checks for diagnostic errors.
fn compile_source(source: &str) -> CompileResult {
    let db = setup_test_db(source);
    assert_no_diagnostic_errors(&db);

    let project = db.get_project().unwrap();
    let all_files = project.files(&db).clone();
    let program = baml_compiler_emit::compile_files(&db, &all_files)
        .expect("compile_files should succeed for valid test source");

    // Extract functions from the program
    let mut functions = Vec::new();
    for (name, obj_idx) in &program.function_indices {
        if let Some(bex_vm_types::Object::Function(func)) = program.objects.get(*obj_idx) {
            functions.push((
                name.clone(),
                CompiledFunction {
                    function: (**func).clone(),
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
        if let bex_vm_types::ConstValue::Object(obj_idx) = value {
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
                && let Some(bex_vm_types::Object::Function(func)) =
                    program.objects.get(obj_idx.raw())
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
