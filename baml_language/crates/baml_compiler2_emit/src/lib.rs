//! Code generation for BAML (compiler2 pipeline).
//!
//! Compiles MIR2 to bytecode for the BAML VM using stackification.

mod analysis;
mod emit;
mod pull_semantics;
mod stack_carry;
mod verifier;

pub use analysis::OptLevel;
pub(crate) use emit::compile_mir_function;

use std::collections::HashMap;

use baml_base::Span;
use baml_compiler2_hir::{compiler2_all_files, file_item_tree, file_package::file_package, loc::FunctionLoc};
use baml_compiler2_mir::{lower_function, BuiltinKind, MirFunctionKind};
use baml_type::TyAttr;
use bex_vm_types::{
    Bytecode, Class, ClassField, ConstValue, Enum, EnumVariant, Function, FunctionKind, Object,
    ObjectIndex, ObjectPool,
};

/// Context for MIR codegen.
pub(crate) struct MirCodegenContext<'ctx, 'obj> {
    pub globals: &'ctx HashMap<String, usize>,
    pub classes: &'ctx HashMap<String, HashMap<String, usize>>,
    pub class_object_indices: &'ctx HashMap<String, usize>,
    pub enum_object_indices: &'ctx HashMap<String, usize>,
    pub enum_variants: &'ctx HashMap<String, HashMap<String, usize>>,
    pub objects: &'obj mut ObjectPool,
}

/// Database trait for compiler2 emit queries.
#[salsa::db]
pub trait Db: baml_compiler2_mir::Db {}

/// Compile options.
pub struct CompileOptions {
    pub emit_test_cases: bool,
}

/// Errors that can occur during bytecode generation.
#[derive(Debug)]
pub enum LoweringError {
    /// A stub — no errors expected from Phase 1 stub.
    Internal(String),
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Internal(msg) => write!(f, "internal lowering error: {msg}"),
        }
    }
}

impl std::error::Error for LoweringError {}

pub use bex_vm_types::Program;

/// Generate bytecode for the entire project.
pub fn generate_project_bytecode(
    db: &dyn baml_compiler2_mir::Db,
    _options: &CompileOptions,
) -> Result<Program, LoweringError> {
    let mut program = Program::new();
    let all_files = compiler2_all_files(db);

    // --- Pass 1: Build globals map (function name -> global index) ---
    // We iterate all files, get their functions, and build a stable index map.
    let mut globals: HashMap<String, usize> = HashMap::new();
    let mut global_idx = 0usize;

    for file in &all_files {
        let item_tree = file_item_tree(db, *file);
        for (local_id, _func_data) in item_tree.functions.iter() {
            let func_loc = FunctionLoc::new(db, *file, *local_id);
            let mir = lower_function(db, func_loc);
            let fq_name = mir.item_ref.to_string();
            globals.entry(fq_name).or_insert_with(|| {
                let idx = global_idx;
                global_idx += 1;
                idx
            });
        }
    }

    // --- Pass 2: Build classes table ---
    // Maps fully-qualified class name -> (field name -> field index).
    // Also builds class_object_indices: class fq_name -> object index in program.objects.
    let mut classes: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut class_object_indices: HashMap<String, usize> = HashMap::new();
    let mut class_type_tag_counter = 0i64;

    for file in &all_files {
        let item_tree = file_item_tree(db, *file);
        let pkg_info = file_package(db, *file);
        for (_class_id, class_data) in item_tree.classes.iter() {
            // Build fully-qualified name: "user.MyClass" or "baml.ns.MyClass"
            let fq_name = if pkg_info.namespace_path.is_empty() {
                format!("{}.{}", pkg_info.package, class_data.name)
            } else {
                let ns: Vec<&str> = pkg_info.namespace_path.iter().map(|n| n.as_str()).collect();
                format!("{}.{}.{}", pkg_info.package, ns.join("."), class_data.name)
            };

            let mut field_indices = HashMap::new();
            let mut fields = Vec::new();
            for (idx, field) in class_data.fields.iter().enumerate() {
                field_indices.insert(field.name.to_string(), idx);
                fields.push(ClassField {
                    name: field.name.to_string(),
                    // Phase 2: use Null as placeholder — Phase 3 will fill in real types
                    field_type: baml_type::Ty::Null {
                        attr: baml_type::TyAttr::default(),
                    },
                    description: None,
                    alias: None,
                    field_attr: baml_base::FieldAttr::default(),
                });
            }

            let type_tag = bex_vm_types::type_tags::CLASS_BASE + class_type_tag_counter;
            class_type_tag_counter += 1;

            let class_obj_idx = program.add_object(Object::Class(Class {
                name: fq_name.clone(),
                fields,
                description: None,
                alias: None,
                type_tag,
                ty_attr: TyAttr::default(),
            }));
            class_object_indices.insert(fq_name.clone(), class_obj_idx);
            classes.insert(fq_name, field_indices);
        }
    }

    // --- Pass 3: Build enums table ---
    // Maps fully-qualified enum name -> (variant name -> variant index).
    let mut enum_variants: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut enum_object_indices: HashMap<String, usize> = HashMap::new();

    for file in &all_files {
        let item_tree = file_item_tree(db, *file);
        let pkg_info = file_package(db, *file);
        for (_enum_id, enum_data) in item_tree.enums.iter() {
            let fq_name = if pkg_info.namespace_path.is_empty() {
                format!("{}.{}", pkg_info.package, enum_data.name)
            } else {
                let ns: Vec<&str> = pkg_info.namespace_path.iter().map(|n| n.as_str()).collect();
                format!("{}.{}.{}", pkg_info.package, ns.join("."), enum_data.name)
            };

            let mut variant_map = HashMap::new();
            let mut variants = Vec::new();
            for (idx, variant) in enum_data.variants.iter().enumerate() {
                variant_map.insert(variant.name.to_string(), idx);
                variants.push(EnumVariant {
                    name: variant.name.to_string(),
                    description: None,
                    alias: None,
                    skip: false,
                });
            }

            let enum_obj_idx = program.add_object(Object::Enum(Enum {
                name: fq_name.clone(),
                variants,
                description: None,
                alias: None,
                ty_attr: TyAttr::default(),
            }));
            enum_object_indices.insert(fq_name.clone(), enum_obj_idx);
            enum_variants.insert(fq_name, variant_map);
        }
    }

    // --- Pass 4: Compile each function ---
    for file in &all_files {
        let line_starts = build_line_starts(file.text(db));
        let item_tree = file_item_tree(db, *file);
        for (local_id, _func_data) in item_tree.functions.iter() {
            let func_loc = FunctionLoc::new(db, *file, *local_id);
            let mir = lower_function(db, func_loc);
            let fq_name = mir.item_ref.to_string();

            let compiled_fn = match &mir.kind {
                MirFunctionKind::Bytecode(body) => {
                    let ctx = MirCodegenContext {
                        globals: &globals,
                        classes: &classes,
                        class_object_indices: &class_object_indices,
                        enum_object_indices: &enum_object_indices,
                        enum_variants: &enum_variants,
                        objects: &mut program.objects,
                    };
                    let mut f = compile_mir_function(body, mir.arity, &line_starts, ctx, OptLevel::One);
                    f.name = fq_name.clone();
                    f
                }
                MirFunctionKind::Builtin(BuiltinKind::Io) => {
                    let sys_op = bex_vm_types::sys_op_for_path(&fq_name)
                        .unwrap_or_else(|| panic!("unknown sys_op path: {fq_name}"));
                    Function {
                        name: fq_name.clone(),
                        arity: mir.arity,
                        real_local_count: 0,
                        bytecode: Bytecode::default(),
                        kind: FunctionKind::SysOp(sys_op),
                        local_names: Vec::new(),
                        debug_locals: Vec::new(),
                        span: Span::fake(),
                        block_notifications: Vec::new(),
                        viz_nodes: Vec::new(),
                        return_type: baml_type::Ty::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                        param_names: Vec::new(),
                        param_types: Vec::new(),
                        body_meta: None,
                        trace: false,
                    }
                }
                MirFunctionKind::Builtin(BuiltinKind::Vm) => Function {
                    name: fq_name.clone(),
                    arity: mir.arity,
                    real_local_count: 0,
                    bytecode: Bytecode::default(),
                    kind: FunctionKind::NativeUnresolved,
                    local_names: Vec::new(),
                    debug_locals: Vec::new(),
                    span: Span::fake(),
                    block_notifications: Vec::new(),
                    viz_nodes: Vec::new(),
                    return_type: baml_type::Ty::Null {
                        attr: baml_type::TyAttr::default(),
                    },
                    param_names: Vec::new(),
                    param_types: Vec::new(),
                    body_meta: None,
                    trace: false,
                },
            };

            let fn_obj_idx = program.add_object(Object::Function(Box::new(compiled_fn)));
            program.function_indices.insert(fq_name.clone(), fn_obj_idx);
            let gi = program.globals.len();
            program.function_global_indices.insert(fq_name, gi);
            program.add_global(ConstValue::Object(ObjectIndex::from_raw(fn_obj_idx)));
        }
    }

    Ok(program)
}

/// Build a table of byte offsets where each line starts in the source text.
///
/// Returns `[0, offset_of_line_2, offset_of_line_3, ...]`.
#[allow(clippy::cast_possible_truncation)]
fn build_line_starts(text: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    for (i, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push((i + 1) as u32);
        }
    }
    starts
}
