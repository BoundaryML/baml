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
use baml_base::{Name, Span};
use baml_compiler2_ast::TypeExpr;
use baml_compiler2_hir::{
    compiler2_all_files,
    contributions::Definition,
    file_package::file_package,
    loc::{FunctionLoc, LetLoc},
    package::PackageId,
};
use baml_compiler2_mir::{
    BuiltinKind, MirFunctionKind, ResolvedAliases, def_to_item_ref, lower_function, lower_let_body,
};
// Use the PPIR item tree (which includes synthetic *$stream items) rather than
// the bare HIR item tree, to stay consistent with TIR's LocalItemId indices.
use baml_compiler2_ppir::file_item_tree;
use baml_type::TyAttr;
use bex_vm_types::{
    Bytecode, Class, ClassField, ConstValue, Enum, EnumVariant, Function, FunctionKind,
    FunctionMeta, FunctionOrigin, Instruction, Object, ObjectIndex, ObjectPool, Program,
};

/// Build a per-package `ResolvedAliases` cache, keyed by package name.
fn build_alias_caches(
    db: &dyn baml_compiler2_mir::Db,
    all_files: &[baml_base::SourceFile],
) -> HashMap<Name, ResolvedAliases> {
    let mut caches: HashMap<Name, ResolvedAliases> = HashMap::new();
    for file in all_files {
        let pkg_info = file_package(db, *file);
        caches.entry(pkg_info.package.clone()).or_insert_with(|| {
            let pkg_id = PackageId::new(db, pkg_info.package.clone());
            ResolvedAliases::for_package(db, pkg_id)
        });
    }
    caches
}
pub(crate) use emit::compile_mir_function;

fn is_builtin_function_name(name: &str) -> bool {
    matches!(
        name.split('.').next(),
        Some("baml" | "assert" | "testing" | "log" | "env")
    )
}

fn emitted_function_origin(
    fq_name: &str,
    origin: baml_compiler2_ast::FunctionOrigin,
) -> FunctionOrigin {
    if is_builtin_function_name(fq_name) {
        FunctionOrigin::Builtin
    } else {
        match origin {
            baml_compiler2_ast::FunctionOrigin::UserDefined => FunctionOrigin::UserDefined,
            baml_compiler2_ast::FunctionOrigin::Companion => FunctionOrigin::Companion,
            baml_compiler2_ast::FunctionOrigin::Internal => FunctionOrigin::Internal,
            baml_compiler2_ast::FunctionOrigin::AutoDerive => FunctionOrigin::AutoDerive,
        }
    }
}

/// Context for MIR codegen.
pub(crate) struct MirCodegenContext<'ctx, 'obj> {
    pub globals: &'ctx HashMap<String, usize>,
    pub classes: &'ctx HashMap<String, HashMap<String, usize>>,
    pub class_object_indices: &'ctx HashMap<String, usize>,
    pub enum_object_indices: &'ctx HashMap<String, usize>,
    pub enum_variants: &'ctx HashMap<String, HashMap<String, usize>>,
    pub objects: &'obj mut ObjectPool,
    /// Maps MIR lambda index → `ObjectPool` index of the compiled lambda `Function`.
    /// Parallel to `lambda_names`. Empty for non-lambda functions.
    pub lambda_object_indices: &'ctx [usize],
    /// Lambda debug names, parallel to `lambda_object_indices`.
    pub lambda_names: &'ctx [String],
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

/// Parse a string attribute value, handling both regular strings (`"text"`)
/// and raw strings (`#"text"#`, `##"text"##`, etc.).
///
/// Returns `None` if the value is not a recognized string literal.
fn parse_string_attr_value(raw: &str) -> Option<String> {
    // Double-quoted string: "text"
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        return Some(baml_compiler2_ast::unescape_string_literal(
            &raw[1..raw.len() - 1],
        ));
    }
    // Single-quoted string: 'text'
    if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
        return Some(baml_compiler2_ast::unescape_string_literal(
            &raw[1..raw.len() - 1],
        ));
    }

    // Raw string: #"text"#, ##"text"##, etc.
    let hash_count = raw.bytes().take_while(|&b| b == b'#').count();
    if hash_count == 0 {
        return None;
    }

    let rest = &raw[hash_count..];
    let closing = format!("\"{}", &raw[..hash_count]);

    // Need at least `"` + `"` + closing hashes
    if rest.len() < hash_count + 2 || !rest.starts_with('"') || !rest.ends_with(&closing) {
        return None;
    }

    // Raw strings: no escape processing
    Some(rest[1..rest.len() - 1 - hash_count].to_string())
}

/// Extract `@description`, `@alias`, `@skip` from span-free HIR attributes.
///
/// Returns `(description, alias, skip)`. Invalid attribute usage is diagnosed
/// at HIR validation time; by this point, malformed attrs are simply skipped.
fn extract_schema_attrs(
    attrs: &[baml_compiler2_hir::item_tree::Attribute],
) -> (Option<String>, Option<String>, bool) {
    let mut description = None;
    let mut alias = None;
    let mut skip = false;
    for attr in attrs {
        match attr.name.as_str() {
            "description" | "alias" => {
                if attr.args.len() == 1 {
                    let raw = attr.args[0].value.as_str();
                    let value = parse_string_attr_value(raw);
                    if attr.name.as_str() == "description" {
                        description = value;
                    } else {
                        alias = value;
                    }
                }
            }
            "skip" => {
                skip = true;
            }
            _ => {}
        }
    }
    (description, alias, skip)
}

pub use bex_vm_types::Program as ProgramAlias;

/// Build a `TypeName` from a fully-qualified dotted path.
///
/// Emit always fully qualifies — `display_name` keeps the literal package
/// prefix (`"user.Point"`, `"baml.http.Response"`, `"<vendor>.<…>"`). The
/// codegen-output Python and the runtime see the same `<pkg>.<…>` form
/// end-to-end. See `12a-namespace-rules.md §5` for the rationale.
fn fq_to_type_name(fq: &str) -> baml_type::TypeName {
    let segments: Vec<&str> = fq.split('.').collect();
    let name = baml_base::Name::new(*segments.last().expect("non-empty fq name"));
    let module_path = segments[..segments.len() - 1]
        .iter()
        .map(|s| baml_base::Name::new(*s))
        .collect();
    baml_type::TypeName {
        name,
        module_path,
        display_name: baml_base::Name::new(fq),
    }
}

/// Generate bytecode for the entire project (default: `OptLevel::Two`).
pub fn generate_project_bytecode(
    db: &dyn baml_compiler2_mir::Db,
    options: &CompileOptions,
) -> Result<Program, LoweringError> {
    generate_project_bytecode_with_opt(db, options, OptLevel::Two)
}

/// Generate bytecode for the entire project with a specific optimization level.
pub fn generate_project_bytecode_with_opt(
    db: &dyn baml_compiler2_mir::Db,
    options: &CompileOptions,
    opt: OptLevel,
) -> Result<Program, LoweringError> {
    let mut program = Program::new();
    let all_files = compiler2_all_files(db);
    let alias_caches = build_alias_caches(db, &all_files);

    // --- Pass 1: Build globals map (function name -> global index) ---
    // Functions are allocated first (slots 0..N-1), then let bindings (slots N..M-1).
    // This ensures function slots match the order they're appended to program.globals
    // in Pass 4, and let binding slots don't interleave with function slots.
    let mut globals: HashMap<String, usize> = HashMap::new();
    let mut global_idx = 0usize;

    // First sub-pass: assign slots to all functions across all files.
    // Intrinsic functions are skipped: they are lowered to StatementKind::Intrinsic
    // at call sites and never appear as callable objects in the globals pool.
    // Including them here would create a mismatch between Pass-1 indices and the
    // actual program.globals array built in Pass 4 (which also skips intrinsics).
    for file in &all_files {
        let item_tree = file_item_tree(db, *file);
        for local_id in item_tree.functions.keys() {
            let func_loc = FunctionLoc::new(db, *file, *local_id);
            let mir = lower_function(db, func_loc, opt);
            // Skip intrinsic functions — they are never called via Call instruction.
            if matches!(mir.kind, MirFunctionKind::Builtin(BuiltinKind::Intrinsic)) {
                continue;
            }
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
        for local_id in item_tree.lets.keys() {
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
        let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        let cache = &alias_caches[&pkg_info.package];
        for class_data in item_tree.classes.values() {
            // Build fully-qualified name: "user.MyClass" or "baml.ns.MyClass"
            let fq_name = if pkg_info.namespace_path.is_empty() {
                format!("{}.{}", pkg_info.package, class_data.name)
            } else {
                let ns: Vec<&str> = pkg_info
                    .namespace_path
                    .iter()
                    .map(baml_base::Name::as_str)
                    .collect();
                format!("{}.{}.{}", pkg_info.package, ns.join("."), class_data.name)
            };

            let mut field_indices = HashMap::new();
            let mut fields = Vec::new();
            // Class-level generic params, used to resolve `T`-references in
            // field type expressions to `TyTemplate::TypeArgRef(N)`.  When
            // empty, `tir2_to_template` produces `TyTemplate::Concrete(...)`
            // for every leaf and `field_template == Concrete(field_type)`.
            let class_generic_params: Vec<baml_base::Name> = class_data.generic_params.clone();
            for (idx, field) in class_data.fields.iter().enumerate() {
                field_indices.insert(field.name.to_string(), idx);
                let (field_type, field_template) = match field.type_expr.as_ref() {
                    Some(te) => {
                        let mut diags = Vec::new();
                        // Pass `class_generic_params` as the binding context so
                        // `T`-references inside `class Container<T> { item: T }`
                        // lower to `Tir2Ty::TypeVar("T")` rather than
                        // `Tir2Ty::Unknown`.  This is the input both to the
                        // erased-`Ty` (TypeVar→Void) used by codegen and to
                        // the `TyTemplate` (TypeVar→TypeArgRef(N)) used by
                        // typed runtime walking.
                        let tir_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                            db,
                            &te.expr,
                            pkg_items,
                            &pkg_info.namespace_path,
                            &class_generic_params,
                            &mut diags,
                        );
                        let resolved_ty = cache.convert(&tir_ty);
                        let template = baml_compiler2_mir::tir2_to_template(
                            &tir_ty,
                            cache,
                            &class_generic_params,
                        );
                        (resolved_ty, template)
                    }
                    None => {
                        let null_ty = baml_type::Ty::Null {
                            attr: baml_type::TyAttr::default(),
                        };
                        (null_ty.clone(), baml_type::TyTemplate::Concrete(null_ty))
                    }
                };
                let (field_desc, field_alias, field_skip) = extract_schema_attrs(&field.attributes);
                fields.push(ClassField {
                    name: field.name.to_string(),
                    field_type,
                    field_template,
                    description: field_desc,
                    alias: field_alias,
                    skip: field_skip,
                });
            }

            let (class_desc, class_alias, _class_skip) =
                extract_schema_attrs(&class_data.attributes);

            let type_tag = bex_vm_types::type_tags::CLASS_BASE + class_type_tag_counter;
            class_type_tag_counter += 1;

            let class_obj_idx = program.add_object(Object::Class(Box::new(Class {
                name: fq_to_type_name(&fq_name),
                fields,
                description: class_desc,
                alias: class_alias,
                type_tag,
                ty_attr: TyAttr::default(),
            })));
            // Register with fully-qualified name for inter-package lookups.
            class_object_indices.insert(fq_name.clone(), class_obj_idx);
            classes.insert(fq_name.clone(), field_indices);
            // MIR TypeName display for user-defined classes omits the `user.`
            // package prefix in diagnostics/snapshots. Register the same key
            // so emit-time type checks can do a direct display-name lookup.
            let display_name = if pkg_info.package.as_str() == "user" {
                if pkg_info.namespace_path.is_empty() {
                    class_data.name.to_string()
                } else {
                    let ns: Vec<&str> = pkg_info
                        .namespace_path
                        .iter()
                        .map(baml_base::Name::as_str)
                        .collect();
                    format!("{}.{}", ns.join("."), class_data.name)
                }
            } else {
                fq_name.clone()
            };
            class_object_indices
                .entry(display_name.clone())
                .or_insert(class_obj_idx);
            // Also register with the short (unqualified) class name so that MIR aggregates,
            // which store only the local name (e.g., "Point" not "user.Point"), can find it.
            let short_name = class_data.name.to_string();
            class_object_indices
                .entry(short_name.clone())
                .or_insert(class_obj_idx);
            classes.entry(display_name).or_insert_with(|| {
                let mut m = HashMap::new();
                for (idx, field) in class_data.fields.iter().enumerate() {
                    m.insert(field.name.to_string(), idx);
                }
                m
            });
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
        for enum_data in item_tree.enums.values() {
            let fq_name = if pkg_info.namespace_path.is_empty() {
                format!("{}.{}", pkg_info.package, enum_data.name)
            } else {
                let ns: Vec<&str> = pkg_info
                    .namespace_path
                    .iter()
                    .map(baml_base::Name::as_str)
                    .collect();
                format!("{}.{}.{}", pkg_info.package, ns.join("."), enum_data.name)
            };

            let mut variant_map = HashMap::new();
            let mut variants = Vec::new();
            for (idx, variant) in enum_data.variants.iter().enumerate() {
                let (var_desc, var_alias, var_skip) = extract_schema_attrs(&variant.attributes);
                variant_map.insert(variant.name.to_string(), idx);
                variants.push(EnumVariant {
                    name: variant.name.to_string(),
                    description: var_desc,
                    alias: var_alias,
                    skip: var_skip,
                });
            }

            let (enum_desc, enum_alias, _enum_skip) = extract_schema_attrs(&enum_data.attributes);

            let enum_obj_idx = program.add_object(Object::Enum(Box::new(Enum {
                name: fq_to_type_name(&fq_name),
                variants,
                description: enum_desc,
                alias: enum_alias,
                ty_attr: TyAttr::default(),
            })));
            enum_object_indices.insert(fq_name.clone(), enum_obj_idx);
            enum_variants.insert(fq_name, variant_map);
        }
    }

    // --- Pass 4: Compile each function ---
    for file in &all_files {
        let line_starts = build_line_starts(file.text(db));
        let item_tree = file_item_tree(db, *file);
        let pkg_info_pass4 = file_package(db, *file);
        let cache_pass4 = &alias_caches[&pkg_info_pass4.package];
        for (local_id, func_data) in &item_tree.functions {
            let func_loc = FunctionLoc::new(db, *file, *local_id);
            let mir = lower_function(db, func_loc, opt);
            let fq_name = mir.item_ref.to_string();

            let mut compiled_fn = match &mir.kind {
                MirFunctionKind::Bytecode(body) => {
                    // Compile lambda children first, collecting their ObjectPool indices.
                    let source_file = file.path(db).display().to_string();
                    let lambda_info = compile_lambdas_flat(
                        &mir.lambdas,
                        &line_starts,
                        &source_file,
                        &globals,
                        &classes,
                        &class_object_indices,
                        &enum_object_indices,
                        &enum_variants,
                        &mut program.objects,
                        opt,
                    );
                    let lambda_obj_indices: Vec<usize> =
                        lambda_info.iter().map(|(idx, _)| *idx).collect();
                    let lambda_names_vec: Vec<String> =
                        lambda_info.iter().map(|(_, name)| name.clone()).collect();
                    let ctx = MirCodegenContext {
                        globals: &globals,
                        classes: &classes,
                        class_object_indices: &class_object_indices,
                        enum_object_indices: &enum_object_indices,
                        enum_variants: &enum_variants,
                        objects: &mut program.objects,
                        lambda_object_indices: &lambda_obj_indices,
                        lambda_names: &lambda_names_vec,
                    };
                    let mut f =
                        compile_mir_function(body, mir.arity, mir.span, &line_starts, ctx, opt);
                    f.name.clone_from(&fq_name);
                    f.source_file.clone_from(&source_file);
                    f
                }
                MirFunctionKind::Builtin(BuiltinKind::Intrinsic) => {
                    // Intrinsic functions have no callable body — call sites use
                    // StatementKind::Intrinsic directly. Skip compilation entirely.
                    continue;
                }
                MirFunctionKind::Builtin(BuiltinKind::Io) => {
                    let sys_op = bex_vm_types::sys_op_for_path(&fq_name)
                        .unwrap_or_else(|| panic!("unknown sys_op path: {fq_name}"));
                    Function {
                        name: fq_name.clone(),
                        source_file: String::new(), // builtins have no source file
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
                        stream_return_type: baml_type::Ty::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                        param_names: Vec::new(),
                        param_types: Vec::new(),
                        param_has_default: Vec::new(),
                        throws_type: None,
                        origin: FunctionOrigin::Builtin,
                        body_meta: None,
                        trace: false,
                    }
                }
                MirFunctionKind::Builtin(BuiltinKind::Vm) => Function {
                    name: fq_name.clone(),
                    source_file: String::new(), // builtins have no source file
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
                    stream_return_type: baml_type::Ty::Null {
                        attr: baml_type::TyAttr::default(),
                    },
                    param_names: Vec::new(),
                    param_types: Vec::new(),
                    param_has_default: Vec::new(),
                    throws_type: None,
                    origin: FunctionOrigin::Builtin,
                    body_meta: None,
                    trace: false,
                },
            };

            // Set function metadata from signature
            let parameter_defaults = baml_compiler2_ppir::function_parameter_defaults(db, func_loc);
            let (param_names, param_types, param_has_default, return_type) =
                compute_function_metadata_from_item_tree(
                    db,
                    *file,
                    *local_id,
                    func_data,
                    &parameter_defaults,
                    cache_pass4,
                );
            compiled_fn.return_type = return_type;
            compiled_fn.param_names = param_names;
            compiled_fn.param_types = param_types;
            compiled_fn.param_has_default = param_has_default;

            // Set inferred throws type from TIR throw inference
            compiled_fn.throws_type = compute_throws_type(db, *file, &func_data.name, cache_pass4);
            compiled_fn.origin = emitted_function_origin(&fq_name, func_data.origin);

            // Set LLM-specific body_meta if this is an LLM function
            if let Some(baml_compiler2_ast::DeclarativeMeta::Llm(llm_meta)) =
                &func_data.declarative_meta
            {
                // Look up the PPIR's pre-computed stream-expanded return type.
                let expansion = baml_compiler2_ppir::ppir_expansion_items(db, *file);
                for (name, stream_te) in expansion.stream_return_types(db) {
                    if *name == fq_name {
                        compiled_fn.stream_return_type =
                            compute_stream_return_type(db, *file, stream_te, cache_pass4);
                        break;
                    }
                }

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
            program
                .function_global_indices
                .insert(fq_name, program.globals.len());
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
            for local_id in item_tree.lets.keys() {
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
        // Topological sort: dependencies before dependents.
        let pkg_name_list: Vec<baml_base::Name> =
            pkg_lets.keys().map(baml_base::Name::new).collect();
        let sorted_pkg_name_owned = topological_sort_packages(db, &pkg_name_list);
        let sorted_pkg_names: Vec<&String> = sorted_pkg_name_owned
            .iter()
            .filter_map(|name| pkg_lets.keys().find(|k| k.as_str() == name.as_str()))
            .collect();
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
                opt,
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

    // --- Pass 4.6: Synthesize $init_test chainer per package ---
    // Cross-file aggregation of per-file $init_test_<N> functions.
    // This must happen at emit level because:
    //   - AST layer (lower_file_with_file_id) is per-file only
    //   - MIR (lower_function) is per-function only
    //   - Only emit iterates all_files and has the compiled program
    // Follows the exact $init pattern at Pass 4.5 above.
    {
        // Discover per-file $init_test_<N> functions using structured
        // compiler metadata (HIR item trees), group by package.
        let mut pkg_init_tests: HashMap<String, Vec<(String, usize)>> = HashMap::new();

        for file in &all_files {
            let item_tree = file_item_tree(db, *file);
            let pkg_info = file_package(db, *file);
            for local_id in item_tree.functions.keys() {
                let func_loc = FunctionLoc::new(db, *file, *local_id);
                let fq_name = def_to_item_ref(db, Definition::Function(func_loc)).to_string();
                // Match per-file $init_test_<N> functions synthesized by
                // lower_cst.rs:912-972. The trailing underscore in the filter
                // is intentional: all real files produce `$init_test_{file_id}`
                // with a numeric suffix. The sentinel FileId path in lower_cst.rs
                // produces bare `$init_test` (no suffix), but that only runs in
                // unit tests, PPIR intermediate processing, and codegen — none of
                // which produce functions that reach program.function_indices at
                // emit time. So `contains("$init_test_")` safely matches only
                // per-file functions without risk of collision with the chainer
                // name we're about to synthesize.
                if fq_name.contains("$init_test_") {
                    if let Some(&global_slot) = program.function_global_indices.get(&fq_name) {
                        pkg_init_tests
                            .entry(pkg_info.package.to_string())
                            .or_default()
                            .push((fq_name, global_slot));
                    }
                }
            }
        }

        for (pkg_name, init_test_fns) in &pkg_init_tests {
            if init_test_fns.is_empty() {
                continue;
            }

            // Build bytecode: for each per-file $init_test_<N>, emit:
            //   LoadVar 1              // push registry param (slot 1 = first arg; slot 0 is reserved for fn ref)
            //   Call($init_test_<N>)   // call per-file init with registry
            //   Pop 1                  // discard null return
            let mut instructions = Vec::new();
            let mut constants: Vec<bex_vm_types::ConstValue> = Vec::new();
            for (_name, global_slot) in init_test_fns {
                instructions.push(Instruction::LoadVar(1)); // slot 1 = first param ("registry")
                instructions.push(Instruction::Call {
                    callee: bex_vm_types::GlobalIndex::from_raw(*global_slot),
                    ntypeargs: 0,
                });
                instructions.push(Instruction::Pop(1));
            }
            // Return null
            let null_const_idx = constants.len();
            constants.push(bex_vm_types::ConstValue::Null);
            instructions.push(Instruction::LoadConst(null_const_idx));
            instructions.push(Instruction::Return);

            let bytecode = Bytecode {
                instructions,
                constants,
                ..Bytecode::default()
            };

            // Synthesized function uses Span::fake() — same pattern as
            // $init synthesis at compile_init_function (lib.rs:1085-1103).
            let chainer_fn = Function {
                name: "$init_test".to_string(),
                source_file: String::new(), // synthesized, no source file
                arity: 1,
                real_local_count: 1, // the registry param
                bytecode,
                kind: FunctionKind::Bytecode,
                // local_names is indexed by slot number:
                //   slot 0 = fn ref (reserved, empty string placeholder)
                //   slot 1 = first param "registry"
                local_names: vec![String::new(), "registry".to_string()],
                debug_locals: Vec::new(),
                span: Span::fake(),
                block_notifications: Vec::new(),
                viz_nodes: Vec::new(),
                return_type: baml_type::Ty::Null {
                    attr: baml_type::TyAttr::default(),
                },
                stream_return_type: baml_type::Ty::Null {
                    attr: baml_type::TyAttr::default(),
                },
                param_names: vec!["registry".to_string()],
                param_types: vec![baml_type::Ty::unknown()], // type not needed for chainer dispatch
                param_has_default: vec![false],
                throws_type: None,
                origin: FunctionOrigin::Internal,
                body_meta: None,
                trace: false,
            };

            let chainer_name = if pkg_name.as_str() == "user" {
                "$init_test".to_string()
            } else {
                format!("{pkg_name}.$init_test")
            };

            let fn_obj_idx = program.add_object(Object::Function(Box::new(chainer_fn)));
            program
                .function_indices
                .insert(chainer_name.clone(), fn_obj_idx);
            let gi = program.globals.len();
            program
                .function_global_indices
                .insert(chainer_name.clone(), gi);
            program.add_global(ConstValue::Object(ObjectIndex::from_raw(fn_obj_idx)));
        }
    }

    // --- Pass 5: Template string macros ---
    let mut template_macros = Vec::new();
    for file in &all_files {
        let item_tree = file_item_tree(db, *file);
        for ts_data in item_tree.template_strings.values() {
            let args = ts_data
                .params
                .iter()
                .map(|param| param.name.to_string())
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

    // --- Pass 7.5: Recursive type alias definitions (ctx.output_format bridge) ---
    // Mirrors the legacy pipeline: only recursive aliases are stored in
    // `Program.recursive_type_alias_defs`; non-recursive aliases are expanded inline
    // by `convert_tir2_ty`. This is required for correct output_format rendering at runtime.
    for cache in alias_caches.values() {
        for (qtn, tir_ty) in &cache.aliases {
            if cache.recursive.contains(qtn) {
                let mir_ty = cache.convert(tir_ty);
                let type_name = baml_compiler2_mir::qtn_to_type_name(qtn);
                program.recursive_type_alias_defs.insert(type_name, mir_ty);
            }
        }
    }

    // --- Pass 8: Test cases (only when requested) ---
    if options.emit_test_cases {
        for file in &all_files {
            let item_tree = file_item_tree(db, *file);
            for test_data in item_tree.tests.values() {
                let function_names: Vec<String> = test_data
                    .function_refs
                    .iter()
                    .map(ToString::to_string)
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

/// Compute the inferred throws type for a function by querying TIR throw inference.
///
/// Returns `Some(ty)` if the function (or its callees) may throw, `None` otherwise.
fn compute_throws_type(
    db: &dyn baml_compiler2_mir::Db,
    file: baml_base::SourceFile,
    func_name: &baml_base::Name,
    cache: &ResolvedAliases,
) -> Option<baml_type::Ty> {
    let pkg_info = file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package);
    let throw_sets = baml_compiler2_tir::throw_inference::function_throw_sets(db, pkg_id);

    let key =
        baml_compiler2_tir::throw_inference::throw_set_key(&pkg_info.namespace_path, func_name);

    let facts = throw_sets.transitive_for(&key)?;
    if facts.is_empty() {
        return None;
    }

    let converted: Vec<baml_type::Ty> = facts.iter().map(|tir_ty| cache.convert(tir_ty)).collect();

    if converted.len() == 1 {
        Some(converted.into_iter().next().unwrap())
    } else {
        Some(baml_type::Ty::Union(
            converted,
            baml_type::TyAttr::default(),
        ))
    }
}

/// Extract param names, param types, and return type from an `item_tree` Function.
///
/// Type resolution delegates to TIR's `lower_type_expr` (single source of truth)
/// then converts via MIR's `convert_tir2_ty` to produce `baml_type::Ty`.
fn compute_function_metadata_from_item_tree(
    db: &dyn baml_compiler2_mir::Db,
    file: baml_base::SourceFile,
    func_id: baml_compiler2_hir::ids::LocalItemId<baml_compiler2_hir::ids::FunctionMarker>,
    func_data: &baml_compiler2_hir::item_tree::Function,
    parameter_defaults: &baml_compiler2_hir::signature::FunctionParameterDefaults,
    cache: &ResolvedAliases,
) -> (Vec<String>, Vec<baml_type::Ty>, Vec<bool>, baml_type::Ty) {
    let param_names: Vec<String> = func_data
        .params
        .iter()
        .map(|p| p.name.to_string())
        .collect();

    let pkg_info = file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package);
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let null_ty = || baml_type::Ty::Null {
        attr: baml_type::TyAttr::default(),
    };

    // For methods on generic classes, the class-level generic params are in
    // scope inside the method signature.  Without them, type references like
    // `S` in `Stream<T, S>.next(self) -> S | StreamFinished` route through
    // `route_name_to_unknown` and erase to `Ty::Void`, breaking the runtime's
    // FFI-boundary return-type check.  Mirror
    // `MirLowerer::enclosing_generic_params`: class params come first, then
    // function-level params.
    let enclosing_generics: Vec<baml_base::Name> = {
        let item_tree = file_item_tree(db, file);
        let mut params: Vec<baml_base::Name> = item_tree
            .classes
            .values()
            .find(|class_data| class_data.methods.contains(&func_id))
            .map(|class_data| class_data.generic_params.clone())
            .unwrap_or_default();
        params.extend(func_data.generic_params.iter().cloned());
        params
    };

    let resolve = |te: &TypeExpr| -> baml_type::Ty {
        let mut diags = Vec::new();
        // Use `lower_type_expr_in_ns` so unqualified references (e.g. `MyLorem`
        // in a function signature under `ns_lorem/`) resolve against the
        // defining file's namespace before falling back to the package root.
        // `lower_type_expr` passes `&[]` as the ns context, which would lose
        // parameter types to `Ty::Unknown` → `Ty::Void` for any non-root-ns
        // class — surfacing as "expected instance, got map" in the runtime
        // because the coercion layer can't see the declared type.
        let tir_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
            db,
            te,
            pkg_items,
            &pkg_info.namespace_path,
            &enclosing_generics,
            &mut diags,
        );
        cache.convert(&tir_ty)
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

    let param_has_default: Vec<bool> = parameter_defaults
        .params
        .iter()
        .map(Option::is_some)
        .collect();

    let return_type = func_data
        .return_type
        .as_ref()
        .map(|te| resolve(&te.expr))
        .unwrap_or_else(null_ty);

    (param_names, param_types, param_has_default, return_type)
}

/// Lower a PPIR-computed stream-expanded `TypeExpr` to `baml_type::Ty`.
///
/// Reuses the same TIR lowering + MIR conversion pipeline as
/// `compute_function_metadata_from_item_tree`.
fn compute_stream_return_type(
    db: &dyn baml_compiler2_mir::Db,
    file: baml_base::SourceFile,
    type_expr: &baml_compiler2_ast::TypeExpr,
    cache: &ResolvedAliases,
) -> baml_type::Ty {
    use baml_compiler2_hir::{file_package::file_package, package::PackageId};

    let pkg_info = file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let mut diags = Vec::new();
    let tir_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr(
        db,
        type_expr,
        pkg_items,
        &pkg_info.namespace_path,
        &mut diags,
    );
    // Diagnostics are intentionally discarded here — same as
    // compute_function_metadata_from_item_tree. Type errors in stream-expanded
    // types are reported upstream by TIR's infer_scope_types via
    // builder.report_at_span().
    baml_compiler2_mir::convert_tir2_ty(&tir_ty, cache)
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

/// Topological sort of packages: dependencies come before dependents.
/// Falls back to alphabetical order for packages at the same depth.
fn topological_sort_packages(
    db: &dyn baml_compiler2_tir::Db,
    pkg_names: &[baml_base::Name],
) -> Vec<baml_base::Name> {
    use std::collections::{HashMap, VecDeque};

    use baml_compiler2_hir::package::{PackageId, package_dependencies};

    let pkg_set: std::collections::HashSet<&baml_base::Name> = pkg_names.iter().collect();
    let mut in_degree: HashMap<baml_base::Name, usize> = HashMap::new();
    let mut dependents: HashMap<baml_base::Name, Vec<baml_base::Name>> = HashMap::new();

    for name in pkg_names {
        in_degree.entry(name.clone()).or_insert(0);
        let pkg_id = PackageId::new(db, name.clone());
        for dep_id in package_dependencies(db, pkg_id) {
            let dep_name = dep_id.name(db).clone();
            if pkg_set.contains(&dep_name) {
                *in_degree.entry(name.clone()).or_insert(0) += 1;
                dependents.entry(dep_name).or_default().push(name.clone());
            }
        }
    }

    // Kahn's algorithm
    let mut initial: Vec<baml_base::Name> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(name, _)| name.clone())
        .collect();
    initial.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut queue: VecDeque<baml_base::Name> = initial.into_iter().collect();

    let mut result = Vec::new();
    while let Some(name) = queue.pop_front() {
        result.push(name.clone());
        if let Some(deps) = dependents.get(&name) {
            let mut next = Vec::new();
            for dep in deps {
                let deg = in_degree.get_mut(dep).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    next.push(dep.clone());
                }
            }
            next.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            queue.extend(next);
        }
    }

    result
}

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
    let mut in_degree: Vec<usize> = deps.iter().map(HashSet::len).collect();
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

/// Compile a flat list of lambda `MirFunction`s into bytecode `Function` objects
/// and register them in `objects`.  Returns a parallel `Vec<(obj_idx, name)>`
/// that can be used to build `lambda_object_indices` and `lambda_names` for the
/// parent function's `MirCodegenContext`.
///
/// "Flat" means we do NOT recurse into nested lambda children here — Phase 3
/// only supports lambdas at one level of nesting inside a top-level function.
/// Nested lambda support (lambdas inside lambdas) comes in a later phase.
#[allow(clippy::too_many_arguments)]
fn compile_lambdas_flat(
    lambdas: &[baml_compiler2_mir::MirFunction],
    line_starts: &[u32],
    source_file: &str,
    globals: &HashMap<String, usize>,
    classes: &HashMap<String, HashMap<String, usize>>,
    class_object_indices: &HashMap<String, usize>,
    enum_object_indices: &HashMap<String, usize>,
    enum_variants: &HashMap<String, HashMap<String, usize>>,
    objects: &mut ObjectPool,
    opt: OptLevel,
) -> Vec<(usize, String)> {
    let mut result = Vec::with_capacity(lambdas.len());
    for lambda in lambdas {
        let lambda_name = lambda.item_ref.to_string();
        let obj_idx = match &lambda.kind {
            MirFunctionKind::Bytecode(body) => {
                // Recursively compile any nested lambdas within this lambda.
                let nested_info = compile_lambdas_flat(
                    &lambda.lambdas,
                    line_starts,
                    source_file,
                    globals,
                    classes,
                    class_object_indices,
                    enum_object_indices,
                    enum_variants,
                    objects,
                    opt,
                );
                let nested_obj_indices: Vec<usize> =
                    nested_info.iter().map(|(idx, _)| *idx).collect();
                let nested_names: Vec<String> =
                    nested_info.iter().map(|(_, name)| name.clone()).collect();
                let ctx = MirCodegenContext {
                    globals,
                    classes,
                    class_object_indices,
                    enum_object_indices,
                    enum_variants,
                    objects,
                    lambda_object_indices: &nested_obj_indices,
                    lambda_names: &nested_names,
                };
                let mut f =
                    compile_mir_function(body, lambda.arity, lambda.span, line_starts, ctx, opt);
                f.name.clone_from(&lambda_name);
                f.source_file = source_file.to_string();
                let idx = objects.len();
                objects.push(Object::Function(Box::new(f)));
                idx
            }
            MirFunctionKind::Builtin(_) => {
                // Builtins can't be lambdas — skip.
                continue;
            }
        };
        result.push((obj_idx, lambda_name));
    }
    result
}

/// Compile the `$init` function that evaluates all let-binding initializers
/// in dependency order, storing each result via `StoreGlobal`.
///
/// Strategy: for each let binding, lower the initializer through MIR → bytecode
/// as a standalone zero-arg helper function. Register the helper in globals
/// (for `Call` addressability), then emit a `$init` body that calls each helper
/// and `StoreGlobal`s the result into the let binding's global slot.
#[allow(clippy::too_many_arguments)]
fn compile_init_function<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    sorted_bindings: &[(String, LetLoc<'db>, baml_base::SourceFile)],
    globals: &HashMap<String, usize>,
    classes: &HashMap<String, HashMap<String, usize>>,
    class_object_indices: &HashMap<String, usize>,
    enum_object_indices: &HashMap<String, usize>,
    enum_variants: &HashMap<String, HashMap<String, usize>>,
    program: &mut Program,
    opt: OptLevel,
) -> Result<Function, LoweringError> {
    // Build the $init bytecode: a sequence of Call + StoreGlobal pairs.
    let mut init_instructions: Vec<Instruction> = Vec::new();
    let mut init_meta: Vec<bex_vm_types::bytecode::InstructionMeta> = Vec::new();
    let mut init_constants: Vec<bex_vm_types::ConstValue> = Vec::new();

    for (i, (fq_name, let_loc, file)) in sorted_bindings.iter().enumerate() {
        // Find the global slot for this let binding.
        let Some(&let_slot) = globals.get(fq_name.as_str()) else {
            return Err(LoweringError::Internal(format!(
                "no global slot for let binding: {fq_name}"
            )));
        };

        // Lower the let initializer through MIR → MirFunctionBody (+ any lambda children).
        let maybe_body = lower_let_body(db, *let_loc, opt);

        let helper_fn = match maybe_body {
            Some((mir_body, lambdas)) => {
                let line_starts = build_line_starts(file.text(db));
                // Compile lambda children first and collect their object indices.
                let source_file = file.path(db).display().to_string();
                let lambda_info = compile_lambdas_flat(
                    &lambdas,
                    &line_starts,
                    &source_file,
                    globals,
                    classes,
                    class_object_indices,
                    enum_object_indices,
                    enum_variants,
                    &mut program.objects,
                    opt,
                );
                let lambda_let_obj_indices: Vec<usize> =
                    lambda_info.iter().map(|(idx, _)| *idx).collect();
                let lambda_let_names: Vec<String> =
                    lambda_info.iter().map(|(_, name)| name.clone()).collect();
                let ctx = MirCodegenContext {
                    globals,
                    classes,
                    class_object_indices,
                    enum_object_indices,
                    enum_variants,
                    objects: &mut program.objects,
                    lambda_object_indices: &lambda_let_obj_indices,
                    lambda_names: &lambda_let_names,
                };
                let mut helper = compile_mir_function(&mir_body, 0, None, &line_starts, ctx, opt);
                helper.name = format!("$init_let_{i}");
                helper.source_file.clone_from(&source_file);
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
                    source_file: String::new(), // synthesized, no source file
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
                    stream_return_type: baml_type::Ty::Null {
                        attr: baml_type::TyAttr::default(),
                    },
                    param_names: Vec::new(),
                    param_types: Vec::new(),
                    param_has_default: Vec::new(),
                    throws_type: None,
                    origin: FunctionOrigin::Internal,
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
        init_instructions.push(Instruction::Call {
            callee: bex_vm_types::GlobalIndex::from_raw(helper_global_slot),
            ntypeargs: 0,
        });
        init_meta.push(bex_vm_types::bytecode::InstructionMeta {
            operand: Some(bex_vm_types::bytecode::OperandMeta::Callable(format!(
                "$init_let_{i}"
            ))),
        });
        init_instructions.push(Instruction::StoreGlobal(
            bex_vm_types::GlobalIndex::from_raw(let_slot),
        ));
        init_meta.push(bex_vm_types::bytecode::InstructionMeta {
            operand: Some(bex_vm_types::bytecode::OperandMeta::Global(fq_name.clone())),
        });
    }

    // Final: push Null and Return (Return pops the top of the eval stack).
    let null_const_idx = init_constants.len();
    init_constants.push(bex_vm_types::ConstValue::Null);
    init_instructions.push(Instruction::LoadConst(null_const_idx));
    init_meta.push(bex_vm_types::bytecode::InstructionMeta {
        operand: Some(bex_vm_types::bytecode::OperandMeta::Const(
            "null".to_string(),
        )),
    });
    init_instructions.push(Instruction::Return);
    init_meta.push(bex_vm_types::bytecode::InstructionMeta::default());

    let bytecode = Bytecode {
        instructions: init_instructions,
        constants: init_constants,
        meta: init_meta,
        ..Bytecode::default()
    };

    Ok(Function {
        name: "$init".to_string(),
        source_file: String::new(), // synthesized, no source file
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
        stream_return_type: baml_type::Ty::Null {
            attr: baml_type::TyAttr::default(),
        },
        param_names: Vec::new(),
        param_types: Vec::new(),
        param_has_default: Vec::new(),
        throws_type: None,
        origin: FunctionOrigin::Internal,
        body_meta: None,
        trace: false,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
        sync::atomic::{AtomicU32, Ordering},
    };

    use baml_base::{FileId, SourceFile};
    use baml_compiler2_ast as ast;
    use baml_compiler2_hir::item_tree::{Attribute, AttributeArg};
    use baml_workspace::Project;

    use super::*;

    #[salsa::db]
    struct TestDb {
        storage: salsa::Storage<TestDb>,
        next_file_id: AtomicU32,
        project: Option<Project>,
    }

    impl Default for TestDb {
        fn default() -> Self {
            Self {
                storage: salsa::Storage::default(),
                next_file_id: AtomicU32::new(0),
                project: None,
            }
        }
    }

    impl Clone for TestDb {
        fn clone(&self) -> Self {
            Self {
                storage: self.storage.clone(),
                next_file_id: AtomicU32::new(self.next_file_id.load(Ordering::SeqCst)),
                project: self.project,
            }
        }
    }

    impl TestDb {
        fn add_file(&mut self, path: impl Into<PathBuf>, content: &str) -> SourceFile {
            let file_id = FileId::new(self.next_file_id.fetch_add(1, Ordering::SeqCst));
            SourceFile::new(self, content.to_string(), path.into(), file_id)
        }

        fn init_with_file(&mut self) -> SourceFile {
            let file = self.add_file("test.baml", "function f() -> int { 1 }");
            self.project = Some(Project::new(self, PathBuf::from("."), vec![file]));
            file
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl baml_workspace::Db for TestDb {
        fn project(&self) -> Project {
            self.project.expect("TestDb not initialized")
        }
    }

    #[salsa::db]
    impl baml_compiler2_hir::Db for TestDb {}

    #[salsa::db]
    impl baml_compiler2_ppir::Db for TestDb {}

    #[salsa::db]
    impl baml_compiler2_tir::Db for TestDb {}

    #[salsa::db]
    impl baml_compiler2_mir::Db for TestDb {}

    #[salsa::db]
    impl Db for TestDb {}

    // ── parse_string_attr_value ─────────────────────────────────────────

    #[test]
    fn parse_regular_string() {
        assert_eq!(
            parse_string_attr_value(r#""hello world""#),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn parse_regular_string_with_escapes() {
        assert_eq!(
            parse_string_attr_value(r#""line\nbreak""#),
            Some("line\nbreak".to_string())
        );
        assert_eq!(
            parse_string_attr_value(r#""tab\tstop""#),
            Some("tab\tstop".to_string())
        );
        assert_eq!(
            parse_string_attr_value(r#""a\\b""#),
            Some(r"a\b".to_string())
        );
        assert_eq!(
            parse_string_attr_value(r#""a\"b""#),
            Some(r#"a"b"#.to_string())
        );
    }

    #[test]
    fn parse_single_quoted_string() {
        assert_eq!(
            parse_string_attr_value("'hello world'"),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn parse_empty_regular_string() {
        assert_eq!(parse_string_attr_value(r#""""#), Some(String::new()));
    }

    #[test]
    fn parse_raw_string_single_hash() {
        // Input: #"raw\ntext"#  — raw strings don't unescape
        let input = "#\"raw\\ntext\"#";
        assert_eq!(
            parse_string_attr_value(input),
            Some("raw\\ntext".to_string())
        );
    }

    #[test]
    fn parse_raw_string_double_hash() {
        // Input: ##"has "# inside"##
        let input = "##\"has \"# inside\"##";
        assert_eq!(
            parse_string_attr_value(input),
            Some("has \"# inside".to_string())
        );
    }

    #[test]
    fn parse_empty_raw_string() {
        // Input: #""#
        let input = "#\"\"#";
        assert_eq!(parse_string_attr_value(input), Some(String::new()));
    }

    #[test]
    fn parse_non_string_returns_none() {
        assert_eq!(parse_string_attr_value("vm"), None);
        assert_eq!(parse_string_attr_value("42"), None);
        assert_eq!(parse_string_attr_value("true"), None);
    }

    #[test]
    fn parse_malformed_returns_none() {
        // Unclosed quote: just a bare "
        assert_eq!(parse_string_attr_value("\"unclosed"), None);
        // Mismatched hashes: #"text"##  (1 opening hash, 2 closing)
        let mismatched = "#\"text\"##";
        assert_eq!(parse_string_attr_value(mismatched), None);
        // Degenerate: #"# (would panic without length guard)
        let degenerate = "#\"#";
        assert_eq!(parse_string_attr_value(degenerate), None);
    }

    #[test]
    fn function_metadata_reports_defaulted_params() {
        let mut db = TestDb::default();
        let file = db.init_with_file();

        let mut defaults = ast::FunctionDefaults::empty();
        let default_expr = ast::DefaultExprId::new(defaults.exprs.exprs.alloc(ast::Expr::Null));
        let function_id = baml_compiler2_hir::ids::LocalItemId::<
            baml_compiler2_hir::ids::FunctionMarker,
        >::new(1, 0);
        let default_ref = baml_compiler2_hir::item_tree::DefaultExprRef {
            function: function_id,
            expr: default_expr,
        };
        let param = |name: &str, default| baml_compiler2_hir::item_tree::FunctionParam {
            name: baml_base::Name::new(name),
            type_expr: None,
            default,
            span: baml_base::Span::fake().range,
        };
        let func_data = baml_compiler2_hir::item_tree::Function {
            name: baml_base::Name::new("f"),
            generic_params: Vec::new(),
            params: vec![
                param("required", None),
                param("with_default", Some(default_ref)),
                param("also_required", None),
            ],
            defaults,
            return_type: None,
            throws: None,
            body: None,
            declarative_meta: None,
            origin: ast::FunctionOrigin::UserDefined,
            docstring: None,
            span: baml_base::Span::fake().range,
        };
        let parameter_defaults = baml_compiler2_hir::signature::FunctionParameterDefaults {
            params: func_data
                .params
                .iter()
                .map(|param| param.default.clone())
                .collect(),
            defaults: func_data.defaults.clone(),
        };
        let cache = ResolvedAliases {
            aliases: HashMap::new(),
            recursive: HashSet::new(),
        };

        let (_, _, param_has_default, _) = compute_function_metadata_from_item_tree(
            &db,
            file,
            function_id,
            &func_data,
            &parameter_defaults,
            &cache,
        );

        assert_eq!(param_has_default, vec![false, true, false]);
    }

    // ── extract_schema_attrs ────────────────────────────────────────────

    fn mk_attr(name: &str, args: &[&str]) -> Attribute {
        Attribute {
            name: baml_base::Name::new(name),
            args: args
                .iter()
                .map(|v| AttributeArg {
                    key: None,
                    value: v.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn extract_description_and_alias() {
        let attrs = vec![
            mk_attr("description", &[r#""A field""#]),
            mk_attr("alias", &[r#""myField""#]),
        ];
        let (desc, alias, skip) = extract_schema_attrs(&attrs);
        assert_eq!(desc, Some("A field".to_string()));
        assert_eq!(alias, Some("myField".to_string()));
        assert!(!skip);
    }

    #[test]
    fn extract_skip() {
        let attrs = vec![mk_attr("skip", &[])];
        let (desc, alias, skip) = extract_schema_attrs(&attrs);
        assert_eq!(desc, None);
        assert_eq!(alias, None);
        assert!(skip);
    }

    #[test]
    fn extract_unknown_attrs_ignored() {
        let attrs = vec![
            mk_attr("stream.done", &["true"]),
            mk_attr("internal.opaque", &[]),
            mk_attr("description", &[r#""kept""#]),
        ];
        let (desc, _, _) = extract_schema_attrs(&attrs);
        assert_eq!(desc, Some("kept".to_string()));
    }

    #[test]
    fn extract_non_string_arg_ignored() {
        let attrs = vec![mk_attr("description", &["42"])];
        let (desc, _, _) = extract_schema_attrs(&attrs);
        assert_eq!(desc, None);
    }

    #[test]
    fn extract_wrong_arg_count_ignored() {
        let attrs = vec![mk_attr("description", &[])]; // 0 args
        let (desc, _, _) = extract_schema_attrs(&attrs);
        assert_eq!(desc, None);
    }

    #[test]
    fn extract_duplicate_last_wins() {
        let attrs = vec![
            mk_attr("description", &[r#""first""#]),
            mk_attr("description", &[r#""second""#]),
        ];
        let (desc, _, _) = extract_schema_attrs(&attrs);
        assert_eq!(desc, Some("second".to_string()));
    }

    #[test]
    fn extract_raw_string_attr() {
        // Simulates @description(#"raw desc"#)
        let attrs = vec![mk_attr("description", &["#\"raw desc\"#"])];
        let (desc, _, _) = extract_schema_attrs(&attrs);
        assert_eq!(desc, Some("raw desc".to_string()));
    }

    #[test]
    fn extract_regular_string_attr_decodes_escapes() {
        let attrs = vec![mk_attr("description", &[r#""a\nb\tc\\d\"e""#])];
        let (desc, _, _) = extract_schema_attrs(&attrs);
        assert_eq!(desc, Some("a\nb\tc\\d\"e".to_string()));
    }

    #[test]
    fn extract_no_attrs() {
        let (desc, alias, skip) = extract_schema_attrs(&[]);
        assert_eq!(desc, None);
        assert_eq!(alias, None);
        assert!(!skip);
    }
}
