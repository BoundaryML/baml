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
mod pull_semantics;
mod stack_carry;

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

use std::collections::{HashMap, HashSet};

use baml_base::{Name, SourceFile, Span};
use baml_compiler_hir::{
    self, ItemId, function_body, function_qualified_name, function_signature,
    function_signature_source_map, template_string_body, template_string_signature,
};
use baml_compiler_tir::TypeResolutionContext;
pub use baml_compiler_vir::LoweringError;
pub use bex_vm_types::{
    BinOp, Bytecode, Class, ClassField, CmpOp, ConstValue, Enum, EnumVariant, Function,
    FunctionKind, GlobalIndex, Instruction, Object, ObjectIndex, Program, SysOp, UnaryOp, Value,
    type_tags,
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
    // Note: Builtin BAML files (like llm.baml) are now loaded at project setup time
    // in ProjectDatabase::set_project_root(), so they're already in the files list.

    let mut program = Program::new();
    let project = db.project();

    let resolution_ctx = TypeResolutionContext::new(db, project);

    // Get type aliases for VIR lowering
    let type_aliases = baml_compiler_tir::type_aliases(db, project)
        .aliases(db)
        .clone();
    let recursive_aliases = baml_compiler_tir::find_recursive_aliases(&type_aliases);

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

    // Then, add user-defined functions (including builtin BAML files)
    // Use function_qualified_name to get the proper namespaced name for builtins
    for file in files {
        let items_struct = baml_compiler_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::Function(func_loc) = item {
                let qualified_name = baml_compiler_hir::function_qualified_name(db, *func_loc);
                let func_name = qualified_name.display();
                globals.insert(func_name, global_idx);
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
        let mut fields = Vec::new();
        let mut field_indices = HashMap::new();
        let mut field_types = HashMap::new();

        // Include ALL fields (public and private) in runtime field order
        for field in &builtin.fields {
            let idx = fields.len();
            field_indices.insert(field.name.to_string(), idx);

            // Determine the Ty for this field
            let tir_ty = baml_compiler_tir::builtins::substitute_unknown(&field.ty);
            let field_ty = baml_type::convert_tir_ty(&tir_ty, &type_aliases, &recursive_aliases)
                .and_then(baml_type::sanitize_for_runtime)
                .unwrap_or(baml_type::Ty::Null);

            fields.push(ClassField {
                name: field.name.to_string(),
                field_type: field_ty,
                description: None,
                alias: None,
            });

            // Only add public fields to field_types (for type checking)
            if !field.is_private {
                field_types.insert(
                    Name::new(field.name),
                    baml_compiler_tir::builtins::substitute_unknown(&field.ty),
                );
            }
        }

        // Compute type tag for this builtin class
        let type_tag = type_tags::CLASS_BASE + class_type_tag_counter;
        class_type_tags.insert(builtin.path.to_string(), type_tag);

        // Add Class object to program and record its index
        let class_obj = Object::Class(Class {
            name: builtin.path.to_string(),
            fields,
            description: None,
            alias: None,
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
                // Use FQN for builtin-file classes (e.g., "baml.llm.OrchestrationStep"),
                // short name for user classes (e.g., "MyClass").
                let fqn = baml_compiler_hir::class_qualified_name(db, *class_loc);
                let class_name = fqn.display();

                let mut field_indices = HashMap::new();
                let mut field_types = HashMap::new();
                let mut fields = Vec::new();
                // Filter @skip fields to match schema_map.rs behavior
                let non_skip_fields: Vec<_> = class
                    .fields
                    .iter()
                    .filter(|f| !f.skip.is_explicit())
                    .collect();
                for (idx, field) in non_skip_fields.iter().enumerate() {
                    field_indices.insert(field.name.to_string(), idx);
                    // Lower TypeRef to Ty for type inference
                    let (ty, _) = resolution_ctx.lower_type_ref(&field.type_ref, Span::default());
                    field_types.insert(field.name.clone(), ty.clone());

                    // Convert TIR Ty to baml_type::Ty for runtime
                    let runtime_ty =
                        baml_type::convert_tir_ty(&ty, &type_aliases, &recursive_aliases)
                            .and_then(baml_type::sanitize_for_runtime)
                            .unwrap_or(baml_type::Ty::Null);

                    fields.push(ClassField {
                        name: field.name.to_string(),
                        field_type: runtime_ty,
                        description: field.description.value().cloned(),
                        alias: field.alias.value().cloned(),
                    });
                }

                // Compute type tag for this class
                let type_tag = type_tags::CLASS_BASE + class_type_tag_counter;
                class_type_tags.insert(class_name.clone(), type_tag);

                // Add Class object to program and record its index
                let class_obj = Object::Class(Class {
                    name: class_name.clone(),
                    fields,
                    description: class.description.value().cloned(),
                    alias: class.alias.value().cloned(),
                    type_tag,
                });
                class_type_tag_counter += 1;
                let class_obj_idx = program.add_object(class_obj);
                class_object_indices.insert(class_name.clone(), class_obj_idx);

                classes.insert(class_name.clone(), field_indices);
                class_field_types.insert(Name::new(&class_name), field_types);
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
                let mut variants = Vec::new();
                let mut variant_name_list: Vec<Name> = Vec::new();
                for (idx, variant) in enum_def.variants.iter().enumerate() {
                    variant_indices.insert(variant.name.to_string(), idx);
                    variants.push(EnumVariant {
                        name: variant.name.to_string(),
                        description: variant.description.value().cloned(),
                        alias: variant.alias.value().cloned(),
                        skip: variant.skip.is_explicit(),
                    });
                    variant_name_list.push(variant.name.clone());
                }

                // Add Enum object to program and record its index
                let enum_obj = Object::Enum(Enum {
                    name: enum_name.clone(),
                    variants,
                    description: None, // HIR Enum doesn't carry description
                    alias: enum_def.alias.value().cloned(),
                });
                let enum_obj_idx = program.add_object(enum_obj);
                enum_object_indices.insert(enum_name.clone(), enum_obj_idx);

                enum_variants.insert(enum_name, variant_indices);
                enum_variant_names.insert(enum_def.name.clone(), variant_name_list);
            }
        }
    }

    // Add builtin enums (e.g., baml.llm.ClientType) to the program.
    // These are not declared in user BAML files but are needed at runtime
    // when sys_ops return values containing builtin enum variants.
    for builtin_enum in baml_builtins::builtin_enums() {
        let variants: Vec<EnumVariant> = builtin_enum
            .variants
            .iter()
            .map(|v| EnumVariant {
                name: v.to_string(),
                description: None,
                alias: None,
                skip: false,
            })
            .collect();
        let mut variant_indices = HashMap::new();
        for (idx, v) in builtin_enum.variants.iter().enumerate() {
            variant_indices.insert(v.to_string(), idx);
        }
        let enum_obj = Object::Enum(Enum {
            name: builtin_enum.path.to_string(),
            variants,
            description: None,
            alias: None,
        });
        let enum_obj_idx = program.add_object(enum_obj);
        enum_object_indices.insert(builtin_enum.path.to_string(), enum_obj_idx);
        // Use FQN as key (e.g., "baml.llm.ClientType") — matches the FQN in enum_names.
        enum_variants.insert(builtin_enum.path.to_string(), variant_indices);

        // Also add to enum_variant_names for type inference (keyed by FQN)
        let variant_name_list: Vec<Name> = builtin_enum
            .variants
            .iter()
            .map(|v| Name::new(*v))
            .collect();
        enum_variant_names.insert(Name::new(builtin_enum.path), variant_name_list);
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

        let tir_ty = baml_compiler_tir::builtins::substitute_unknown(&builtin.returns);
        let return_type = baml_type::convert_tir_ty(&tir_ty, &type_aliases, &recursive_aliases)
            .and_then(baml_type::sanitize_for_runtime)
            .unwrap_or(baml_type::Ty::Null);

        let builtin_fn = Function {
            name: builtin.path.to_string(),
            arity: builtin.arity(),
            bytecode: Bytecode::default(),
            kind,
            locals_in_scope: Vec::new(),
            span: baml_base::Span::fake(),
            block_notifications: Vec::new(),
            viz_nodes: Vec::new(),
            return_type,
            param_names: Vec::new(),
            param_types: Vec::new(),
            body_meta: None,
            trace: false,
        };
        let fn_obj_idx = program.add_object(Object::Function(Box::new(builtin_fn)));
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

                // Get the qualified name - for builtin files this includes the namespace
                // e.g., "baml.llm.render_prompt" for functions in <builtin>/baml/llm.baml
                let qualified_name = function_qualified_name(db, *func_loc);
                let func_name = qualified_name.display();

                // Compute metadata once for all body types
                let (meta_param_names, meta_param_types, meta_return_type) =
                    compute_function_metadata(
                        &signature,
                        &resolution_ctx,
                        &type_aliases,
                        &recursive_aliases,
                    );

                // Handle different function body types
                let mut compiled_fn = match &*body {
                    baml_compiler_hir::FunctionBody::Llm(_) => {
                        // LLM functions are now lowered to synthetic Expr bodies
                        // during HIR lowering. This branch should be unreachable.
                        unreachable!(
                            "FunctionBody::Llm should have been converted to Expr during HIR lowering"
                        );
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
                            return_type: baml_type::Ty::Null,
                            param_names: Vec::new(),
                            param_types: Vec::new(),
                            body_meta: None,
                            trace: false,
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
                        let vir = baml_compiler_vir::lower_from_hir(
                            &body,
                            &inference,
                            &resolution_ctx,
                            &type_aliases,
                            &recursive_aliases,
                        )
                        .map_err(|e| e.in_function(signature.name.to_string()))?;
                        let mir = baml_compiler_mir::lower(
                            &signature,
                            &vir,
                            db,
                            &classes,
                            &enum_variants,
                            &class_type_tags,
                            &resolution_ctx,
                            &type_aliases,
                            &recursive_aliases,
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

                // Always set metadata (overwrite placeholder for Expr, redundant for Missing)
                compiled_fn.return_type = meta_return_type;
                compiled_fn.param_names = meta_param_names;
                compiled_fn.param_types = meta_param_types;

                // If this is an LLM function, attach prompt/client metadata and enable tracing
                if let Some(llm_meta) = baml_compiler_hir::llm_function_meta(db, *func_loc) {
                    compiled_fn.body_meta = Some(bex_vm_types::FunctionMeta::Llm {
                        prompt_template: llm_meta.prompt.text.clone(),
                        client: llm_meta.client.to_string(),
                    });
                    compiled_fn.trace = true;
                }

                // Validate types at emit time (safety net)
                debug_assert!(
                    compiled_fn.return_type.validate_runtime().is_ok(),
                    "Compiler-only type leaked to runtime return type: {}",
                    compiled_fn.return_type
                );
                for pt in &compiled_fn.param_types {
                    debug_assert!(
                        pt.validate_runtime().is_ok(),
                        "Compiler-only type leaked to runtime param type: {pt}"
                    );
                }

                // Update function name if it's a builtin
                compiled_fn.name.clone_from(&func_name);

                // Add function object to program
                let fn_obj_idx = program.add_object(Object::Function(Box::new(compiled_fn)));

                // Register in function indices
                program
                    .function_indices
                    .insert(func_name.clone(), fn_obj_idx);

                // Track global index before adding
                let global_idx = program.globals.len();
                program
                    .function_global_indices
                    .insert(func_name.clone(), global_idx);

                // Add to globals
                program.add_global(ConstValue::Object(ObjectIndex::from_raw(fn_obj_idx)));
            }
        }
    }

    // --- Pass: Format template_string macros ---
    let mut template_macros = Vec::new();
    for file in files {
        let items_struct = baml_compiler_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::TemplateString(ts_loc) = item {
                let signature = template_string_signature(db, *ts_loc);
                let body = template_string_body(db, *ts_loc);
                let args = signature
                    .params
                    .iter()
                    .map(|p| p.name.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                template_macros.push(format!(
                    "{{% macro {name}({args}) %}}{body}{{% endmacro %}}",
                    name = signature.name,
                    body = body.text,
                ));
            }
        }
    }
    program.template_strings_macros = template_macros.join("\n");

    // --- Pass: Extract client and retry policy metadata ---
    // First, collect all retry policies by name
    let mut retry_policies: HashMap<String, bex_vm_types::RetryPolicyMeta> = HashMap::new();
    for file in files {
        let item_tree = baml_compiler_hir::file_item_tree(db, *file);
        let items_struct = baml_compiler_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::RetryPolicy(rp_loc) = item {
                let rp = &item_tree[rp_loc.id(db)];
                let policy_name = rp.name.to_string();
                retry_policies.insert(
                    policy_name.clone(),
                    bex_vm_types::RetryPolicyMeta {
                        max_retries: parse_retry_policy_field(
                            &policy_name,
                            "max_retries",
                            rp.max_retries.as_deref(),
                            0_i64,
                        )?,
                        initial_delay_ms: parse_retry_policy_field(
                            &policy_name,
                            "initial_delay_ms",
                            rp.initial_delay_ms.as_deref(),
                            0_i64,
                        )?,
                        multiplier: parse_retry_policy_field(
                            &policy_name,
                            "multiplier",
                            rp.multiplier.as_deref(),
                            1.0_f64,
                        )?,
                        max_delay_ms: parse_retry_policy_field(
                            &policy_name,
                            "max_delay_ms",
                            rp.max_delay_ms.as_deref(),
                            60_000_i64,
                        )?,
                    },
                );
            }
        }
    }

    // Then, collect all clients with their metadata
    for file in files {
        let item_tree = baml_compiler_hir::file_item_tree(db, *file);
        let items_struct = baml_compiler_hir::file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::Client(client_loc) = item {
                let client = &item_tree[client_loc.id(db)];
                let client_name = client.name.to_string();
                let provider = client.provider.as_str();

                let client_type = match provider {
                    "fallback" => bex_vm_types::ClientBuildType::Fallback,
                    "round-robin" => bex_vm_types::ClientBuildType::RoundRobin,
                    _ => bex_vm_types::ClientBuildType::Primitive,
                };

                let sub_client_names: Vec<String> = client
                    .sub_client_names
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect();

                let retry_policy = client
                    .retry_policy_name
                    .as_ref()
                    .and_then(|name| retry_policies.get(name.as_str()).cloned());

                program.client_metadata.insert(
                    client_name.clone(),
                    bex_vm_types::ClientBuildMeta {
                        client_type,
                        sub_client_names,
                        retry_policy,
                        round_robin_start: client.round_robin_start,
                    },
                );
            }
        }
    }

    Ok(program)
}

fn parse_retry_policy_field<T>(
    policy_name: &str,
    field_name: &str,
    raw_value: Option<&str>,
    default: T,
) -> Result<T, LoweringError>
where
    T: std::str::FromStr + Copy,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    match raw_value {
        None => Ok(default),
        Some(value) => value
            .parse::<T>()
            .map_err(|e| LoweringError::InvalidRetryPolicyValue {
                policy_name: policy_name.to_string(),
                field_name: field_name.to_string(),
                value: value.to_string(),
                reason: e.to_string(),
            }),
    }
}

/// Extract param names, param types, and return type from a function signature.
///
/// Performs the standard `lower_type_ref` → `convert_tir_ty` → `sanitize_for_runtime`
/// pipeline for each parameter and the return type.
fn compute_function_metadata(
    signature: &baml_compiler_hir::FunctionSignature,
    resolution_ctx: &TypeResolutionContext,
    type_aliases: &HashMap<Name, baml_compiler_tir::Ty>,
    recursive_aliases: &HashSet<Name>,
) -> (Vec<String>, Vec<baml_type::Ty>, baml_type::Ty) {
    let param_names: Vec<String> = signature
        .params
        .iter()
        .map(|p| p.name.to_string())
        .collect();
    let param_types: Vec<baml_type::Ty> = signature
        .params
        .iter()
        .map(|p| {
            let (ty, _) = resolution_ctx.lower_type_ref(&p.type_ref, Span::default());
            baml_type::convert_tir_ty(&ty, type_aliases, recursive_aliases)
                .and_then(baml_type::sanitize_for_runtime)
                .unwrap_or(baml_type::Ty::Null)
        })
        .collect();
    let (ret_ty, _) = resolution_ctx.lower_type_ref(&signature.return_type, Span::default());
    let return_type = baml_type::convert_tir_ty(&ret_ty, type_aliases, recursive_aliases)
        .and_then(baml_type::sanitize_for_runtime)
        .unwrap_or(baml_type::Ty::Null);
    (param_names, param_types, return_type)
}

/// Build typing context from source files.
///
/// Maps function names (qualified for builtins) to their arrow types for type inference.
/// Functions from builtin files (e.g., `<builtin>/baml/llm.baml`) are registered with
/// their qualified names (e.g., `baml.llm.render_prompt`).
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

                // Get the qualified name - for builtin files this includes the namespace
                let qualified_name = baml_compiler_hir::function_qualified_name(db, *func_loc);

                // Build the arrow type: (param_types) -> return_type
                let params: Vec<(Option<Name>, baml_compiler_tir::Ty)> = signature
                    .params
                    .iter()
                    .map(|p| {
                        let ty = resolution_ctx
                            .lower_type_ref(&p.type_ref, Span::default())
                            .0;
                        (Some(p.name.clone()), ty)
                    })
                    .collect();

                let (return_type, _) =
                    resolution_ctx.lower_type_ref(&signature.return_type, Span::default());

                let func_type = baml_compiler_tir::Ty::Function {
                    params,
                    ret: Box::new(return_type),
                };

                // Use the display name as the key (e.g., "baml.llm.render_prompt" or "my_func")
                context.insert(qualified_name.display_name(), func_type);
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
    // Delegate to the generated function from bex_vm_types, which is
    // derived from the same #[sys_op] definitions in with_builtins!.
    bex_vm_types::sys_op_for_path(path)
}
