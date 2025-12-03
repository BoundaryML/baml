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
    BinOp, Bytecode, Class, CmpOp, Enum, Function, FunctionKind, Instruction, Object, Program,
    UnaryOp, Value,
};
use baml_workspace::ProjectRoot;
pub use compiler::{Compiler, compile_function};

/// Generate bytecode for all functions in a project.
///
/// This is the main entry point for project-wide code generation.
/// It collects all functions from HIR, type-checks them via THIR,
/// and compiles them to bytecode.
#[salsa::tracked]
pub fn generate_project_bytecode(db: &dyn baml_thir::Db, root: ProjectRoot) -> Program {
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
                let signature = function_signature(db, *file, *func_loc);
                globals.insert(signature.name.to_string(), global_idx);
                global_idx += 1;
            }
        }
    }

    // Compile each function
    for file in files {
        let items_struct = baml_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::Function(func_loc) = item {
                let signature = function_signature(db, *file, *func_loc);
                let body = function_body(db, *file, *func_loc);

                // Run type inference
                let inference =
                    baml_thir::infer_function(db, &signature, &body, Some(typing_context.clone()));

                // Get parameter names
                let params: Vec<Name> = signature.params.iter().map(|p| p.name.clone()).collect();

                // Compile to bytecode
                let (compiled_fn, objects) = compile_function(
                    signature.name.as_str(),
                    &params,
                    &body,
                    &inference,
                    globals.clone(),
                );

                // Add function object to program
                let fn_obj_idx = program.add_object(Object::Function(compiled_fn));

                // Add all objects from this function to the program's object pool
                for obj in objects {
                    program.add_object(obj);
                }

                // Register in function indices
                program
                    .function_indices
                    .insert(signature.name.to_string(), fn_obj_idx);

                // Add to globals
                program.add_global(Value::Object(fn_obj_idx));
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
                let signature = function_signature(db, *file, *func_loc);

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

#[cfg(test)]
mod tests;
