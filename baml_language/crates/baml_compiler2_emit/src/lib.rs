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
use baml_compiler2_ast::TypeExpr;
use baml_compiler2_hir::{
    compiler2_all_files,
    file_item_tree,
    file_package::file_package,
    loc::FunctionLoc,
    package::{PackageId, package_items},
};
use baml_compiler2_mir::{lower_function, BuiltinKind, MirFunctionKind};
use baml_type::TyAttr;
use bex_vm_types::{
    Bytecode, Class, ClassField, ClientBuildMeta, ClientBuildType, ConstValue, Enum, EnumVariant,
    Function, FunctionKind, FunctionMeta, Object, ObjectIndex, ObjectPool, Program,
    RetryPolicyMeta,
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
    /// A retry policy field had an invalid value.
    InvalidRetryPolicyValue {
        policy_name: String,
        field_name: String,
        value: String,
        reason: String,
    },
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Internal(msg) => write!(f, "internal lowering error: {msg}"),
            Self::InvalidRetryPolicyValue {
                policy_name,
                field_name,
                value,
                reason,
            } => write!(
                f,
                "retry policy '{policy_name}': invalid value for '{field_name}': '{value}' — {reason}"
            ),
        }
    }
}

impl std::error::Error for LoweringError {}

pub use bex_vm_types::Program as ProgramAlias;

/// Generate bytecode for the entire project.
pub fn generate_project_bytecode(
    db: &dyn baml_compiler2_mir::Db,
    options: &CompileOptions,
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
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let pkg_items = package_items(db, pkg_id);
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
                let field_type = field
                    .type_expr
                    .as_ref()
                    .map(|te| {
                        let mut diags = Vec::new();
                        let tir_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr(
                            db, &te.expr, &pkg_items, &mut diags,
                        );
                        baml_compiler2_mir::convert_tir2_ty(&tir_ty)
                    })
                    .unwrap_or_else(|| baml_type::Ty::Null {
                        attr: baml_type::TyAttr::default(),
                    });
                fields.push(ClassField {
                    name: field.name.to_string(),
                    field_type,
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
            // Register with fully-qualified name for inter-package lookups.
            class_object_indices.insert(fq_name.clone(), class_obj_idx);
            classes.insert(fq_name, field_indices);
            // Also register with the short (unqualified) class name so that MIR aggregates,
            // which store only the local name (e.g., "Point" not "user.Point"), can find it.
            let short_name = class_data.name.to_string();
            class_object_indices.entry(short_name.clone()).or_insert(class_obj_idx);
            classes.entry(short_name).or_insert_with(|| {
                // Rebuild field_indices since we moved it above; re-read from class_data.
                let mut m = HashMap::new();
                for (idx, field) in class_data.fields.iter().enumerate() {
                    m.insert(field.name.to_string(), idx);
                }
                m
            });
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
        for (local_id, func_data) in item_tree.functions.iter() {
            let func_loc = FunctionLoc::new(db, *file, *local_id);
            let mir = lower_function(db, func_loc);
            let fq_name = mir.item_ref.to_string();

            let mut compiled_fn = match &mir.kind {
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

            // Set function metadata from signature
            let (param_names, param_types, return_type) =
                compute_function_metadata_from_item_tree(db, *file, func_data);
            compiled_fn.return_type = return_type;
            compiled_fn.param_names = param_names;
            compiled_fn.param_types = param_types;

            // Set LLM-specific body_meta if this is an LLM function
            if let Some(llm_meta) = &func_data.llm_meta {
                if let (Some(client), Some(prompt)) = (&llm_meta.client, &llm_meta.prompt) {
                    compiled_fn.body_meta = Some(FunctionMeta::Llm {
                        prompt_template: prompt.text.clone(),
                        client: client.to_string(),
                    });
                    compiled_fn.trace = true;
                }
            }

            let fn_obj_idx = program.add_object(Object::Function(Box::new(compiled_fn)));
            program.function_indices.insert(fq_name.clone(), fn_obj_idx);
            let gi = program.globals.len();
            program.function_global_indices.insert(fq_name, gi);
            program.add_global(ConstValue::Object(ObjectIndex::from_raw(fn_obj_idx)));
        }
    }

    // --- Pass 5: Template string macros ---
    let mut template_macros = Vec::new();
    for file in &all_files {
        let item_tree = file_item_tree(db, *file);
        for (_ts_id, ts_data) in item_tree.template_strings.iter() {
            let args = ts_data
                .params
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            if let Some(body) = &ts_data.body {
                template_macros.push(format!(
                    "{{% macro {name}({args}) %}}{body}{{% endmacro %}}",
                    name = ts_data.name,
                ));
            }
        }
    }
    program.template_strings_macros = template_macros.join("\n");

    // --- Pass 6: Retry policies ---
    let mut retry_policies: HashMap<String, RetryPolicyMeta> = HashMap::new();
    for file in &all_files {
        let item_tree = file_item_tree(db, *file);
        for (_rp_id, rp_data) in item_tree.retry_policies.iter() {
            let policy_name = rp_data.name.to_string();
            let meta = RetryPolicyMeta {
                max_retries: parse_retry_policy_field(
                    &policy_name,
                    "max_retries",
                    rp_data.max_retries.as_deref(),
                    0_i64,
                )?,
                initial_delay_ms: parse_retry_policy_field(
                    &policy_name,
                    "initial_delay_ms",
                    rp_data.initial_delay_ms.as_deref(),
                    0_i64,
                )?,
                multiplier: parse_retry_policy_field(
                    &policy_name,
                    "multiplier",
                    rp_data.multiplier.as_deref(),
                    1.0_f64,
                )?,
                max_delay_ms: parse_retry_policy_field(
                    &policy_name,
                    "max_delay_ms",
                    rp_data.max_delay_ms.as_deref(),
                    60_000_i64,
                )?,
            };
            retry_policies.insert(policy_name, meta);
        }
    }

    // --- Pass 7: Client metadata ---
    for file in &all_files {
        let item_tree = file_item_tree(db, *file);
        for (_client_id, client_data) in item_tree.clients.iter() {
            let client_name = client_data.name.to_string();
            let provider = client_data
                .provider
                .as_ref()
                .map(|p| p.as_str())
                .unwrap_or("");

            let client_type = match provider {
                "fallback" => ClientBuildType::Fallback,
                "round-robin" => ClientBuildType::RoundRobin,
                _ => ClientBuildType::Primitive,
            };

            let sub_client_names: Vec<String> = client_data
                .sub_client_names
                .iter()
                .map(|n| n.to_string())
                .collect();

            let retry_policy = client_data
                .retry_policy_name
                .as_ref()
                .and_then(|name| retry_policies.get(name.as_str()).cloned());

            #[allow(clippy::cast_sign_loss)]
            let round_robin_start = client_data.round_robin_start.map(|v| v as i32);

            program.client_metadata.insert(
                client_name,
                ClientBuildMeta {
                    client_type,
                    sub_client_names,
                    retry_policy,
                    round_robin_start,
                },
            );
        }
    }

    // --- Pass 8: Test cases (only when requested) ---
    if options.emit_test_cases {
        for file in &all_files {
            let item_tree = file_item_tree(db, *file);
            for (_test_id, test_data) in item_tree.tests.iter() {
                let function_names: Vec<String> = test_data
                    .function_refs
                    .iter()
                    .map(|n| n.to_string())
                    .collect();
                let args: indexmap::IndexMap<String, bex_vm_types::TestArgValue> = test_data
                    .args
                    .iter()
                    .map(|(k, v)| (k.to_string(), convert_test_arg_value(v)))
                    .collect();
                program.test_cases.push(bex_vm_types::TestCase {
                    name: test_data.name.to_string(),
                    function_names,
                    args,
                });
            }
        }
    }

    Ok(program)
}

/// Parse a retry policy field value, returning a default if absent.
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

/// Convert a compiler2 `TestArgValue` to a `bex_vm_types::TestArgValue`.
fn convert_test_arg_value(
    v: &baml_compiler2_hir::item_tree::TestArgValue,
) -> bex_vm_types::TestArgValue {
    use baml_compiler2_hir::item_tree::TestArgValue as Hir2Arg;
    match v {
        Hir2Arg::Null => bex_vm_types::TestArgValue::Null,
        Hir2Arg::Int(i) => bex_vm_types::TestArgValue::Int(*i),
        Hir2Arg::FloatBits(bits) => bex_vm_types::TestArgValue::Float(f64::from_bits(*bits)),
        Hir2Arg::Bool(b) => bex_vm_types::TestArgValue::Bool(*b),
        Hir2Arg::String(s) => bex_vm_types::TestArgValue::String(s.clone()),
        Hir2Arg::Array(items) => {
            // Use Null element type as placeholder — full type inference not run yet
            bex_vm_types::TestArgValue::Array {
                element_type: baml_type::Ty::Null {
                    attr: baml_type::TyAttr::default(),
                },
                items: items.iter().map(convert_test_arg_value).collect(),
            }
        }
        Hir2Arg::Map(entries) => {
            let converted: indexmap::IndexMap<String, bex_vm_types::TestArgValue> = entries
                .iter()
                .map(|(k, v)| (k.clone(), convert_test_arg_value(v)))
                .collect();
            bex_vm_types::TestArgValue::Map {
                key_type: baml_type::Ty::String {
                    attr: baml_type::TyAttr::default(),
                },
                value_type: baml_type::Ty::Null {
                    attr: baml_type::TyAttr::default(),
                },
                entries: converted,
            }
        }
    }
}

/// Extract param names, param types, and return type from an item_tree Function.
///
/// Type resolution delegates to TIR's `lower_type_expr` (single source of truth)
/// then converts via MIR's `convert_tir2_ty` to produce `baml_type::Ty`.
fn compute_function_metadata_from_item_tree(
    db: &dyn baml_compiler2_mir::Db,
    file: baml_base::SourceFile,
    func_data: &baml_compiler2_hir::item_tree::Function,
) -> (Vec<String>, Vec<baml_type::Ty>, baml_type::Ty) {
    let param_names: Vec<String> = func_data
        .params
        .iter()
        .map(|p| p.name.to_string())
        .collect();

    let pkg_info = file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = package_items(db, pkg_id);
    let null_ty = || baml_type::Ty::Null {
        attr: baml_type::TyAttr::default(),
    };

    let resolve = |te: &TypeExpr| -> baml_type::Ty {
        let mut diags = Vec::new();
        let tir_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr(
            db, te, &pkg_items, &mut diags,
        );
        baml_compiler2_mir::convert_tir2_ty(&tir_ty)
    };

    let param_types: Vec<baml_type::Ty> = func_data
        .params
        .iter()
        .map(|p| {
            p.type_expr
                .as_ref()
                .map(|te| resolve(&te.expr))
                .unwrap_or_else(null_ty)
        })
        .collect();

    let return_type = func_data
        .return_type
        .as_ref()
        .map(|te| resolve(&te.expr))
        .unwrap_or_else(null_ty);

    (param_names, param_types, return_type)
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
