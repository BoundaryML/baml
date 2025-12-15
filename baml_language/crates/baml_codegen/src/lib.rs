//! Code generation for BAML.
//!
//! Compiles the Typed High-level IR (THIR) to bytecode for the BAML VM.
//!
//! # Architecture
//!
//! The compilation pipeline is:
//! ```text
//! Source -> CST -> HIR -> THIR -> Bytecode
//! ```
//!
//! This crate handles the final step: THIR -> Bytecode.
//!
//! The compiler takes THIR's `InferenceResult` (which contains type information
//! for every expression) along with the HIR expression body, and generates
//! stack-based bytecode instructions. Key components:
//!
//! - **Compiler**: Main entry point that compiles functions using THIR types
//! - **Scope tracking**: Manages local variables and their stack positions
//! - **Constant pool**: Deduplicates constant values
//! - **Jump patching**: Handles forward jumps for control flow

mod compiler;

use std::collections::HashMap;

use baml_base::{Name, SourceFile};
use baml_hir::{self, ItemId, function_body, function_signature};
pub use baml_vm::{
    BinOp, Bytecode, Class, CmpOp, Enum, Function, FunctionKind, GlobalIndex, Instruction, Object,
    ObjectIndex, Program, UnaryOp, Value,
};
use baml_workspace::Project;
pub use compiler::{CodegenContext, Compiler, compile_function};

/// Generate bytecode for all functions in a project.
///
/// This is the main entry point for project-wide code generation.
/// It collects all functions from HIR, type-checks them via THIR,
/// and compiles them to bytecode.
pub fn generate_project_bytecode(db: &dyn baml_thir::Db, root: Project) -> Program {
    let files = baml_workspace::project_files(db, root);
    compile_files(db, &files)
}

/// Generate bytecode for a list of source files.
///
/// This is useful for testing or when you have a subset of files.
pub fn compile_files(db: &dyn baml_thir::Db, files: &[SourceFile]) -> Program {
    let mut program = Program::new();

    // Build typing context (maps function names to their types)
    let typing_context = build_typing_context(db, files);

    // Build globals map (function name -> global index)
    let mut globals: HashMap<String, usize> = HashMap::new();
    let mut global_idx = 0;

    for file in files {
        let items_struct = baml_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::Function(func_loc) = item {
                let signature = function_signature(db, *func_loc);
                globals.insert(signature.name.to_string(), global_idx);
                global_idx += 1;
            }
        }
    }

    // Build classes map (class name -> field name -> field index) and add Class objects to program
    // Also build class_field_types for type inference (class name -> field name -> Ty)
    let mut classes: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut class_field_types: HashMap<Name, HashMap<Name, baml_thir::Ty>> = HashMap::new();
    let mut class_object_indices: HashMap<String, usize> = HashMap::new();

    for file in files {
        let item_tree = baml_hir::file_item_tree(db, *file);
        let items_struct = baml_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::Class(class_loc) = item {
                let class = &item_tree[class_loc.id(db)];
                let class_name = class.name.to_string();

                let mut field_indices = HashMap::new();
                let mut field_types = HashMap::new();
                let mut field_names = Vec::new();
                for (idx, field) in class.fields.iter().enumerate() {
                    field_indices.insert(field.name.to_string(), idx);
                    field_names.push(field.name.to_string());
                    // Lower TypeRef to Ty for type inference
                    let ty = baml_thir::lower_type_ref(db, &field.type_ref);
                    field_types.insert(field.name.clone(), ty);
                }

                // Add Class object to program and record its index
                let class_obj = Object::Class(Class {
                    name: class_name.clone(),
                    field_names,
                });
                let class_obj_idx = program.add_object(class_obj);
                class_object_indices.insert(class_name.clone(), class_obj_idx);

                classes.insert(class_name, field_indices);
                class_field_types.insert(class.name.clone(), field_types);
            }
        }
    }

    // Compile each function
    for file in files {
        let items_struct = baml_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::Function(func_loc) = item {
                let signature = function_signature(db, *func_loc);
                let body = function_body(db, *func_loc);

                // Run type inference
                let inference = baml_thir::infer_function(
                    db,
                    &signature,
                    &body,
                    Some(typing_context.clone()),
                    Some(class_field_types.clone()),
                );

                // Compile to bytecode (objects are added directly to program.objects)
                let ctx = CodegenContext {
                    inference: &inference,
                    globals: &globals,
                    classes: &classes,
                    class_object_indices: &class_object_indices,
                    objects: &mut program.objects,
                };
                let compiled_fn = compile_function(&signature, &body, ctx);

                // Add function object to program
                let fn_obj_idx = program.add_object(Object::Function(compiled_fn));

                // Register in function indices
                program
                    .function_indices
                    .insert(signature.name.to_string(), fn_obj_idx);

                // Add to globals
                program.add_global(Value::Object(ObjectIndex::from_raw(fn_obj_idx)));
            }
        }
    }

    program
}

/// Build typing context from source files.
///
/// Maps function names to their arrow types for use during type inference.
fn build_typing_context<'db>(
    db: &'db dyn baml_thir::Db,
    files: &[SourceFile],
) -> HashMap<Name, baml_thir::Ty<'db>> {
    let mut context = HashMap::new();

    for file in files {
        let items_struct = baml_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::Function(func_loc) = item {
                let signature = function_signature(db, *func_loc);

                // Build the arrow type: (param_types) -> return_type
                let param_types: Vec<baml_thir::Ty<'db>> = signature
                    .params
                    .iter()
                    .map(|p| baml_thir::lower_type_ref(db, &p.type_ref))
                    .collect();

                let return_type = baml_thir::lower_type_ref(db, &signature.return_type);

                let func_type = baml_thir::Ty::Function {
                    params: param_types,
                    ret: Box::new(return_type),
                };

                context.insert(signature.name.clone(), func_type);
            }
        }
    }

    context
}
