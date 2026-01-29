//! Code generation for BAML.
//!
//! Compiles MIR (Mid-level IR) to bytecode for the BAML VM using stackification.
//!
//! # Architecture
//!
//! The compilation pipeline is:
//! ```text
//! Source -> CST -> HIR -> TIR -> VIR -> MIR -> Bytecode
//! ```
//!
//! This crate handles the final step: MIR -> Bytecode.
//!
//! The compiler classifies MIR locals as Virtual or Real:
//! - **Virtual locals**: Single-use temporaries inlined at use site
//! - **Real locals**: Multi-use or cross-block variables that need stack slots
//!
//! Key modules:
//! - **`analysis`**: Def-use analysis, dominator computation, local classification
//! - **`emit`**: Bytecode emission with stackification optimization

mod analysis;
mod emit;

use bex_vm_types::ObjectPool;
pub(crate) use emit::compile_mir_function;

/// Context for MIR codegen.
///
/// Contains all shared state needed during MIR compilation:
/// global mappings, class information, and the shared object pool.
pub(crate) struct MirCodegenContext<'ctx, 'obj> {
    /// Resolved global names to indices (function names -> global index).
    pub globals: &'ctx HashMap<String, usize>,
    /// Resolved class field indices (class name -> field name -> field index).
    pub classes: &'ctx HashMap<String, HashMap<String, usize>>,
    /// Pre-allocated Class object indices in the program's object pool.
    pub class_object_indices: &'ctx HashMap<String, usize>,
    /// Pre-allocated Enum object indices in the program's object pool.
    pub enum_object_indices: &'ctx HashMap<String, usize>,
    /// Enum variant mappings (enum name -> variant name -> variant index).
    pub enum_variants: &'ctx HashMap<String, HashMap<String, usize>>,
    /// Shared object pool for strings, etc.
    pub objects: &'obj mut ObjectPool,
}

use std::collections::HashMap;

use baml_base::{Name, SourceFile, Span};
use baml_compiler_hir::{
    self, ItemId, function_body, function_signature, function_signature_source_map,
};
use baml_compiler_tir::TypeResolutionContext;
pub use baml_compiler_vir::LoweringError;
pub use bex_vm_types::{
    BinOp, Bytecode, Class, CmpOp, ConstValue, Enum, Function, FunctionKind, GlobalIndex,
    Instruction, Object, ObjectIndex, Program, SysOp, UnaryOp, Value, type_tags,
};

/// Generate bytecode for all functions in a project.
///
/// This is the main entry point for project-wide code generation.
/// It collects all functions from HIR, type-checks them via TIR,
/// lowers to MIR, and compiles to bytecode.
///
/// Returns `Err` if any function contains unrecoverable errors (Missing nodes).
pub fn generate_project_bytecode(db: &dyn baml_compiler_mir::Db) -> Result<Program, LoweringError> {
    let project = db.project();
    compile_files(db, project.files(db))
}

/// Generate bytecode for a list of source files.
///
/// This is useful for testing or when you have a subset of files.
///
/// Returns `Err` if any function contains unrecoverable errors (Missing nodes).
pub fn compile_files(
    db: &dyn baml_compiler_mir::Db,
    files: &[SourceFile],
) -> Result<Program, LoweringError> {
    let mut program = Program::new();
    let project = db.project();

    let resolution_ctx = TypeResolutionContext::new(db, project);

    // Build typing context (maps function names to their types)
    let typing_context = build_typing_context(db, files, &resolution_ctx);

    // Build globals map (function name -> global index)
    // Register builtins first for stable indices, then user functions
    let mut globals: HashMap<String, usize> = HashMap::new();
    let mut global_idx = 0;

    // First, add builtin functions (stable indices 0, 1, 2, ...)
    let builtins = baml_builtins::builtins();
    for path in builtins {
        globals.insert(path.path.to_string(), global_idx);
        global_idx += 1;
    }

    // Then, add user-defined functions
    for file in files {
        let items_struct = baml_compiler_hir::file_items(db, *file);
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
    // Also build class_type_tags for TypeTag switch optimization (class name -> type tag)
    let mut classes: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut class_field_types: HashMap<Name, HashMap<Name, baml_compiler_tir::Ty>> = HashMap::new();
    let mut class_object_indices: HashMap<String, usize> = HashMap::new();
    let mut class_type_tags: HashMap<String, i64> = HashMap::new();
    let mut class_type_tag_counter = 0i64;

    // Inject builtin classes BEFORE user classes for stable indices
    for builtin in baml_builtins::builtin_types() {
        let mut field_names = Vec::new();
        let mut field_indices = HashMap::new();
        let mut field_types = HashMap::new();

        // Include ALL fields (public and private) in runtime field order
        for field in &builtin.fields {
            let idx = field_names.len();
            field_indices.insert(field.name.to_string(), idx);
            field_names.push(field.name.to_string());

            // Only add public fields to field_types (for type checking)
            if !field.is_private {
                if let Some(ref ty_pattern) = field.ty {
                    field_types.insert(
                        Name::new(field.name),
                        baml_compiler_tir::builtins::substitute_unknown(ty_pattern),
                    );
                }
            }
        }

        // Compute type tag for this builtin class
        let type_tag = type_tags::CLASS_BASE + class_type_tag_counter;
        class_type_tags.insert(builtin.path.to_string(), type_tag);

        // Add Class object to program and record its index
        let class_obj = Object::Class(Class {
            name: builtin.path.to_string(),
            field_names,
            type_tag,
        });
        class_type_tag_counter += 1;
        let class_obj_idx = program.add_object(class_obj);
        class_object_indices.insert(builtin.path.to_string(), class_obj_idx);

        classes.insert(builtin.path.to_string(), field_indices);
        class_field_types.insert(Name::new(builtin.path), field_types);
    }

    // Now add user-defined classes
    for file in files {
        let item_tree = baml_compiler_hir::file_item_tree(db, *file);
        let items_struct = baml_compiler_hir::file_items(db, *file);
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
                    let (ty, _) = resolution_ctx.lower_type_ref(&field.type_ref, Span::default());
                    field_types.insert(field.name.clone(), ty);
                }

                // Compute type tag for this class
                let type_tag = type_tags::CLASS_BASE + class_type_tag_counter;
                class_type_tags.insert(class_name.clone(), type_tag);

                // Add Class object to program and record its index
                let class_obj = Object::Class(Class {
                    name: class_name.clone(),
                    field_names,
                    type_tag,
                });
                class_type_tag_counter += 1;
                let class_obj_idx = program.add_object(class_obj);
                class_object_indices.insert(class_name.clone(), class_obj_idx);

                classes.insert(class_name, field_indices);
                class_field_types.insert(class.name.clone(), field_types);
            }
        }
    }

    // Build enums map (enum name -> variant name -> variant index) and add Enum objects to program
    // Also build enum_variant_names for type inference (enum name -> list of variant names)
    let mut enum_variants: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut enum_variant_names: HashMap<Name, Vec<Name>> = HashMap::new();
    let mut enum_object_indices: HashMap<String, usize> = HashMap::new();

    for file in files {
        let item_tree = baml_compiler_hir::file_item_tree(db, *file);
        let items_struct = baml_compiler_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::Enum(enum_loc) = item {
                let enum_def = &item_tree[enum_loc.id(db)];
                let enum_name = enum_def.name.to_string();

                let mut variant_indices = HashMap::new();
                let mut variant_names = Vec::new();
                let mut variant_name_list: Vec<Name> = Vec::new();
                for (idx, variant) in enum_def.variants.iter().enumerate() {
                    variant_indices.insert(variant.name.to_string(), idx);
                    variant_names.push(variant.name.to_string());
                    variant_name_list.push(variant.name.clone());
                }

                // Add Enum object to program and record its index
                let enum_obj = Object::Enum(Enum {
                    name: enum_name.clone(),
                    variant_names,
                });
                let enum_obj_idx = program.add_object(enum_obj);
                enum_object_indices.insert(enum_name.clone(), enum_obj_idx);

                enum_variants.insert(enum_name, variant_indices);
                enum_variant_names.insert(enum_def.name.clone(), variant_name_list);
            }
        }
    }

    // Add builtin functions to globals FIRST (stable indices)
    for builtin in builtins {
        // Sys_op builtins (like file I/O) use FunctionKind::SysOp
        // so the VM knows to dispatch them via DispatchFuture/Await
        let kind = if builtin.is_sys_op {
            let sys_op = sys_op_for_builtin_path(builtin.path)
                .expect("sys_op builtin must have SysOp mapping");
            FunctionKind::SysOp(sys_op)
        } else {
            FunctionKind::NativeUnresolved
        };

        let builtin_fn = Function {
            name: builtin.path.to_string(),
            arity: builtin.arity(),
            bytecode: Bytecode::default(),
            kind,
            locals_in_scope: Vec::new(),
            span: baml_base::Span::fake(),
            block_notifications: Vec::new(),
            viz_nodes: Vec::new(),
        };
        let fn_obj_idx = program.add_object(Object::Function(builtin_fn));
        program.add_global(ConstValue::Object(ObjectIndex::from_raw(fn_obj_idx)));
    }

    // Compile each user function using MIR
    for file in files {
        let items_struct = baml_compiler_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::Function(func_loc) = item {
                let signature = function_signature(db, *func_loc);
                let sig_source_map = function_signature_source_map(db, *func_loc);
                let body = function_body(db, *func_loc);

                // Handle different function body types
                let compiled_fn = match &*body {
                    baml_compiler_hir::FunctionBody::Llm(_) => {
                        // LLM functions have no bytecode - they are dispatched by the embedder
                        let params: Vec<baml_base::Name> =
                            signature.params.iter().map(|p| p.name.clone()).collect();
                        Function {
                            name: signature.name.to_string(),
                            arity: params.len(),
                            bytecode: Bytecode::new(),
                            kind: FunctionKind::SysOp(SysOp::RenderPrompt),
                            locals_in_scope: vec![
                                params
                                    .iter()
                                    .map(std::string::ToString::to_string)
                                    .collect(),
                            ],
                            span: baml_base::Span::fake(),
                            block_notifications: Vec::new(),
                            viz_nodes: Vec::new(),
                        }
                    }
                    baml_compiler_hir::FunctionBody::Missing => {
                        // Missing body - placeholder function
                        let params: Vec<baml_base::Name> =
                            signature.params.iter().map(|p| p.name.clone()).collect();
                        Function {
                            name: signature.name.to_string(),
                            arity: params.len(),
                            bytecode: Bytecode::new(),
                            kind: FunctionKind::Bytecode,
                            locals_in_scope: vec![
                                params
                                    .iter()
                                    .map(std::string::ToString::to_string)
                                    .collect(),
                            ],
                            span: baml_base::Span::fake(),
                            block_notifications: Vec::new(),
                            viz_nodes: Vec::new(),
                        }
                    }
                    baml_compiler_hir::FunctionBody::Expr(_, _) => {
                        // Run type inference
                        // Note: type_aliases is not passed here, so exhaustiveness
                        // checking for type aliases won't work. This is acceptable
                        // since codegen is for runtime execution, and type errors
                        // should be caught in the TIR phase.
                        let inference = baml_compiler_tir::infer_function(
                            db,
                            &signature,
                            Some(&sig_source_map),
                            &body,
                            Some(typing_context.clone()),
                            Some(class_field_types.clone()),
                            None, // type_aliases - not needed for codegen
                            Some(enum_variant_names.clone()), // enum_variants - needed for enum variant detection
                            *func_loc,
                        );

                        // Lower HIR → VIR → MIR
                        // Returns early if there are Missing nodes (errors in source)
                        let vir =
                            baml_compiler_vir::lower_from_hir(&body, &inference, &resolution_ctx)
                                .map_err(|e| e.in_function(signature.name.to_string()))?;
                        let mir = baml_compiler_mir::lower(
                            &signature,
                            &vir,
                            db,
                            &classes,
                            &enum_variants,
                            &class_type_tags,
                            &resolution_ctx,
                        );

                        // Compile MIR to bytecode
                        let ctx = MirCodegenContext {
                            globals: &globals,
                            classes: &classes,
                            class_object_indices: &class_object_indices,
                            enum_object_indices: &enum_object_indices,
                            enum_variants: &enum_variants,
                            objects: &mut program.objects,
                        };
                        compile_mir_function(&mir, ctx)
                    }
                };

                // Add function object to program
                let fn_obj_idx = program.add_object(Object::Function(compiled_fn));

                // Register in function indices
                program
                    .function_indices
                    .insert(signature.name.to_string(), fn_obj_idx);

                // Add to globals
                program.add_global(ConstValue::Object(ObjectIndex::from_raw(fn_obj_idx)));
            }
        }
    }

    Ok(program)
}

/// Build typing context from source files.
///
/// Maps function names to their arrow types for use during type inference.
fn build_typing_context(
    db: &dyn baml_compiler_mir::Db,
    files: &[SourceFile],
    resolution_ctx: &TypeResolutionContext,
) -> HashMap<Name, baml_compiler_tir::Ty> {
    let mut context = HashMap::new();

    for file in files {
        let items_struct = baml_compiler_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::Function(func_loc) = item {
                let signature = function_signature(db, *func_loc);

                // Build the arrow type: (param_types) -> return_type
                let param_types: Vec<baml_compiler_tir::Ty> = signature
                    .params
                    .iter()
                    .map(|p| {
                        resolution_ctx
                            .lower_type_ref(&p.type_ref, Span::default())
                            .0
                    })
                    .collect();

                let (return_type, _) =
                    resolution_ctx.lower_type_ref(&signature.return_type, Span::default());

                let func_type = baml_compiler_tir::Ty::Function {
                    params: param_types,
                    ret: Box::new(return_type),
                };

                context.insert(signature.name.clone(), func_type);
            }
        }
    }

    context
}

/// Map a builtin path to its corresponding `SysOp`.
///
/// This is used during code generation to set the correct `SysOp` variant
/// for `sys_op` builtin functions.
fn sys_op_for_builtin_path(path: &str) -> Option<SysOp> {
    match path {
        // LLM operations
        "baml.llm.PrimitiveClient.render_prompt" => Some(SysOp::RenderPrompt),
        "baml.llm.PrimitiveClient.specialize_prompt" => Some(SysOp::SpecializePrompt),
        // System operations
        "baml.fs.open" => Some(SysOp::FsOpen),
        "baml.fs.File.read" => Some(SysOp::FsRead),
        "baml.fs.File.close" => Some(SysOp::FsClose),
        "baml.sys.shell" => Some(SysOp::Shell),
        "baml.net.connect" => Some(SysOp::NetConnect),
        "baml.net.Socket.read" => Some(SysOp::NetRead),
        "baml.net.Socket.close" => Some(SysOp::NetClose),
        "baml.http.fetch" => Some(SysOp::HttpFetch),
        "baml.http.Response.text" => Some(SysOp::ResponseText),
        "baml.http.Response.ok" => Some(SysOp::ResponseOk),
        _ => None,
    }
}
