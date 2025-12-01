//! Common test utilities for compiler tests.

#![allow(clippy::print_stderr)] // Tests use eprintln! for debugging output

use std::collections::HashMap;

use baml_db::{RootDatabase, baml_hir, baml_thir};
use baml_hir::{function_body, function_signature};
use baml_thir::build_typing_context_from_files;
use baml_vm::test::{Instruction, Value};

use crate::ClassInfo;

/// Helper struct for testing bytecode compilation.
pub(super) struct Program {
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
    fn_objects: &[baml_vm::Object],
    class_objects: &[baml_vm::Object],
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
            let test_value = convert_value(value, fn_objects)?;
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
                .get(global_idx)
                .map(|s| (*s).to_string())
                .unwrap_or_else(|| format!("global_{global_idx}"));
            Instruction::LoadGlobal(Value::Function(name))
        }
        baml_vm::Instruction::StoreGlobal(global_idx) => {
            let name = globals_by_index
                .get(global_idx)
                .map(|s| (*s).to_string())
                .unwrap_or_else(|| format!("global_{global_idx}"));
            Instruction::StoreGlobal(Value::Function(name))
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
            let obj = class_objects.get(*obj_idx).ok_or_else(|| {
                anyhow::anyhow!("Class object index {obj_idx} not found for AllocInstance (have {} class objects)", class_objects.len())
            })?;
            match obj {
                baml_vm::Object::Class(class) => {
                    Instruction::AllocInstance(Value::Class(class.name.clone()))
                }
                _ => anyhow::bail!("Expected Class object for AllocInstance, got {obj:?}"),
            }
        }
        baml_vm::Instruction::AllocVariant(obj_idx) => {
            // Enums would also be pre-allocated, similar to classes
            let obj = class_objects.get(*obj_idx).ok_or_else(|| {
                anyhow::anyhow!("Object index {obj_idx} not found for AllocVariant")
            })?;
            match obj {
                baml_vm::Object::Enum(enm) => {
                    Instruction::AllocVariant(Value::Enum(enm.name.clone()))
                }
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
                .get(*obj_idx)
                .ok_or_else(|| anyhow::anyhow!("Object index {obj_idx} not found"))?;
            match obj {
                baml_vm::Object::String(s) => Value::String(s.clone()),
                baml_vm::Object::Function(f) => Value::Function(f.name.clone()),
                baml_vm::Object::Class(c) => Value::Class(c.name.clone()),
                baml_vm::Object::Enum(e) => Value::Enum(e.name.clone()),
                _ => anyhow::bail!("Unsupported object type in constant pool: {obj:?}"),
            }
        }
    })
}

/// Compiled function with its objects.
struct CompiledFunction {
    function: baml_vm::Function,
    /// Function-local objects (strings, etc.) - indices in bytecode constants reference this.
    fn_objects: Vec<baml_vm::Object>,
    /// Pre-allocated class objects - `AllocInstance` indices reference this.
    class_objects: Vec<baml_vm::Object>,
}

/// Result of compiling source code.
type CompileResult = (Vec<(String, CompiledFunction)>, HashMap<String, usize>);

/// Compile BAML source and return compiled functions with their object pools.
fn compile_source(source: &str) -> CompileResult {
    let mut db = RootDatabase::new();
    let file = db.add_file("test.baml", source);

    // Get all functions from the file
    let items_struct = baml_hir::file_items(&db, file);
    let items = items_struct.items(&db);

    // Get the item tree for class lookups
    let item_tree = baml_hir::file_item_tree(&db, file);

    // Build globals map (function name -> index)
    let mut globals: HashMap<String, usize> = HashMap::new();
    let mut global_idx = 0;
    for item in items {
        if let baml_hir::ItemId::Function(func_loc) = item {
            let sig = function_signature(&db, *func_loc);
            globals.insert(sig.name.to_string(), global_idx);
            global_idx += 1;
        }
    }

    // Add native functions for for-in loop support
    globals.insert("baml.Array.length".to_string(), global_idx);
    global_idx += 1;
    let _ = global_idx; // suppress unused variable warning

    // Build classes map (class name -> ClassInfo) and pre-allocate Class objects
    let mut classes: HashMap<String, ClassInfo> = HashMap::new();
    let mut class_object_indices: HashMap<String, usize> = HashMap::new();
    let mut class_objects: Vec<baml_vm::Object> = Vec::new();

    for item in items {
        if let baml_hir::ItemId::Class(class_loc) = item {
            let class = &item_tree[class_loc.id(&db)];
            let class_name = class.name.to_string();

            let mut field_indices = HashMap::new();
            let mut field_names = Vec::new();
            for (idx, field) in class.fields.iter().enumerate() {
                field_indices.insert(field.name.to_string(), idx);
                field_names.push(field.name.to_string());
            }

            // Pre-allocate Class object and record its index
            let class_obj_idx = class_objects.len();
            class_objects.push(baml_vm::Object::Class(baml_vm::Class {
                name: class_name.clone(),
                field_names: field_names.clone(),
            }));
            class_object_indices.insert(class_name.clone(), class_obj_idx);

            classes.insert(
                class_name,
                ClassInfo {
                    field_indices,
                    field_names,
                },
            );
        }
    }

    // Build typing context
    let typing_context = build_typing_context_from_files(&db, &[file]);

    // Compile each function
    let mut functions = Vec::new();
    for item in items {
        if let baml_hir::ItemId::Function(func_loc) = item {
            let signature = function_signature(&db, *func_loc);
            let body = function_body(&db, *func_loc);

            // Run type inference
            let inference = baml_thir::infer_function(
                &db,
                &signature,
                &body,
                Some(typing_context.clone()),
                None,
            );

            // Get parameter names
            let params: Vec<baml_base::Name> =
                signature.params.iter().map(|p| p.name.clone()).collect();

            // Compile to bytecode
            let (compiled, fn_objects) = crate::compile_function(
                signature.name.as_str(),
                &params,
                &body,
                &inference,
                globals.clone(),
                classes.clone(),
                class_object_indices.clone(),
            );

            functions.push((
                signature.name.to_string(),
                CompiledFunction {
                    function: compiled,
                    fn_objects,
                    class_objects: class_objects.clone(),
                },
            ));
        }
    }

    (functions, globals)
}

/// Helper function to assert that source code compiles to expected bytecode instructions.
#[track_caller]
pub(super) fn assert_compiles(input: Program) -> anyhow::Result<()> {
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
        let fn_objects = &compiled.fn_objects;
        let class_objects = &compiled.class_objects;

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
                    fn_objects,
                    class_objects,
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
