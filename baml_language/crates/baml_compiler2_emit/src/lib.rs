//! Code generation for BAML (compiler2 pipeline).
//!
//! Compiles MIR2 to bytecode for the BAML VM using stackification.

mod analysis;
mod emit;
mod pull_semantics;
mod stack_carry;
mod verifier;

use std::collections::HashMap;

pub use analysis::OptLevel;
use baml_base::Span;
use baml_compiler2_ast::TypeExpr;
use baml_compiler2_hir::{
    compiler2_all_files,
    contributions::Definition,
    file_item_tree,
    file_package::file_package,
    loc::{FunctionLoc, LetLoc},
    package::{PackageId, package_items},
};
use baml_compiler2_mir::{
    BuiltinKind, MirFunctionKind, def_to_item_ref, lower_function, lower_let_body,
};
use baml_type::TyAttr;
use bex_vm_types::{
    Bytecode, Class, ClassField, ConstValue, Enum, EnumVariant, Function, FunctionKind,
    FunctionMeta, Instruction, Object, ObjectIndex, ObjectPool, Program,
};
pub(crate) use emit::compile_mir_function;

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

pub use bex_vm_types::Program as ProgramAlias;

/// Generate bytecode for the entire project.
pub fn generate_project_bytecode(
    db: &dyn baml_compiler2_mir::Db,
    options: &CompileOptions,
) -> Result<Program, LoweringError> {
    let mut program = Program::new();
    let all_files = compiler2_all_files(db);

    // --- Pass 1: Build globals map (function name -> global index) ---
    // Functions are allocated first (slots 0..N-1), then let bindings (slots N..M-1).
    // This ensures function slots match the order they're appended to program.globals
    // in Pass 4, and let binding slots don't interleave with function slots.
    let mut globals: HashMap<String, usize> = HashMap::new();
    let mut global_idx = 0usize;

    // First sub-pass: assign slots to all functions across all files.
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

    // Second sub-pass: assign slots to all let bindings across all files,
    // after all function slots have been reserved.
    for file in &all_files {
        let item_tree = file_item_tree(db, *file);
        for (local_id, _let_data) in item_tree.lets.iter() {
            let let_loc = LetLoc::new(db, *file, *local_id);
            let fq_name = def_to_item_ref(db, Definition::Let(let_loc)).to_string();
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
                            db,
                            &te.expr,
                            &pkg_items,
                            &[],
                            &mut diags,
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
            class_object_indices
                .entry(short_name.clone())
                .or_insert(class_obj_idx);
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
                    let mut f =
                        compile_mir_function(body, mir.arity, &line_starts, ctx, OptLevel::One);
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
            if let Some(baml_compiler2_ast::DeclarativeMeta::Llm(llm_meta)) =
                &func_data.declarative_meta
            {
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

    // --- Pass 4.5: Populate let-binding global slots and synthesize $init ---
    // Collect all let bindings grouped by package.
    {
        let mut pkg_lets: HashMap<String, Vec<(String, LetLoc, baml_base::SourceFile)>> =
            HashMap::new();
        for file in &all_files {
            let item_tree = file_item_tree(db, *file);
            let pkg_info = file_package(db, *file);
            for (local_id, _let_data) in item_tree.lets.iter() {
                let let_loc = LetLoc::new(db, *file, *local_id);
                let fq_name = def_to_item_ref(db, Definition::Let(let_loc)).to_string();
                // Ensure the global slot for this let binding is populated with Null.
                // Also register in let_global_indices for test/debug visibility.
                if let Some(&slot) = globals.get(&fq_name) {
                    while program.globals.len() <= slot {
                        program.add_global(ConstValue::Null);
                    }
                    program.let_global_indices.insert(fq_name.clone(), slot);
                }
                pkg_lets
                    .entry(pkg_info.package.to_string())
                    .or_default()
                    .push((fq_name, let_loc, *file));
            }
        }

        // Track package init order for runtime.
        // Sort packages: non-user packages first (alphabetical), then "user".
        // This ensures `baml.$init` runs before the user package's `$init`.
        let mut sorted_pkg_names: Vec<&String> = pkg_lets.keys().collect();
        sorted_pkg_names.sort_by(|a, b| match (a.as_str(), b.as_str()) {
            ("user", _) => std::cmp::Ordering::Greater,
            (_, "user") => std::cmp::Ordering::Less,
            (a, b) => a.cmp(b),
        });
        let mut package_init_order: Vec<String> = Vec::new();

        for pkg_name in &sorted_pkg_names {
            let let_bindings = &pkg_lets[*pkg_name];
            if let_bindings.is_empty() {
                continue;
            }

            // Topologically sort the bindings (detect circular deps).
            let sorted_bindings = topological_sort_lets(db, let_bindings)?;

            // Compile all let-binding initializers into a single $init function.
            let init_fn = compile_init_function(
                db,
                &sorted_bindings,
                &globals,
                &classes,
                &class_object_indices,
                &enum_object_indices,
                &enum_variants,
                &mut program,
            )?;

            let init_fq_name = if pkg_name.as_str() == "user" {
                "$init".to_string()
            } else {
                format!("{pkg_name}.$init")
            };

            let fn_obj_idx = program.add_object(Object::Function(Box::new(init_fn)));
            program
                .function_indices
                .insert(init_fq_name.clone(), fn_obj_idx);
            let gi = program.globals.len();
            program
                .function_global_indices
                .insert(init_fq_name.clone(), gi);
            program.add_global(ConstValue::Object(ObjectIndex::from_raw(fn_obj_idx)));

            package_init_order.push(init_fq_name);
        }

        program.package_init_order = package_init_order;
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
    // Retry policies are now synthesized as Item::Let bindings during CST lowering.
    // Their values flow through the $init pipeline instead of being parsed here.
    // Pass 6 is intentionally empty.

    // Client metadata is now synthesized as Item::Let bindings during CST lowering.
    // Client values (including sub-clients, retry policies) flow through the $init pipeline.
    // Pass 7 is intentionally empty.

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
            db,
            te,
            &pkg_items,
            &[],
            &mut diags,
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

// ─── Let-binding helpers ─────────────────────────────────────────────────────

/// Topologically sort let bindings by their dependencies.
///
/// Walks each binding's `ExprBody` to find `Expr::Path` references to other
/// let bindings in the same package, then runs Kahn's algorithm. Returns an
/// error if circular dependencies are detected.
fn topological_sort_lets<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    bindings: &[(String, LetLoc<'db>, baml_base::SourceFile)],
) -> Result<Vec<(String, LetLoc<'db>, baml_base::SourceFile)>, LoweringError> {
    use std::collections::{HashSet, VecDeque};

    // Build adjacency list: binding[i] depends on (needs) binding[j]
    let mut deps: Vec<HashSet<usize>> = vec![HashSet::new(); bindings.len()];
    for (i, (_name, let_loc, _file)) in bindings.iter().enumerate() {
        let body = baml_compiler2_hir::body::let_body(db, *let_loc);
        if let baml_compiler2_hir::body::LetBody::Expr(expr_body) = body.as_ref() {
            // Walk all expressions to find path references to other let bindings.
            for (_expr_id, expr) in expr_body.exprs.iter() {
                if let baml_compiler2_ast::Expr::Path(segments) = expr {
                    // Single-segment paths might reference another let binding.
                    if segments.len() == 1 {
                        let ref_name_short = segments[0].as_str();
                        for (j, (fq, _, _)) in bindings.iter().enumerate() {
                            if j != i && fq.ends_with(&format!(".{ref_name_short}")) {
                                deps[i].insert(j);
                            }
                        }
                    }
                }
            }
        }
    }

    // Kahn's algorithm: if A depends on B, B must come first.
    // Build reverse edges (used_by) and in-degree (dep count).
    let mut in_degree: Vec<usize> = deps.iter().map(|d| d.len()).collect();
    let mut reverse_deps: Vec<Vec<usize>> = vec![Vec::new(); bindings.len()];
    for (i, dep_set) in deps.iter().enumerate() {
        for &j in dep_set {
            reverse_deps[j].push(i);
        }
    }

    let mut queue: VecDeque<usize> = VecDeque::new();
    for (i, &deg) in in_degree.iter().enumerate() {
        if deg == 0 {
            queue.push_back(i);
        }
    }

    let mut sorted = Vec::with_capacity(bindings.len());
    while let Some(node) = queue.pop_front() {
        sorted.push(node);
        for &dependent in &reverse_deps[node] {
            in_degree[dependent] -= 1;
            if in_degree[dependent] == 0 {
                queue.push_back(dependent);
            }
        }
    }

    if sorted.len() != bindings.len() {
        return Err(LoweringError::Internal(
            "Circular dependency detected among top-level let bindings".to_string(),
        ));
    }

    Ok(sorted.into_iter().map(|i| bindings[i].clone()).collect())
}

/// Compile the `$init` function that evaluates all let-binding initializers
/// in dependency order, storing each result via `StoreGlobal`.
///
/// Strategy: for each let binding, lower the initializer through MIR → bytecode
/// as a standalone zero-arg helper function. Register the helper in globals
/// (for `Call` addressability), then emit a `$init` body that calls each helper
/// and `StoreGlobal`s the result into the let binding's global slot.
fn compile_init_function<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    sorted_bindings: &[(String, LetLoc<'db>, baml_base::SourceFile)],
    globals: &HashMap<String, usize>,
    classes: &HashMap<String, HashMap<String, usize>>,
    class_object_indices: &HashMap<String, usize>,
    enum_object_indices: &HashMap<String, usize>,
    enum_variants: &HashMap<String, HashMap<String, usize>>,
    program: &mut Program,
) -> Result<Function, LoweringError> {
    // Build the $init bytecode: a sequence of Call + StoreGlobal pairs.
    let mut init_instructions: Vec<Instruction> = Vec::new();
    let mut init_constants: Vec<bex_vm_types::ConstValue> = Vec::new();

    for (i, (fq_name, let_loc, file)) in sorted_bindings.iter().enumerate() {
        // Find the global slot for this let binding.
        let let_slot = match globals.get(fq_name.as_str()) {
            Some(&slot) => slot,
            None => {
                return Err(LoweringError::Internal(format!(
                    "no global slot for let binding: {fq_name}"
                )));
            }
        };

        // Lower the let initializer through MIR → MirFunctionBody.
        let maybe_body = lower_let_body(db, *let_loc);

        let helper_fn = match maybe_body {
            Some(mir_body) => {
                let line_starts = build_line_starts(file.text(db));
                let ctx = MirCodegenContext {
                    globals,
                    classes,
                    class_object_indices,
                    enum_object_indices,
                    enum_variants,
                    objects: &mut program.objects,
                };
                let mut helper =
                    compile_mir_function(&mir_body, 0, &line_starts, ctx, OptLevel::One);
                helper.name = format!("$init_let_{i}");
                helper.arity = 0;
                helper
            }
            None => {
                // No initializer — helper just pushes Null.
                let mut bytecode = Bytecode::default();
                // LoadConst(0) → Null constant
                bytecode.constants.push(bex_vm_types::ConstValue::Null);
                bytecode.instructions.push(Instruction::LoadConst(0));
                bytecode.instructions.push(Instruction::Return);
                Function {
                    name: format!("$init_let_{i}"),
                    arity: 0,
                    real_local_count: 0,
                    bytecode,
                    kind: FunctionKind::Bytecode,
                    local_names: Vec::new(),
                    debug_locals: Vec::new(),
                    span: baml_base::Span::fake(),
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
        };

        // Register the helper function as an object and a global slot.
        // This lets $init call it via Call(global_idx).
        let helper_obj_idx = program.add_object(Object::Function(Box::new(helper_fn)));
        let helper_global_slot = program.globals.len();
        program.add_global(bex_vm_types::ConstValue::Object(ObjectIndex::from_raw(
            helper_obj_idx,
        )));

        // Emit: Call(helper_global_slot) then StoreGlobal(let_slot)
        init_instructions.push(Instruction::Call(bex_vm_types::GlobalIndex::from_raw(
            helper_global_slot,
        )));
        init_instructions.push(Instruction::StoreGlobal(
            bex_vm_types::GlobalIndex::from_raw(let_slot),
        ));
    }

    // Final: push Null and Return (Return pops the top of the eval stack).
    let null_const_idx = init_constants.len();
    init_constants.push(bex_vm_types::ConstValue::Null);
    init_instructions.push(Instruction::LoadConst(null_const_idx));
    init_instructions.push(Instruction::Return);

    let mut bytecode = Bytecode::default();
    bytecode.instructions = init_instructions;
    bytecode.constants = init_constants;

    Ok(Function {
        name: "$init".to_string(),
        arity: 0,
        real_local_count: 0,
        bytecode,
        kind: FunctionKind::Bytecode,
        local_names: Vec::new(),
        debug_locals: Vec::new(),
        span: baml_base::Span::fake(),
        block_notifications: Vec::new(),
        viz_nodes: Vec::new(),
        return_type: baml_type::Ty::Null {
            attr: baml_type::TyAttr::default(),
        },
        param_names: Vec::new(),
        param_types: Vec::new(),
        body_meta: None,
        trace: false,
    })
}
