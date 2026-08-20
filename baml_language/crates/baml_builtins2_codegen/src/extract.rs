//! Extract `$rust_function` and `$rust_io_function` builtins from the compiler2 `.baml` stdlib files.
//!
//! Iterates `baml_builtins2::ALL`, parses each file through the compiler2
//! front-end (lex → parse → lower), and collects every function whose body is
//! `FunctionBodyDef::Builtin(BuiltinKind::Vm)` or `BuiltinKind::Io` into a
//! `NativeBuiltin` record. The CST is also retained per file for
//! `//baml:mut_self`, `//baml:vm`, and `//baml:mut_vm` directive scanning.

use baml_base::FileId;
use baml_compiler_diagnostics::ToDiagnostic;
use baml_compiler_syntax::{NodeOrToken, SyntaxKind, SyntaxNode};
use baml_compiler2_ast::ast::{
    BuiltinKind, ClassDef, FunctionBodyDef, FunctionDef, ImplementsForDef, Item, TypeExpr,
    TypeExprKind,
};

use crate::types::{
    BamlType, BuiltinPipeline, NativeBuiltin, NativeClassDef, NativeClassField, Param, Receiver,
    ReceiverType, VmUsage,
};

/// Convert a byte offset in source text to a 1-based `(line, column)` pair.
fn offset_to_line_col(source: &str, offset: u32) -> (usize, usize) {
    let offset = (offset as usize).min(source.len());
    let prefix = &source[..offset];
    let line = prefix.matches('\n').count() + 1;
    let col = offset - prefix.rfind('\n').map_or(0, |p| p + 1) + 1;
    (line, col)
}

/// Returned when a builtin `.baml` file has parse errors or HIR lowering diagnostics.
pub struct ExtractNativeBuiltinsError {
    message: String,
}

impl std::fmt::Debug for ExtractNativeBuiltinsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for ExtractNativeBuiltinsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ExtractNativeBuiltinsError {}

/// Stdlib packages outside `baml` that also declare `$rust_io_function`
/// sys-ops, and so contribute to the generated IO dispatch surface (`SysOp`,
/// the `IoNamespace*` traits, `RuntimeIo`).
///
/// Scanning is deliberately narrow: only the files that actually contain a
/// `$rust_io_function` are parsed, and only their IO builtins and class defs
/// are kept — a package's `$rust_function` (VM) builtins keep coming from its
/// own per-package extraction (`extract_native_builtins_for`, used by
/// `bex_vm`'s build script), so nothing else about these packages is dragged
/// into the IO codegen.
const EXTRA_IO_PACKAGES: &[&str] = &["ai"];

/// Marker whose presence in a file's source selects it for the extra-package
/// IO scan (see `EXTRA_IO_PACKAGES`).
const IO_BUILTIN_MARKER: &str = "$rust_io_function";

/// Parse, lower, and extract all `$rust_function` and `$rust_io_function` builtins
/// from the `.baml` stdlib.
///
/// Returns `(vm_builtins, io_builtins, class_defs)`:
/// - `vm_builtins`: `$rust_function` builtins of the `baml` package
///   (synchronous, run inline in VM)
/// - `io_builtins`: `$rust_io_function` builtins (async, dispatched via engine)
///   of the `baml` package plus `EXTRA_IO_PACKAGES`
/// - `class_defs`: `baml`-package class definitions with fields (for view/owned
///   struct generation)
///
/// Fails with [`ExtractNativeBuiltinsError`] if any file has parse errors or non-empty HIR
/// diagnostics (so codegen never runs on a silently broken stdlib).
#[allow(clippy::type_complexity)]
pub fn extract_native_builtins()
-> Result<(Vec<NativeBuiltin>, Vec<NativeBuiltin>, Vec<NativeClassDef>), ExtractNativeBuiltinsError>
{
    let (vm_builtins, mut io_builtins, mut class_defs) =
        extract_native_builtins_for(baml_builtins2::PACKAGE_BAML)?;
    for package in EXTRA_IO_PACKAGES {
        let (_vm, extra_io, extra_classes) =
            extract_scoped(package, |f| f.contents.contains(IO_BUILTIN_MARKER))?;
        io_builtins.extend(extra_io);
        // Classes declared alongside extra-package IO builtins (e.g. the
        // `ai.Context` render-context surface) participate in owned/view
        // struct generation so receiver methods and typed returns marshal
        // through generated structs, exactly like `baml`-package classes.
        // `filter_io_class_defs` still drops anything in a namespace without
        // IO builtins, and `sys_op_variant_name` keys are package-qualified,
        // so this cannot collide with `baml` names.
        class_defs.extend(extra_classes);
    }
    Ok((vm_builtins, io_builtins, class_defs))
}

/// [`extract_native_builtins`] scoped to one stdlib package, so each package
/// with Rust-implemented builtins gets its own generated dispatch surface.
#[allow(clippy::type_complexity)]
pub fn extract_native_builtins_for(
    package: &str,
) -> Result<(Vec<NativeBuiltin>, Vec<NativeBuiltin>, Vec<NativeClassDef>), ExtractNativeBuiltinsError>
{
    extract_scoped(package, |_| true)
}

/// [`extract_native_builtins_for`] restricted to the package's files matching
/// `keep_file`.
#[allow(clippy::type_complexity)]
fn extract_scoped(
    package: &str,
    keep_file: impl Fn(&baml_builtins2::BuiltinFile) -> bool,
) -> Result<(Vec<NativeBuiltin>, Vec<NativeBuiltin>, Vec<NativeClassDef>), ExtractNativeBuiltinsError>
{
    let mut vm_builtins = Vec::new();
    let mut io_builtins = Vec::new();
    let mut class_defs = Vec::new();
    let mut diagnostic_lines: Vec<String> = Vec::new();

    for builtin_file in baml_builtins2::ALL
        .iter()
        .filter(|f| f.package == package && keep_file(f))
    {
        let path = builtin_file.virtual_path();
        // Real filesystem path for diagnostic messages (clickable in editors).
        let diag_path = format!(
            "{}/{}/{}",
            baml_builtins2::BAML_STD_DIR,
            builtin_file.package,
            builtin_file.relative_path
        );
        // Lex and parse into a lossless CST.
        let tokens = baml_compiler_lexer::lex_lossless(builtin_file.contents, FileId::new(0));
        let (green, errors) = baml_compiler_parser::parse_file(&tokens);
        for e in &errors {
            let d = e.to_diagnostic();
            let location = d
                .primary_span()
                .map(|span| {
                    let (line, col) =
                        offset_to_line_col(builtin_file.contents, span.range.start().into());
                    format!("{diag_path}:{line}:{col}")
                })
                .unwrap_or_else(|| diag_path.clone());
            diagnostic_lines.push(format!("  {location}: [{}] {}", d.id.code(), d.message));
        }
        if !errors.is_empty() {
            continue;
        }
        let cst_root = SyntaxNode::new_root(green);

        // Lower CST → AST items. This extractor only ever lowers the builtin
        // stdlib packages, which define the builtin names themselves
        // (`type json = ...`), so the reserved-declaration-name check is off.
        let (items, diags, _) = baml_compiler2_ast::lower_file_with_path_and_test_owner(
            &cst_root,
            None,
            baml_compiler2_ast::LowerFileOpts {
                test_owner: None,
                in_builtin_package: true,
            },
        );
        for ld in &diags {
            let d = ld.to_diagnostic(FileId::new(0));
            let location = d
                .primary_span()
                .map(|span| {
                    let (line, col) =
                        offset_to_line_col(builtin_file.contents, span.range.start().into());
                    format!("{diag_path}:{line}:{col}")
                })
                .unwrap_or_else(|| diag_path.clone());
            diagnostic_lines.push(format!("  {location}: [{}] {}", d.id.code(), d.message));
        }
        if !diags.is_empty() {
            continue;
        }

        // Build the namespace prefix from the file's package and path-derived namespace.
        // e.g. package="baml", ns_path=["sys"] → "baml.sys"
        //      package="baml", ns_path=[]       → "baml"
        let ns_path = builtin_file.namespace_path();
        let namespace_prefix = if ns_path.is_empty() {
            builtin_file.package.to_string()
        } else {
            format!("{}.{}", builtin_file.package, ns_path.join("."))
        };

        for item in &items {
            match item {
                Item::Class(class_def) => {
                    extract_from_class(
                        class_def,
                        &namespace_prefix,
                        &cst_root,
                        &path,
                        &mut vm_builtins,
                        &mut io_builtins,
                    );
                    if let Some(class_def_record) =
                        extract_class_fields(class_def, &namespace_prefix, &path)
                    {
                        class_defs.push(class_def_record);
                    }
                }
                Item::Function(func_def) => {
                    extract_from_free_function(
                        func_def,
                        &namespace_prefix,
                        &cst_root,
                        &path,
                        &mut vm_builtins,
                        &mut io_builtins,
                    );
                }
                Item::ImplementsFor(impl_def) => {
                    extract_from_implements_for(
                        impl_def,
                        &namespace_prefix,
                        &cst_root,
                        &path,
                        &mut vm_builtins,
                        &mut io_builtins,
                    );
                }
                _ => {}
            }
        }
    }

    // Every host-bound builtin must declare a `throws` clause (use `throws never`
    // if it cannot fail). A missing clause is a contract gap, not a silent
    // "infallible" default — reject it so fallibility is always explicit.
    for b in vm_builtins.iter().chain(io_builtins.iter()) {
        if b.throws.is_none() {
            diagnostic_lines.push(format!(
                "  {}: builtin `{}` is missing a `throws` clause \
                 (declare `throws never` if it cannot fail)",
                b.source_file, b.path
            ));
        }
    }

    if !diagnostic_lines.is_empty() {
        return Err(ExtractNativeBuiltinsError {
            message: format!(
                "extract_native_builtins failed (fix stdlib .baml sources):\n{}",
                diagnostic_lines.join("\n")
            ),
        });
    }

    Ok((vm_builtins, io_builtins, class_defs))
}

/// Extract `$rust_function` and `$rust_io_function` methods from a class definition.
fn extract_from_class(
    class_def: &ClassDef,
    namespace_prefix: &str,
    cst_root: &SyntaxNode,
    source_file: &str,
    vm_builtins: &mut Vec<NativeBuiltin>,
    io_builtins: &mut Vec<NativeBuiltin>,
) {
    let class_name = class_def.name.as_str();
    let class_generics: Vec<String> = class_def
        .generic_params
        .iter()
        .map(|param| param.name.as_str().to_string())
        .collect();

    // Builtin methods may be declared directly on the class or inside an
    // `implements I { ... }` block (BEP-044) — e.g. the `random.Rng`
    // implementors put their `$rust_function` / `$rust_io_function` methods in
    // an `implements Rng { ... }` block. A method inside `implements I` is named
    // `{ns}.{Class}.{I}.{method}` at runtime (matching MIR's
    // `scoped_implements_method_name`), so it carries the interface qualifier; a
    // direct method is just `{ns}.{Class}.{method}`.
    let direct = class_def.methods.iter().map(|m| (m, None));
    let in_impl = class_def.implements.iter().flat_map(|b| {
        // The interface `TypeExpr`'s `Display` is the exact form MIR uses to
        // build the method's fully-qualified name, so reuse it verbatim.
        let iface = b.target.to_string();
        b.methods.iter().map(move |m| (m, Some(iface.clone())))
    });
    for (method, iface_qualifier) in direct.chain(in_impl) {
        let Some(pipeline) = extract_builtin_pipeline(method) else {
            continue;
        };

        // Merge class generics with method-level generics.
        let method_generics: Vec<String> = method
            .generic_params
            .iter()
            .map(|param| param.name.as_str().to_string())
            .collect();
        let mut all_generics = class_generics.clone();
        for g in &method_generics {
            if !all_generics.contains(g) {
                all_generics.push(g.clone());
            }
        }

        let path = match &iface_qualifier {
            Some(iface) => {
                format!(
                    "{namespace_prefix}.{class_name}.{iface}.{}",
                    method.name.as_str()
                )
            }
            None => format!("{namespace_prefix}.{class_name}.{}", method.name.as_str()),
        };
        let fn_name = path_to_fn_name(&path);

        let has_self = method
            .params
            .first()
            .map(|p| p.name.as_str() == "self")
            .unwrap_or(false);

        let method_name = method.name.as_str();
        let is_mut =
            has_self && has_method_directive(cst_root, class_name, method_name, "//baml:mut_self");
        let has_vm = has_method_directive(cst_root, class_name, method_name, "//baml:vm");
        let has_mut_vm = has_method_directive(cst_root, class_name, method_name, "//baml:mut_vm");
        let may_yield = has_method_directive(cst_root, class_name, method_name, "//baml:may_yield");
        let fallible = has_method_directive(cst_root, class_name, method_name, "//baml:fallible");

        assert!(
            !(has_vm && has_mut_vm),
            "baml codegen error: {path} has both //baml:vm and //baml:mut_vm \
             -- these are mutually exclusive"
        );
        assert!(
            !(is_mut && has_vm || is_mut && has_mut_vm && !may_yield),
            "baml codegen error: {path} has //baml:mut_self with //baml:vm, or non-yielding //baml:mut_vm \
             -- mutable receiver and VM access are only supported for //baml:may_yield glue"
        );
        assert!(
            !may_yield || has_mut_vm,
            "baml codegen error: {path} has //baml:may_yield without //baml:mut_vm \
             -- yielding methods require mutable VM access"
        );

        let vm_usage = if has_mut_vm {
            VmUsage::MutRef
        } else if has_vm {
            VmUsage::Ref
        } else {
            VmUsage::None
        };

        let throws = extract_throws(method);

        // Always set receiver for class methods — even static methods (no `self`)
        // need it for dispatch routing. The runtime path is
        // "baml.sap.ParseCache.new" which dispatches via class name.
        let receiver_type = if !has_self {
            ReceiverType::Static
        } else if is_mut {
            ReceiverType::MutSelf
        } else {
            ReceiverType::RefSelf
        };
        let receiver = Some(Receiver {
            class_name: class_name.to_string(),
            namespace: namespace_prefix
                .strip_prefix("baml.")
                .unwrap_or("")
                .to_string(),
            // Mirrors `extract_class_fields`: dedicated-variant types and
            // field-less (opaque/marker) classes get no `view::` struct.
            instance_backed: !matches!(class_name, "Array" | "Map" | "String" | "Uint8Array")
                && !class_def.fields.is_empty(),
            class_generics: class_generics.clone(),
            receiver_type,
        });
        let params = if has_self {
            extract_params_skip_self(method, &all_generics)
        } else {
            method
                .params
                .iter()
                .map(|p| Param {
                    name: p.name.as_str().to_string(),
                    ty: p
                        .type_expr
                        .as_ref()
                        .map(|te| type_expr_to_baml_type(te, &all_generics))
                        .unwrap_or(BamlType::Named("unknown".to_string())),
                })
                .collect()
        };

        let return_type = method
            .return_type
            .as_ref()
            .map(|te| type_expr_to_baml_type(te, &all_generics))
            .unwrap_or(BamlType::Null);

        let builtin = NativeBuiltin {
            path,
            fn_name,
            params,
            return_type,
            generics: all_generics,
            receiver,
            vm_usage,
            may_yield,
            fallible,
            pipeline,
            throws,
            source_file: source_file.to_string(),
        };

        match pipeline {
            BuiltinPipeline::Vm => vm_builtins.push(builtin),
            BuiltinPipeline::Io => io_builtins.push(builtin),
        }
    }
}

/// Extract field definitions from a class, producing a `NativeClassDef`.
///
/// Returns `None` for classes that keep dedicated `Object` variants (Array, Map, String, `Uint8Array`)
/// since they don't use `Object::Instance`.
fn extract_class_fields(
    class_def: &ClassDef,
    namespace_prefix: &str,
    source_file: &str,
) -> Option<NativeClassDef> {
    let class_name = class_def.name.as_str();

    // Skip classes with dedicated Object variants — they are not Instance-based.
    match class_name {
        "Array" | "Map" | "String" | "Uint8Array" => return None,
        _ => {}
    }

    // Skip classes with no fields (pure namespace markers or method-only classes).
    if class_def.fields.is_empty() {
        return None;
    }

    let generic_params: Vec<String> = class_def
        .generic_params
        .iter()
        .map(|param| param.name.as_str().to_string())
        .collect();

    let fields: Vec<NativeClassField> = class_def
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let field_type = type_expr_to_baml_type(&field.type_expr, &generic_params);
            NativeClassField {
                name: field.name.as_str().to_string(),
                field_type,
                index,
            }
        })
        .collect();

    Some(NativeClassDef {
        name: class_name.to_string(),
        namespace_prefix: namespace_prefix.to_string(),
        generic_params,
        fields,
        source_file: source_file.to_string(),
    })
}

/// Extract a `$rust_function` or `$rust_io_function` free function (not inside a class).
fn extract_from_free_function(
    func_def: &FunctionDef,
    namespace_prefix: &str,
    cst_root: &SyntaxNode,
    source_file: &str,
    vm_builtins: &mut Vec<NativeBuiltin>,
    io_builtins: &mut Vec<NativeBuiltin>,
) {
    let Some(pipeline) = extract_builtin_pipeline(func_def) else {
        return;
    };

    let generics: Vec<String> = func_def
        .generic_params
        .iter()
        .map(|param| param.name.as_str().to_string())
        .collect();

    let path = format!("{namespace_prefix}.{}", func_def.name.as_str());
    let fn_name = path_to_fn_name(&path);
    let has_vm = has_free_fn_directive(cst_root, func_def.name.as_str(), "//baml:vm");
    let has_mut_vm = has_free_fn_directive(cst_root, func_def.name.as_str(), "//baml:mut_vm");
    let may_yield = has_free_fn_directive(cst_root, func_def.name.as_str(), "//baml:may_yield");
    let fallible = has_free_fn_directive(cst_root, func_def.name.as_str(), "//baml:fallible");

    assert!(
        !(has_vm && has_mut_vm),
        "baml codegen error: {path} has both //baml:vm and //baml:mut_vm \
         -- these are mutually exclusive"
    );
    assert!(
        !may_yield || has_mut_vm,
        "baml codegen error: {path} has //baml:may_yield without //baml:mut_vm \
         -- yielding functions require mutable VM access"
    );

    let vm_usage = if has_mut_vm {
        VmUsage::MutRef
    } else if has_vm {
        VmUsage::Ref
    } else {
        VmUsage::None
    };

    let throws = extract_throws(func_def);

    let params: Vec<Param> = func_def
        .params
        .iter()
        .map(|p| Param {
            name: p.name.as_str().to_string(),
            ty: p
                .type_expr
                .as_ref()
                .map(|te| type_expr_to_baml_type(te, &generics))
                .unwrap_or(BamlType::Named("unknown".to_string())),
        })
        .collect();

    let return_type = func_def
        .return_type
        .as_ref()
        .map(|te| type_expr_to_baml_type(te, &generics))
        .unwrap_or(BamlType::Null);

    let builtin = NativeBuiltin {
        path,
        fn_name,
        params,
        return_type,
        generics,
        receiver: None,
        vm_usage,
        may_yield,
        fallible,
        pipeline,
        throws,
        source_file: source_file.to_string(),
    };

    match pipeline {
        BuiltinPipeline::Vm => vm_builtins.push(builtin),
        BuiltinPipeline::Io => io_builtins.push(builtin),
    }
}

/// Extract `$rust_function` methods from a top-level `implement Interface for Type`
/// block (e.g. `implement Equals for int { function eq(...) { $rust_function } }`).
///
/// These are dispatched at runtime under the synthetic class name
/// `<Interface>$for$<for_target>`, which MUST match the name the MIR lowering
/// assigns to such methods (see `baml_compiler2_mir::lower`'s
/// `definition_item_ref` / `{iface}$for${for}` formatting) so that
/// `get_native_fn` resolves the function the VM looks up.
///
/// The receiver (`self`) and any `Self`-typed parameters are mapped to the
/// `for` target's primitive class so the existing receiver/argument extraction
/// machinery applies (`int` → `i64`, `bigint` → `Arc<BigInt>`, etc.). Only
/// blocks implemented for a built-in primitive or container are native-backed;
/// blocks for user-defined types compile as ordinary bytecode and are skipped.
fn extract_from_implements_for(
    impl_def: &ImplementsForDef,
    namespace_prefix: &str,
    cst_root: &SyntaxNode,
    source_file: &str,
    vm_builtins: &mut Vec<NativeBuiltin>,
    io_builtins: &mut Vec<NativeBuiltin>,
) {
    let Some(recv_class) = receiver_class_for_target(&impl_def.for_target) else {
        return;
    };

    // The synthetic class segment must match MIR's `{iface}$for${for}` exactly.
    let synthetic_class = format!("{}$for${}", impl_def.interface_target, impl_def.for_target);

    let impl_generics: Vec<String> = impl_def
        .generic_params
        .iter()
        .map(|param| param.name.as_str().to_string())
        .collect();

    // `Self` inside method signatures resolves to the `for` target.
    let self_baml = type_expr_to_baml_type(&impl_def.for_target, &impl_generics);

    // Container element comparison reads heap values, so it needs a `&BexVm`;
    // scalar comparisons operate on `Copy`/`Arc` receivers and need no VM. A
    // method-level `//baml:vm` / `//baml:mut_vm` directive overrides this default.
    let default_vm_usage = match recv_class {
        "Array" | "Map" => VmUsage::Ref,
        _ => VmUsage::None,
    };

    for method in &impl_def.methods {
        let Some(pipeline) = extract_builtin_pipeline(method) else {
            continue;
        };

        // Merge the `implement` block's generics with method-level generics so a
        // method type parameter resolves to `BamlType::Generic`, not a `Named`.
        let method_generics: Vec<String> = method
            .generic_params
            .iter()
            .map(|param| param.name.as_str().to_string())
            .collect();
        let mut all_generics = impl_generics.clone();
        for g in &method_generics {
            if !all_generics.contains(g) {
                all_generics.push(g.clone());
            }
        }

        let path = format!(
            "{namespace_prefix}.{synthetic_class}.{}",
            method.name.as_str()
        );
        let fn_name = path_to_fn_name(&path);

        // Scan the method's `//baml:` directives, scoped to this exact method by
        // its `name_span` (the method names `add` / `div` / … collide across
        // `implement` blocks, so a name-keyed lookup would be ambiguous). The
        // `implement` blocks have no `//baml:mut_self` (the receiver is always
        // `&self`), so only VM-access, yielding, and fallibility are scanned.
        let has_vm = impl_method_has_directive(cst_root, method, "//baml:vm");
        let has_mut_vm = impl_method_has_directive(cst_root, method, "//baml:mut_vm");
        let may_yield = impl_method_has_directive(cst_root, method, "//baml:may_yield");
        let fallible = impl_method_has_directive(cst_root, method, "//baml:fallible");

        assert!(
            !(has_vm && has_mut_vm),
            "baml codegen error: {path} has both //baml:vm and //baml:mut_vm \
             -- these are mutually exclusive"
        );
        assert!(
            !may_yield || has_mut_vm,
            "baml codegen error: {path} has //baml:may_yield without //baml:mut_vm \
             -- yielding methods require mutable VM access"
        );

        let vm_usage = if has_mut_vm {
            VmUsage::MutRef
        } else if has_vm {
            VmUsage::Ref
        } else {
            default_vm_usage
        };

        let receiver = Some(Receiver {
            class_name: recv_class.to_string(),
            namespace: namespace_prefix
                .strip_prefix("baml.")
                .unwrap_or("")
                .to_string(),
            instance_backed: false,
            class_generics: impl_generics.clone(),
            receiver_type: ReceiverType::RefSelf,
        });

        let params: Vec<Param> = method
            .params
            .iter()
            .skip(1) // skip `self`
            .map(|p| Param {
                name: p.name.as_str().to_string(),
                ty: p
                    .type_expr
                    .as_ref()
                    .map(|te| type_expr_to_baml_type_with_self(te, &all_generics, &self_baml))
                    .unwrap_or(BamlType::Named("unknown".to_string())),
            })
            .collect();

        let return_type = method
            .return_type
            .as_ref()
            .map(|te| type_expr_to_baml_type_with_self(te, &all_generics, &self_baml))
            .unwrap_or(BamlType::Null);

        let builtin = NativeBuiltin {
            path,
            fn_name,
            params,
            return_type,
            generics: all_generics,
            receiver,
            vm_usage,
            may_yield,
            fallible,
            pipeline,
            throws: extract_throws(method),
            source_file: source_file.to_string(),
        };

        match pipeline {
            BuiltinPipeline::Vm => vm_builtins.push(builtin),
            BuiltinPipeline::Io => io_builtins.push(builtin),
        }
    }
}

/// Map a `for` target type expression to the receiver class name whose
/// extraction logic the codegen already knows. Returns `None` for targets that
/// are not native-backed primitives/containers.
fn receiver_class_for_target(ty: &TypeExpr) -> Option<&'static str> {
    use baml_type::PrimitiveType as P;
    if let Some(primitive) = ty.kind.written_primitive() {
        return match primitive {
            P::Int => Some("Int"),
            P::Bigint => Some("Bigint"),
            P::Float => Some("Float"),
            P::Bool => Some("Bool"),
            P::Null => Some("Null"),
            P::String => Some("String"),
            P::Uint8Array => Some("Uint8Array"),
            P::Image | P::Audio | P::Video | P::Pdf => None,
        };
    }
    Some(match &ty.kind {
        TypeExprKind::List { .. } => "Array",
        TypeExprKind::Map { .. } => "Map",
        _ => return None,
    })
}

/// Like [`type_expr_to_baml_type`] but resolves a `Self` path to `self_baml`
/// (the `for` target of the enclosing `implement` block) at any nesting depth,
/// e.g. `Self[]`, `Self?`, or `map<K, Self>`.
///
/// Container shapes are reconstructed here rather than deferring to
/// [`type_expr_to_baml_type`], which has no notion of `Self` and would emit
/// `BamlType::Named("Self")` for a nested occurrence. The container arms mirror
/// `type_expr_to_baml_type` exactly so the only behavioural difference is the
/// `Self` substitution. `Self?` parses as `Union(Self, null)`, so the `Union`
/// arm replicates the single-non-null-variant → `Optional` collapse while
/// recursing into the variant to catch a nested `Self`.
fn type_expr_to_baml_type_with_self(
    ty: &TypeExpr,
    generics: &[String],
    self_baml: &BamlType,
) -> BamlType {
    let recurse = |inner: &TypeExpr| type_expr_to_baml_type_with_self(inner, generics, self_baml);
    match &ty.kind {
        TypeExprKind::Path { segments, .. }
            if segments.len() == 1 && segments[0].as_str() == "Self" =>
        {
            self_baml.clone()
        }
        TypeExprKind::Optional { inner, .. } => BamlType::Optional(Box::new(recurse(inner))),
        TypeExprKind::List { inner, .. } => BamlType::List(Box::new(recurse(inner))),
        TypeExprKind::Map { key, value, .. } => {
            BamlType::Map(Box::new(recurse(key)), Box::new(recurse(value)))
        }
        TypeExprKind::Union { variants, .. } => {
            let non_null: Vec<_> = variants.iter().filter(|v| !v.kind.is_null()).collect();
            if non_null.len() == 1 && non_null.len() < variants.len() {
                BamlType::Optional(Box::new(recurse(non_null[0])))
            } else {
                BamlType::Named("union".to_string())
            }
        }
        _ => type_expr_to_baml_type(ty, generics),
    }
}

/// Returns the pipeline kind if the function body is a Rust builtin, or None otherwise.
fn extract_builtin_pipeline(func: &FunctionDef) -> Option<BuiltinPipeline> {
    match &func.body {
        Some(FunctionBodyDef::Builtin(BuiltinKind::Vm)) => Some(BuiltinPipeline::Vm),
        Some(FunctionBodyDef::Builtin(BuiltinKind::Io)) => Some(BuiltinPipeline::Io),
        _ => None,
    }
}

/// Extract error categories from the `throws` clause of an IO function.
///
/// The `throws` field is `Option<SpannedTypeExpr>`. For a single error like
/// `throws root.errors.Io`, it's `TypeExprKind::Path(["root", "errors", "Io"])`.
/// For multiple errors like `throws root.errors.Io | root.errors.Timeout`,
/// it's `TypeExprKind::Union([Path(...), Path(...)])`.
/// Extract the `throws` clause of a builtin.
///
/// - `None` — no `throws` clause (rejected by the missing-throws check).
/// - `Some([])` — `throws never` (the `Never` type yields no categories).
/// - `Some([cats])` — one or more error categories; the builtin is fallible.
fn extract_throws(func: &FunctionDef) -> Option<Vec<String>> {
    let throws_expr = func.throws.as_ref()?;
    Some(extract_throw_categories(throws_expr))
}

#[allow(clippy::redundant_closure_for_method_calls)]
fn extract_throw_categories(ty: &TypeExpr) -> Vec<String> {
    match &ty.kind {
        TypeExprKind::Path { segments, .. } => {
            let path: Vec<&str> = segments.iter().map(|s| s.as_str()).collect();
            if path.len() >= 3 && (path[0] == "baml" || path[0] == "root") && path[1] == "errors" {
                vec![path[2..].join(".")]
            } else {
                vec![
                    segments
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join("."),
                ]
            }
        }
        TypeExprKind::Union { variants, .. } => {
            variants.iter().flat_map(extract_throw_categories).collect()
        }
        _ => vec![],
    }
}

/// Convert a dotted path to a Rust function name.
///
/// Examples:
/// - `"baml.Array.length"` → `"baml_array_length"`
/// - `"baml.deep_copy"` → `"baml_deep_copy"`
/// - `"baml.sys.argv"` → `"baml_sys_argv"`
/// - `"baml.media.Pdf.url"` → `"baml_media_pdf_url"`
fn path_to_fn_name(path: &str) -> String {
    path.replace('.', "_").to_lowercase()
}

/// Extract parameters from a method, skipping the first `self` parameter.
fn extract_params_skip_self(func: &FunctionDef, generics: &[String]) -> Vec<Param> {
    func.params
        .iter()
        .skip(1) // skip `self`
        .map(|p| Param {
            name: p.name.as_str().to_string(),
            ty: p
                .type_expr
                .as_ref()
                .map(|te| type_expr_to_baml_type(te, generics))
                .unwrap_or(BamlType::Named("unknown".to_string())),
        })
        .collect()
}

/// Convert a `TypeExpr` from the AST to a `BamlType`.
///
/// `generics` is the combined set of type parameter names in scope (class + method).
#[allow(clippy::redundant_closure_for_method_calls)]
fn type_expr_to_baml_type(ty: &TypeExpr, generics: &[String]) -> BamlType {
    use baml_type::PrimitiveType as P;
    if let Some(primitive) = ty.kind.written_primitive() {
        return match primitive {
            P::Int => BamlType::Int,
            P::Bigint => BamlType::Bigint,
            P::Float => BamlType::Float,
            P::String => BamlType::String,
            P::Bool => BamlType::Bool,
            P::Null => BamlType::Null,
            P::Uint8Array => BamlType::Uint8Array,
            P::Image => BamlType::Media("Image".to_string()),
            P::Audio => BamlType::Media("Audio".to_string()),
            P::Video => BamlType::Media("Video".to_string()),
            P::Pdf => BamlType::Media("Pdf".to_string()),
        };
    }
    match &ty.kind {
        TypeExprKind::Never { .. } => BamlType::Null,
        TypeExprKind::Void { .. } => BamlType::Null,

        // Consumed by the written_primitive check above; grouped here only
        // for match exhaustiveness - no second mapping table.
        TypeExprKind::Int { .. }
        | TypeExprKind::Bigint { .. }
        | TypeExprKind::Float { .. }
        | TypeExprKind::String { .. }
        | TypeExprKind::Bool { .. }
        | TypeExprKind::Null { .. }
        | TypeExprKind::Uint8Array { .. } => {
            unreachable!("consumed by the written_primitive check")
        }
        // Only `MediaKind::Generic` (no primitive) reaches this arm.
        TypeExprKind::Media { .. } => BamlType::Media("Media".to_string()),

        TypeExprKind::Optional { inner, .. } => {
            BamlType::Optional(Box::new(type_expr_to_baml_type(inner, generics)))
        }

        TypeExprKind::List { inner, .. } => {
            BamlType::List(Box::new(type_expr_to_baml_type(inner, generics)))
        }

        TypeExprKind::Map { key, value, .. } => BamlType::Map(
            Box::new(type_expr_to_baml_type(key, generics)),
            Box::new(type_expr_to_baml_type(value, generics)),
        ),

        TypeExprKind::Path { segments, .. } => {
            // Single-segment path may be a generic type param or a named type.
            if segments.len() == 1 {
                let name = segments[0].as_str();
                if generics.iter().any(|g| g == name) {
                    BamlType::Generic(name.to_string())
                } else {
                    BamlType::Named(name.to_string())
                }
            } else {
                // Multi-segment path (e.g. `baml.errors.Io`) — treat as Named.
                let name = segments
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                BamlType::Named(name)
            }
        }

        TypeExprKind::Union { variants, .. } => {
            let non_null: Vec<_> = variants.iter().filter(|v| !v.kind.is_null()).collect();
            if non_null.len() == 1 && non_null.len() < variants.len() {
                BamlType::Optional(Box::new(type_expr_to_baml_type(non_null[0], generics)))
            } else {
                BamlType::Named("union".to_string())
            }
        }
        TypeExprKind::Literal { .. } => BamlType::Named("literal".to_string()),
        TypeExprKind::Function { .. } => BamlType::Named("function".to_string()),
        TypeExprKind::AssociatedTypeProjection { .. }
        | TypeExprKind::BuiltinUnknown { .. }
        | TypeExprKind::Unknown { .. }
        | TypeExprKind::Error { .. }
        | TypeExprKind::Infer { .. } => BamlType::Named("unknown".to_string()),
        TypeExprKind::Type { .. } => BamlType::Named("type".to_string()),
        TypeExprKind::Rust { .. } => BamlType::RustType,
    }
}

/// Check if a method inside a class has the given `directive` comment (e.g. `"//baml:mut_self"`)
/// before its `function` keyword in the CST.
///
/// In the Rowan CST, the parser's `bump()` emits leading trivia tokens (whitespace,
/// comments) immediately before the `function` keyword inside the `FUNCTION_DEF` node
/// itself. So the directive appears as a `LINE_COMMENT` token child of the
/// `FUNCTION_DEF` node, before the `KW_FUNCTION` token.
fn has_method_directive(
    cst_root: &SyntaxNode,
    class_name: &str,
    method_name: &str,
    directive: &str,
) -> bool {
    for class_node in cst_root.descendants() {
        if class_node.kind() != SyntaxKind::CLASS_DEF {
            continue;
        }
        if !class_node_has_name(&class_node, class_name) {
            continue;
        }
        for func_node in class_node.descendants() {
            if func_node.kind() != SyntaxKind::FUNCTION_DEF {
                continue;
            }
            if !func_node_has_name(&func_node, method_name) {
                continue;
            }
            if function_node_has_leading_directive(&func_node, directive) {
                return true;
            }
        }
    }
    false
}

/// Check if an `implement`-block method has the given `directive` comment before
/// its `function` keyword in the CST.
///
/// Unlike [`has_method_directive`], the lookup is scoped to a single method by
/// its `name_span` rather than by `(class_name, method_name)`: the method names
/// in `implement` blocks (`add`, `div`, `eq`, …) repeat across every block, so a
/// name-keyed search would match the wrong block. Exactly one `FUNCTION_DEF`
/// node contains the method's name token, so containment uniquely identifies it.
/// Offsets are compared as raw `u32`s so the match does not depend on the
/// `text_size` version the AST and CST crates each resolve.
fn impl_method_has_directive(cst_root: &SyntaxNode, method: &FunctionDef, directive: &str) -> bool {
    let name_start = u32::from(method.name_span.start());
    for node in cst_root.descendants() {
        if node.kind() != SyntaxKind::FUNCTION_DEF {
            continue;
        }
        let range = node.text_range();
        if u32::from(range.start()) <= name_start && name_start < u32::from(range.end()) {
            return function_node_has_leading_directive(&node, directive);
        }
    }
    false
}

/// Check if a top-level (non-class) function has the given `directive` comment
/// before its `function` keyword in the CST.
fn has_free_fn_directive(cst_root: &SyntaxNode, fn_name: &str, directive: &str) -> bool {
    for node in cst_root.children() {
        if node.kind() != SyntaxKind::FUNCTION_DEF {
            continue;
        }
        if !func_node_has_name(&node, fn_name) {
            continue;
        }
        if function_node_has_leading_directive(&node, directive) {
            return true;
        }
    }
    false
}

/// Returns true if the `CLASS_DEF` node has a name token matching `class_name`.
fn class_node_has_name(class_node: &SyntaxNode, class_name: &str) -> bool {
    // The class name is the first WORD token that is a direct meaningful child.
    // In the CST: `class WORD<...> { ... }`
    // Scan children_with_tokens: skip the `class` keyword and trivia,
    // then the next WORD should be the class name.
    for element in class_node.children_with_tokens() {
        if let NodeOrToken::Token(tok) = element {
            if tok.kind().is_trivia() || tok.kind() == SyntaxKind::KW_CLASS {
                continue;
            }
            // First non-trivia, non-keyword token should be the class name.
            return tok.kind() == SyntaxKind::WORD && tok.text() == class_name;
        }
        // Encountered a child node before finding the name token — not a match.
        // (Shouldn't happen for CLASS_DEF in practice.)
    }
    false
}

/// Returns true if the `FUNCTION_DEF` node has a name matching `method_name`.
fn func_node_has_name(func_node: &SyntaxNode, method_name: &str) -> bool {
    for element in func_node.children_with_tokens() {
        if let NodeOrToken::Token(tok) = element {
            if tok.kind().is_trivia() || tok.kind() == SyntaxKind::KW_FUNCTION {
                continue;
            }
            // First non-trivia, non-`function` token is the function name.
            // BEP-044 introduced `implements`, `extends`, and `interface` as
            // top-level keywords; the parser still accepts them as method
            // names so reflection methods like `TypeValue.implements(...)`
            // can keep their natural spelling.
            let kind = tok.kind();
            let is_name_token = matches!(
                kind,
                SyntaxKind::WORD
                    | SyntaxKind::KW_IMPLEMENTS
                    | SyntaxKind::KW_IMPLEMENT
                    | SyntaxKind::KW_EXTENDS
                    | SyntaxKind::KW_REQUIRES
                    | SyntaxKind::KW_INTERFACE
            );
            return is_name_token && tok.text() == method_name;
        }
        // Encountered a child node — past the name.
        break;
    }
    false
}

/// Check whether a `FUNCTION_DEF` node contains a specific directive `LINE_COMMENT`
/// (e.g. `"//baml:mut_self"` or `"//baml:mut_vm"`) before its `KW_FUNCTION` token.
///
/// The parser emits trivia (whitespace, comments) as tokens within the containing
/// syntactic node before the first real token. So any directive comment that
/// appears immediately before `function foo(...)` in source is stored as a
/// `LINE_COMMENT` child of that `FUNCTION_DEF` node.
fn function_node_has_leading_directive(func_node: &SyntaxNode, directive: &str) -> bool {
    for element in func_node.children_with_tokens() {
        match element {
            NodeOrToken::Token(tok) => match tok.kind() {
                SyntaxKind::LINE_COMMENT => {
                    let text = tok.text().trim();
                    if text == directive {
                        return true;
                    }
                }
                k if k.is_whitespace() => {}
                SyntaxKind::KW_FUNCTION => {
                    return false;
                }
                SyntaxKind::AT_AT | SyntaxKind::BLOCK_COMMENT => {}
                _ => {
                    return false;
                }
            },
            NodeOrToken::Node(_) => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the expected `throws` value (`Some([cats])`) for assertions.
    /// `throws(&[])` is `throws never`.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "mirrors the Option<Vec<String>> `throws` field so assertions read naturally"
    )]
    fn throws(cats: &[&str]) -> Option<Vec<String>> {
        Some(cats.iter().map(|s| (*s).to_string()).collect())
    }

    #[test]
    fn test_extract_class_fields() {
        let (_vm, _io, class_defs) = extract_native_builtins().unwrap();

        let pdf = class_defs
            .iter()
            .find(|c| c.name == "Pdf")
            .expect("missing Pdf");
        assert_eq!(pdf.namespace_prefix, "baml.media");
        assert_eq!(pdf.fields.len(), 1);
        assert_eq!(pdf.fields[0].name, "_data");
        assert_eq!(pdf.fields[0].index, 0);
        assert!(matches!(pdf.fields[0].field_type, BamlType::RustType));

        assert!(
            class_defs.iter().any(|c| c.name == "Audio"),
            "missing Audio"
        );
        assert!(
            class_defs.iter().any(|c| c.name == "Video"),
            "missing Video"
        );
        assert!(
            class_defs.iter().any(|c| c.name == "Image"),
            "missing Image"
        );

        assert!(
            !class_defs.iter().any(|c| c.name == "Array"),
            "Array should be excluded"
        );
        assert!(
            !class_defs.iter().any(|c| c.name == "Map"),
            "Map should be excluded"
        );
        assert!(
            !class_defs.iter().any(|c| c.name == "String"),
            "String should be excluded"
        );

        // IO class fields
        let file = class_defs
            .iter()
            .find(|c| c.name == "File")
            .expect("missing File");
        assert_eq!(file.namespace_prefix, "baml.fs");
        assert_eq!(file.fields.len(), 1);
        assert_eq!(file.fields[0].name, "_handle");
        assert!(matches!(file.fields[0].field_type, BamlType::RustType));

        let socket = class_defs
            .iter()
            .find(|c| c.name == "TcpStream")
            .expect("missing TcpStream");
        assert_eq!(socket.namespace_prefix, "baml.net");
        assert_eq!(socket.fields.len(), 1);
        assert_eq!(socket.fields[0].name, "_handle");
        assert!(matches!(socket.fields[0].field_type, BamlType::RustType));

        // UDP datagram is a plain data class (payload + sender address).
        let datagram = class_defs
            .iter()
            .find(|c| c.name == "Datagram")
            .expect("missing Datagram");
        assert_eq!(datagram.namespace_prefix, "baml.net");
        assert_eq!(datagram.fields.len(), 2);
        assert_eq!(datagram.fields[0].name, "data");
        assert!(matches!(
            datagram.fields[0].field_type,
            BamlType::Uint8Array
        ));
        assert_eq!(datagram.fields[1].name, "addr");
        assert!(matches!(datagram.fields[1].field_type, BamlType::String));

        let response = class_defs
            .iter()
            .find(|c| c.name == "Response")
            .expect("missing Response");
        assert_eq!(response.namespace_prefix, "baml.http");
        assert_eq!(response.fields.len(), 4);
        assert_eq!(response.fields[0].name, "status_code");
        assert!(matches!(response.fields[0].field_type, BamlType::Int));
        assert_eq!(response.fields[1].name, "headers");
        assert!(matches!(response.fields[1].field_type, BamlType::Map(_, _)));
        assert_eq!(response.fields[2].name, "url");
        assert!(matches!(response.fields[2].field_type, BamlType::String));
        assert_eq!(response.fields[3].name, "_body");
        assert!(matches!(response.fields[3].field_type, BamlType::RustType));

        let request = class_defs
            .iter()
            .find(|c| c.name == "Request")
            .expect("missing Request");
        assert_eq!(request.namespace_prefix, "baml.http");
        assert_eq!(request.fields.len(), 4);

        // The structural prompt lives in the ai package.
        let (_ai_vm, _ai_io, ai_class_defs) = extract_native_builtins_for("ai").unwrap();
        let prompt = ai_class_defs
            .iter()
            .find(|c| c.name == "Prompt")
            .expect("missing ai.Prompt");
        assert_eq!(prompt.namespace_prefix, "ai");
        assert_eq!(prompt.fields.len(), 1);
        assert!(matches!(prompt.fields[0].field_type, BamlType::RustType));
    }

    #[test]
    fn test_path_to_fn_name() {
        assert_eq!(path_to_fn_name("baml.Array.length"), "baml_array_length");
        assert_eq!(path_to_fn_name("baml.deep_copy"), "baml_deep_copy");
        assert_eq!(path_to_fn_name("baml.sys.argv"), "baml_sys_argv");
        assert_eq!(path_to_fn_name("baml.media.Pdf.url"), "baml_media_pdf_url");
        assert_eq!(path_to_fn_name("baml.Array.push"), "baml_array_push");
    }

    #[test]
    fn test_sys_op_variant_name() {
        let make = |path: &str| NativeBuiltin {
            path: path.to_string(),
            fn_name: String::new(),
            params: vec![],
            return_type: BamlType::Null,
            generics: vec![],
            receiver: None,
            vm_usage: VmUsage::None,
            may_yield: false,
            fallible: false,
            pipeline: BuiltinPipeline::Io,
            throws: Some(vec![]),
            source_file: String::new(),
        };
        assert_eq!(make("baml.fs.open").sys_op_variant_name(), "BamlFsOpen");
        assert_eq!(
            make("baml.fs.File.read").sys_op_variant_name(),
            "BamlFsFileRead"
        );
        assert_eq!(make("baml.env.get").sys_op_variant_name(), "BamlEnvGet");
        assert_eq!(
            make("baml.http.fetch").sys_op_variant_name(),
            "BamlHttpFetch"
        );
        assert_eq!(make("baml.sys.panic").sys_op_variant_name(), "BamlSysPanic");
        assert_eq!(make("ai.Prompt.text").sys_op_variant_name(), "AiPromptText");
        assert_eq!(
            make("ai.internal.render_output_format").sys_op_variant_name(),
            "AiInternalRenderOutputFormat"
        );
    }

    #[test]
    fn test_extract_vm_builtins_unchanged() {
        let (vm_builtins, _io, _class_defs) = extract_native_builtins().unwrap();
        assert!(
            vm_builtins.len() >= 24,
            "Expected at least 24 VM builtins, got {}",
            vm_builtins.len()
        );

        // All VM builtins should have pipeline == Vm. They MAY declare a `throws`
        // clause (e.g. `Array.map<U, E>(... throws E)` carries `E` through, and
        // `Uint8Array.from_hex` throws `InvalidArgument`). Codegen consumes the
        // declared throws to decide whether the trait method returns a `Result`.
        for b in &vm_builtins {
            assert_eq!(b.pipeline, BuiltinPipeline::Vm, "{} should be Vm", b.path);
        }

        let array_length = vm_builtins
            .iter()
            .find(|b| b.path == "baml.Array.length")
            .expect("missing Array.length");
        assert_eq!(array_length.fn_name, "baml_array_length");
        assert!(array_length.receiver.is_some());
        assert_eq!(array_length.params.len(), 0);
        // Infallible builtin declares `throws never` -> `Some([])` (otherwise
        // codegen would wrap it in a spurious `Result`).
        assert_eq!(array_length.throws, throws(&[]));

        // Concrete-error throws: `Uint8Array.from_hex` rejects malformed input
        // with `InvalidArgument`. Pin this so a regression in throws extraction
        // (or in the .baml signature) trips this test instead of silently
        // dropping the `Result` wrapper from the generated trait method.
        let from_hex = vm_builtins
            .iter()
            .find(|b| b.path == "baml.Uint8Array.from_hex")
            .expect("missing Uint8Array.from_hex");
        assert_eq!(from_hex.throws, throws(&["InvalidArgument"]));

        // Generic-throws: `Array.map<U, E>(... throws E)` carries the callback's
        // error type through. The extractor records the generic name verbatim.
        let array_map = vm_builtins
            .iter()
            .find(|b| b.path == "baml.Array.map")
            .expect("missing Array.map");
        assert_eq!(array_map.throws, throws(&["E"]));

        let deep_copy = vm_builtins
            .iter()
            .find(|b| b.path == "baml.deep_copy")
            .expect("missing deep_copy");
        assert!(deep_copy.receiver.is_none());
        assert_eq!(deep_copy.generics, vec!["T"]);

        let array_push = vm_builtins
            .iter()
            .find(|b| b.path == "baml.Array.push")
            .expect("missing Array.push");
        assert!(array_push.receiver.as_ref().unwrap().receiver_type.is_mut());

        let string_length = vm_builtins
            .iter()
            .find(|b| b.path == "baml.String.length")
            .expect("missing String.length");
        assert_eq!(string_length.fn_name, "baml_string_length");

        let trunc_to_int = vm_builtins
            .iter()
            .find(|b| b.path == "baml._trunc_to_int")
            .expect("missing _trunc_to_int");
        assert!(trunc_to_int.receiver.is_none());
        assert_eq!(trunc_to_int.params.len(), 1);
        assert!(matches!(trunc_to_int.params[0].ty, BamlType::Float));

        let pdf_url = vm_builtins
            .iter()
            .find(|b| b.path == "baml.media.Pdf.url")
            .expect("missing media.Pdf.url");
        assert!(pdf_url.receiver.is_some());
        assert_eq!(pdf_url.receiver.as_ref().unwrap().class_name, "Pdf");

        assert_eq!(deep_copy.vm_usage, VmUsage::MutRef);

        // `//baml:vm` on a non-container receiver: `Array`/`Map` methods default to
        // `Ref` without the directive, so a media method is what actually pins the
        // directive being read.
        assert_eq!(pdf_url.vm_usage, VmUsage::Ref);

        assert_eq!(array_length.vm_usage, VmUsage::None);
        assert_eq!(array_push.vm_usage, VmUsage::None);
        assert_eq!(trunc_to_int.vm_usage, VmUsage::None);

        let string_split = vm_builtins
            .iter()
            .find(|b| b.path == "baml.String.split")
            .expect("missing String.split");
        assert_eq!(string_split.vm_usage, VmUsage::None);
    }

    #[test]
    fn test_io_builtin_throws() {
        let (_vm, io_builtins, _class_defs) = extract_native_builtins().unwrap();

        let fs_open = io_builtins
            .iter()
            .find(|b| b.path == "baml.fs.open")
            .unwrap();
        assert_eq!(fs_open.throws, throws(&["Io", "InvalidArgument"]));

        // `connect`/`fetch` are now thin BAML wrappers that resolve the optional
        // `timeout` default; the underlying sys-ops are the `_`-prefixed forms.
        let net_connect = io_builtins
            .iter()
            .find(|b| b.path == "baml.net.TcpStream._connect")
            .unwrap();
        assert_eq!(net_connect.throws, throws(&["Io", "Timeout"]));

        let http_fetch = io_builtins
            .iter()
            .find(|b| b.path == "baml.http._fetch")
            .unwrap();
        assert_eq!(http_fetch.throws, throws(&["Io", "Timeout"]));

        let sap_final = io_builtins
            .iter()
            .find(|b| b.path == "baml.sap.__parse_final")
            .unwrap();
        assert_eq!(sap_final.throws, throws(&["LlmClient"]));
    }
}
