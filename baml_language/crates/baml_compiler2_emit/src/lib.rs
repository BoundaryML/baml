//! Code generation for BAML (compiler2 pipeline).
//!
//! Compiles MIR2 to bytecode for the BAML VM using stackification.

mod analysis;
mod emit;
mod pull_semantics;
mod stack_carry;
mod verifier;

use std::collections::{HashMap, HashSet};

pub use analysis::OptLevel;
use baml_base::{Name, Span};
use baml_compiler2_ast::TypeExpr;
use baml_compiler2_hir::{
    compiler2_all_files,
    contributions::Definition,
    file_package::file_package,
    loc::{ClassLoc, FunctionLoc, InterfaceLoc, LetLoc},
    package::PackageId,
};
use baml_compiler2_mir::{
    BuiltinKind, Local, MirFunctionBody, MirFunctionKind, Operand, Place, ResolvedAliases, Rvalue,
    StatementKind, Terminator, def_to_item_ref, lower_function, lower_let_body,
};
// Use the PPIR item tree (which includes synthetic *$stream items) rather than
// the bare HIR item tree, to stay consistent with TIR's LocalItemId indices.
use baml_compiler2_ppir::file_item_tree;
use baml_type::{RuntimeTy, TyAttr};
use bex_vm_types::{
    Bytecode, CaptureCategory, Class, ClassField, ConstValue, Enum, EnumVariant, Function,
    FunctionCaptureProps, FunctionKind, FunctionMeta, FunctionOrigin, Instruction, Object,
    ObjectIndex, ObjectPool, Program,
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
            baml_compiler2_mir::resolved_aliases_for_package(db, pkg_id)
        });
    }
    caches
}

/// Build the program-wide interface-implementation registry for the runtime
/// resolver.
///
/// Bakes every interface impl as a `RuntimeImplRule` carrying the implementor
/// pattern (`TyTemplate`, generic params → `TypeArgRef`), the per-param bounds,
/// the interface args/associated bindings, and the method FQNs. The runtime
/// resolver matches a value's concrete type against these (rustc-style
/// selection) to dispatch an interface method — operator overloading today;
/// reflection once unified. Built from the item tree in two parts: (a)
/// out-of-body `implements_for` blocks; (b) methods folded onto a class.
fn build_packages(
    db: &dyn baml_compiler2_mir::Db,
    all_files: &[baml_base::SourceFile],
    alias_caches: &HashMap<Name, ResolvedAliases>,
    function_indices: &HashMap<String, usize>,
    interface_indices: &HashMap<baml_type::TypeName, usize>,
    program_packages: &mut indexmap::IndexMap<Name, bex_vm_types::types::ProgramPackage>,
) {
    use baml_compiler2_tir::{
        lower_type_expr::{lower_type_expr_in_ns, qualify_def},
        ty,
    };
    use bex_vm_types::{
        ObjectIndex,
        types::{InterfaceBound, ProgramImplRule, ProgramMethodImpl},
    };

    type IfaceParts = (
        baml_type::TypeName,
        Vec<baml_type::TyTemplate>,
        Vec<(Name, baml_type::TyTemplate)>,
    );
    // Split a lowered interface type into its base `TypeName` plus its args /
    // associated bindings as `TyTemplate`s (generic params → `TypeArgRef`).
    fn split_interface(
        iface_ty: &ty::Ty,
        resolved: &ResolvedAliases,
        generics: &[Name],
    ) -> Option<IfaceParts> {
        let ty::Ty::Interface(qtn, args, assoc, _) = iface_ty else {
            return None;
        };
        let arg_templates = args
            .iter()
            .map(|a| baml_compiler2_mir::tir2_to_template(a, resolved, generics))
            .collect();
        let assoc_templates = assoc
            .iter()
            .map(|(n, t)| {
                (
                    n.clone(),
                    baml_compiler2_mir::tir2_to_template(t, resolved, generics),
                )
            })
            .collect();
        Some((qtn.clone(), arg_templates, assoc_templates))
    }

    // Resolve a function FQN to its emitted object index. A missing entry means
    // the method's function object wasn't emitted (shouldn't happen for a valid
    // impl), so drop just that method — losing a dispatch, never adding a wrong
    // one (mirrors the `None`-skips below).
    let resolve_fqn = |fqn: &str| -> Option<ObjectIndex> {
        let idx = function_indices.get(fqn).copied();
        debug_assert!(
            idx.is_some(),
            "impl method `{fqn}` has no emitted function object",
        );
        idx.map(ObjectIndex::from_raw)
    };

    // Per interface, its default methods (`name → fn FQN`). An implementing rule
    // inherits these for any method it doesn't override, so each baked rule's
    // method table is complete (the resolver needs no separate default lookup; a
    // default body is generic over `Self`, so calling it on the concrete value
    // dispatches its inner `self.m()` calls back to the impl). Built across all
    // files first since an impl may live in a different file/package than its
    // interface.
    let mut iface_defaults: indexmap::IndexMap<
        baml_type::TypeName,
        indexmap::IndexMap<Name, String>,
    > = indexmap::IndexMap::new();
    // Per interface (with defaults), its declared associated-type names *in order*.
    // An inherited default is compiled against the interface's frame, so its
    // type-arg layout is `[interface generic args ++ associated types]`, the assoc
    // ordered by this declaration order (matching the closed-world switch's
    // `interface_assoc_frame_tys`). Used to build each default method's frame.
    let mut iface_assoc_order: indexmap::IndexMap<baml_type::TypeName, Vec<Name>> =
        indexmap::IndexMap::new();
    for file in all_files {
        let item_tree = file_item_tree(db, *file);
        for (iface_id, iface_data) in &item_tree.interfaces {
            if iface_data.default_methods.is_empty() {
                continue;
            }
            let iface_tn = qualify_def(
                db,
                Definition::Interface(InterfaceLoc::new(db, *file, *iface_id)),
                &iface_data.name,
            );
            iface_assoc_order
                .entry(iface_tn.clone())
                .or_insert_with(|| {
                    iface_data
                        .associated_types
                        .iter()
                        .map(|assoc| assoc.name.clone())
                        .collect()
                });
            let entry = iface_defaults.entry(iface_tn).or_default();
            for &m in &iface_data.default_methods {
                entry.insert(
                    item_tree[m].name.clone(),
                    def_to_item_ref(db, Definition::Function(FunctionLoc::new(db, *file, m)))
                        .to_string(),
                );
            }
        }
    }
    // The frame an inherited default of `iface_tn` is invoked with, for a rule
    // implementing it at `interface_args` / `interface_assoc`: the interface's
    // generic args followed by its associated types in declared order (templates
    // over the impl's generics). A non-generic interface with no associated types
    // (`Equals`/`Compare`) yields `[]`.
    let interface_frame = |iface_tn: &baml_type::TypeName,
                           interface_args: &[baml_type::TyTemplate],
                           interface_assoc: &[(Name, baml_type::TyTemplate)]|
     -> Vec<baml_type::TyTemplate> {
        let mut frame: Vec<baml_type::TyTemplate> = interface_args.to_vec();
        if let Some(order) = iface_assoc_order.get(iface_tn) {
            for name in order {
                // One slot per *declared* associated type, in order — so the frame
                // width is always `interface_args + assoc_count` and the method-level
                // type args (appended after this frame at the call site) land at the
                // De Bruijn indices the callee expects. An associated type the impl
                // leaves to its default is padded with `BuiltinUnknown`, matching the
                // closed-world switch's `interface_assoc_frame_tys` still-missing
                // fallback; a short frame would instead shift every later slot and
                // miscompile the method's own type args.
                // TODO(M1): for a defaulted assoc *read as a runtime type* in a default
                // body, complete with the assoc's actual default rather than this
                // placeholder (needs TyTemplate-space default completion). Currently
                // unobservable — std defaults read assoc only in `throws` position.
                let slot = interface_assoc
                    .iter()
                    .find(|(an, _)| an == name)
                    .map(|(_, t)| t.clone())
                    .unwrap_or_else(|| {
                        baml_type::TyTemplate::Concrete(baml_type::RuntimeTy::BuiltinUnknown {
                            attr: TyAttr::default(),
                        })
                    });
                frame.push(slot);
            }
        }
        frame
    };
    // Fill a rule's method table with the interface's defaults (override winning),
    // each carrying the interface frame it is invoked with.
    let merge_defaults = |methods: &mut indexmap::IndexMap<Name, ProgramMethodImpl>,
                          iface_tn: &baml_type::TypeName,
                          interface_frame: &[baml_type::TyTemplate]| {
        if let Some(defaults) = iface_defaults.get(iface_tn) {
            for (name, fqn) in defaults {
                let Some(fqn_idx) = resolve_fqn(fqn) else {
                    continue;
                };
                methods
                    .entry(name.clone())
                    .or_insert_with(|| ProgramMethodImpl {
                        fqn: fqn_idx,
                        frame: interface_frame.to_vec(),
                    });
            }
        }
    };

    for file in all_files {
        let item_tree = file_item_tree(db, *file);
        let pkg_info = file_package(db, *file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        let ns = &pkg_info.namespace_path;
        let resolved = &alias_caches[&pkg_info.package];
        // Lower a type expr in this file's namespace, discarding diagnostics
        // (these targets were already validated upstream).
        let lower = |expr: &TypeExpr, generics: &[Name]| -> ty::Ty {
            let mut diags = Vec::new();
            lower_type_expr_in_ns(db, expr, pkg_items, ns, generics, &mut diags)
        };
        // Each generic param's interface bound set (`T extends A & B` → {A, B};
        // a param has at most one bound today). A bound is an interface, possibly
        // generic or carrying associated bindings — `split_interface` captures its
        // args/assoc as templates over the impl's params. A non-interface bound,
        // rejected upstream, has no interface to record, so skip the whole rule
        // (`None`); dropping a rule only ever loses a dispatch, never adds a wrong
        // one.
        let bound_sets = |param_bounds: &[Option<TypeExpr>],
                          generics: &[Name]|
         -> Option<Vec<Vec<InterfaceBound>>> {
            param_bounds
                .iter()
                .map(|b| match b {
                    None => Some(Vec::new()),
                    Some(te) => split_interface(&lower(te, generics), resolved, generics).map(
                        |(interface, args, assoc)| {
                            vec![InterfaceBound {
                                interface,
                                args,
                                assoc,
                            }]
                        },
                    ),
                })
                .collect()
        };
        // Associated-type bindings written in an `implements` block body
        // (`type Item = int`) live beside the target, not in it (`split_interface`
        // only sees the target), so lower them here to fold into the implemented
        // interface's bindings.
        let lower_assoc = |bindings: &[baml_compiler2_ast::AssociatedTypeBindingDef],
                           generics: &[Name]|
         -> Vec<(Name, baml_type::TyTemplate)> {
            bindings
                .iter()
                .filter_map(|b| {
                    let te = b.type_expr.as_ref()?;
                    Some((
                        b.name.clone(),
                        baml_compiler2_mir::tir2_to_template(
                            &lower(te, generics),
                            resolved,
                            generics,
                        ),
                    ))
                })
                .collect()
        };

        // (a) Out-of-body `implement<G> I for FOR { ... }`: primitives,
        // containers, generic classes, and blanket `for T`. (A non-generic
        // concrete class's out-of-body impl folds onto the class — see (b).)
        for imp in &item_tree.implements_for {
            let Some((iface_tn, interface_args, mut interface_assoc)) = split_interface(
                &lower(&imp.interface_target, &imp.generic_params),
                resolved,
                &imp.generic_params,
            ) else {
                continue;
            };
            interface_assoc.extend(lower_assoc(
                &imp.associated_type_bindings,
                &imp.generic_params,
            ));
            let for_ty_pattern = baml_compiler2_mir::tir2_to_template(
                &lower(&imp.for_target, &imp.generic_params),
                resolved,
                &imp.generic_params,
            );
            let Some(generic_param_bounds) =
                bound_sets(&imp.generic_param_bounds, &imp.generic_params)
            else {
                continue;
            };
            // An impl's own method is compiled against the impl's own generics.
            let impl_frame: Vec<baml_type::TyTemplate> =
                (0..u32::try_from(imp.generic_params.len()).expect("generic arity fits u32"))
                    .map(baml_type::TyTemplate::TypeArgRef)
                    .collect();
            let Some(interface_head) = interface_indices
                .get(&iface_tn)
                .copied()
                .map(ObjectIndex::from_raw)
            else {
                continue;
            };
            let mut methods = indexmap::IndexMap::new();
            for &m in &imp.methods {
                let fqn = def_to_item_ref(db, Definition::Function(FunctionLoc::new(db, *file, m)))
                    .to_string();
                let Some(fqn) = resolve_fqn(&fqn) else {
                    continue;
                };
                methods.insert(
                    item_tree[m].name.clone(),
                    ProgramMethodImpl {
                        fqn,
                        frame: impl_frame.clone(),
                    },
                );
            }
            let iface_frame = interface_frame(&iface_tn, &interface_args, &interface_assoc);
            merge_defaults(&mut methods, &iface_tn, &iface_frame);
            program_packages
                .entry(pkg_info.package.clone())
                .or_default()
                .impl_rules
                .entry(interface_head)
                .or_default()
                .push(ProgramImplRule {
                    interface_head,
                    for_ty_pattern,
                    generic_param_bounds,
                    interface_args,
                    interface_assoc,
                    methods,
                });
        }

        // (b) In-body `class C { implements I { ... } }` and folded non-generic
        // out-of-body `implement I for C` impls. Drive off the impl *blocks* so a
        // field-only (method-less) impl is still registered (membership matters
        // for reflection and bound checks even when there's nothing to dispatch);
        // attach any folded methods, grouped by their interface target.
        for (class_id, class_data) in &item_tree.classes {
            if class_data.implements.is_empty() {
                continue;
            }
            let class_tn = qualify_def(
                db,
                Definition::Class(ClassLoc::new(db, *file, *class_id)),
                &class_data.name,
            );
            let generics = &class_data.generic_params;

            // Each folded method tagged with the full interface instantiation it
            // implements (name + args). A class may implement the same interface
            // at several instantiations (e.g. `Converter<int>` + `Converter<float>`),
            // each with its own methods; keying only by interface name would let
            // one block's method overwrite the other's, so methods are matched to
            // their block by the full instantiation below.
            let class_method_impls: Vec<(
                baml_type::TypeName,
                Vec<baml_type::TyTemplate>,
                Name,
                String,
            )> = class_data
                .methods
                .iter()
                .filter_map(|&m| {
                    let target = item_tree.method_to_iface_target.get(&m)?;
                    let (m_iface_tn, m_args, _m_assoc) =
                        split_interface(&lower(target, generics), resolved, generics)?;
                    Some((
                        m_iface_tn,
                        m_args,
                        item_tree[m].name.clone(),
                        def_to_item_ref(db, Definition::Function(FunctionLoc::new(db, *file, m)))
                            .to_string(),
                    ))
                })
                .collect();

            // The implementor pattern is the class at its own parameters; bounds
            // come from the class's generic parameters. Shared by all its blocks.
            let for_ty_pattern = if generics.is_empty() {
                baml_type::TyTemplate::Concrete(baml_type::RuntimeTy::Class(
                    class_tn.clone(),
                    Vec::new(),
                    TyAttr::default(),
                ))
            } else {
                baml_type::TyTemplate::Class(
                    class_tn.clone(),
                    (0..u32::try_from(generics.len()).expect("generic arity fits u32"))
                        .map(baml_type::TyTemplate::TypeArgRef)
                        .collect(),
                )
            };
            let Some(generic_param_bounds) = bound_sets(&class_data.generic_param_bounds, generics)
            else {
                continue;
            };
            // An impl block's own methods are compiled against the class's generics.
            let impl_frame: Vec<baml_type::TyTemplate> = (0..u32::try_from(generics.len())
                .expect("generic arity fits u32"))
                .map(baml_type::TyTemplate::TypeArgRef)
                .collect();

            for block in &class_data.implements {
                let Some((iface_tn, interface_args, mut interface_assoc)) =
                    split_interface(&lower(&block.target, generics), resolved, generics)
                else {
                    continue;
                };
                interface_assoc.extend(lower_assoc(&block.associated_type_bindings, generics));
                // Match folded methods to THIS block by the full interface
                // instantiation (name + args), not name alone — coherence makes a
                // given `(type, Iface<Args>)` unique, so this picks exactly this
                // block's methods even when the class implements the same
                // interface at another instantiation.
                let Some(interface_head) = interface_indices
                    .get(&iface_tn)
                    .copied()
                    .map(ObjectIndex::from_raw)
                else {
                    continue;
                };
                let mut methods: indexmap::IndexMap<Name, ProgramMethodImpl> = class_method_impls
                    .iter()
                    .filter(|(m_iface_tn, m_args, _, _)| {
                        *m_iface_tn == iface_tn && *m_args == interface_args
                    })
                    .filter_map(|(_, _, name, fqn)| {
                        Some((
                            name.clone(),
                            ProgramMethodImpl {
                                fqn: resolve_fqn(fqn)?,
                                frame: impl_frame.clone(),
                            },
                        ))
                    })
                    .collect();
                let iface_frame = interface_frame(&iface_tn, &interface_args, &interface_assoc);
                merge_defaults(&mut methods, &iface_tn, &iface_frame);
                program_packages
                    .entry(pkg_info.package.clone())
                    .or_default()
                    .impl_rules
                    .entry(interface_head)
                    .or_default()
                    .push(ProgramImplRule {
                        interface_head,
                        for_ty_pattern: for_ty_pattern.clone(),
                        generic_param_bounds: generic_param_bounds.clone(),
                        interface_args,
                        interface_assoc,
                        methods,
                    });
            }
        }
    }

    // Deterministic order: files/classes iterate from unordered maps. Impl rules
    // are keyed by their interface's object index (assigned in deterministic
    // emission order); within one interface a `for_ty_pattern` is unique (overlap
    // is a coherence error). The primary rule key is the rendered pattern; its
    // `Display` drops module paths, so two distinct same-short-name for-types tie.
    // `{:?}` carries the module-qualified identity and breaks the tie (rather than
    // falling back to unordered-map insertion order). The interface instantiation
    // (args + associated bindings) is folded in last so the same for-type
    // implementing one interface at several instantiations (e.g. `Converter<int>`
    // + `Converter<float>`) orders by content rather than declaration order.
    // Package-level ordering is finalized by the caller once every map is built.
    for pkg in program_packages.values_mut() {
        pkg.interfaces.sort_keys();
        pkg.impl_rules.sort_keys();
        for rules in pkg.impl_rules.values_mut() {
            rules.sort_by_cached_key(|rule| {
                (
                    rule.for_ty_pattern.to_string(),
                    format!("{:?}", rule.for_ty_pattern),
                    format!("{:?}", rule.interface_args),
                    format!("{:?}", rule.interface_assoc),
                )
            });
        }
    }
}

pub(crate) use emit::compile_mir_function;

fn is_builtin_function_name(name: &str) -> bool {
    matches!(
        name.split('.').next(),
        Some("baml" | "boundary" | "reflect" | "assert" | "testing" | "log" | "env")
    )
}

fn emitted_function_origin(
    fq_name: &str,
    is_builtin_file: bool,
    origin: baml_compiler2_ast::FunctionOrigin,
) -> FunctionOrigin {
    if is_builtin_file || is_builtin_function_name(fq_name) {
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
    /// Compile-time types for captures in the function currently being emitted.
    pub capture_types: &'ctx [RuntimeTy],
    /// Capture slots whose cells may be touched by spawned code.
    pub spawn_capture_indices: &'ctx HashSet<usize>,
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
    /// The project has unresolved compile errors, so bytecode generation was
    /// not attempted. Lowering an error-bearing program would feed
    /// inference-only `Unknown`/`Error` types through the runtime-conversion
    /// boundary, which rejects them. Callers should surface the diagnostics to
    /// the user instead of treating this as a compiler fault.
    ProjectHasErrors { error_count: usize },
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Internal(msg) => write!(f, "internal lowering error: {msg}"),
            Self::ProjectHasErrors { error_count } => write!(
                f,
                "cannot generate bytecode: project has {error_count} unresolved compile error(s)"
            ),
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
            "description" | "alias" if attr.args.len() == 1 => {
                let raw = attr.args[0].value.as_str();
                let value = parse_string_attr_value(raw);
                if attr.name.as_str() == "description" {
                    description = value;
                } else {
                    alias = value;
                }
            }
            "description" | "alias" => {}
            "skip" => {
                skip = true;
            }
            _ => {}
        }
    }
    (description, alias, skip)
}

pub use bex_vm_types::Program as ProgramAlias;

/// One entry in the emitted runtime field list for a class.
type MergedFieldEntry = (
    String,
    Option<baml_compiler2_ast::TypeExpr>,
    Vec<baml_compiler2_hir::item_tree::Attribute>,
    Vec<Name>,
    Vec<Name>,
);

/// BEP-044: collect actual runtime fields. Interface fields are views over
/// class fields, so they never add qualified runtime slots.
fn collect_class_fields_with_implements(
    pkg_ns: &[Name],
    class_data: &baml_compiler2_hir::item_tree::Class,
) -> Vec<MergedFieldEntry> {
    let mut out: Vec<MergedFieldEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for field in &class_data.fields {
        let name = field.name.to_string();
        if !seen.insert(name.clone()) {
            continue;
        }
        out.push((
            name,
            field.type_expr.clone(),
            field.attributes.clone(),
            class_data.generic_params.clone(),
            pkg_ns.to_vec(),
        ));
    }

    out
}

/// Build a `TypeName` from a fully-qualified dotted path.
///
/// Emit always fully qualifies — `display_name` keeps the literal package
/// prefix (`"user.Point"`, `"baml.http.Response"`, `"<vendor>.<…>"`). The
/// codegen-output Python and the runtime see the same `<pkg>.<…>` form
/// end-to-end. See `12a-namespace-rules.md §5` for the rationale.
fn fq_to_type_name(fq: &str) -> baml_type::TypeName {
    baml_type::QualifiedTypeName::from_dotted_path(fq)
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
            // Skip intrinsic and await-any functions — they are never called via
            // a Call instruction (intrinsics lower to StatementKind::Intrinsic;
            // `__await_any` lowers to a Terminator::AwaitAny). Pass 4 skips them
            // too, so they must be skipped here as well or the Pass-1 indices
            // desync from the program.globals array (off-by-one for everything
            // after the skipped function).
            if matches!(
                mir.kind,
                MirFunctionKind::Builtin(BuiltinKind::Intrinsic | BuiltinKind::AwaitAny)
            ) {
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
            // BEP-044: collect only the class's actual runtime fields.
            // Interface fields are typed views over class storage, and the
            // validator enforces/link-checks them before emit.
            let merged_fields =
                collect_class_fields_with_implements(&pkg_info.namespace_path, class_data);
            for (idx, (name, type_expr, attrs, gen_params, ns)) in merged_fields.iter().enumerate()
            {
                field_indices.insert(name.clone(), idx);
                let (field_type, field_template) = match type_expr {
                    Some(te) => {
                        let mut diags = Vec::new();
                        // Pass `class_generic_params` as the binding context so
                        // `T`-references inside `class Container<T> { item: T }`
                        // lower to `Tir2Ty::TypeVar("T")` rather than
                        // `Tir2Ty::Unknown`.  This is the input both to the
                        // erased-`Ty` (TypeVar→BuiltinUnknown) used by codegen and to
                        // the `TyTemplate` (TypeVar→TypeArgRef(N)) used by
                        // typed runtime walking.
                        let tir_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                            db, te, pkg_items, ns, gen_params, &mut diags,
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
                        let null_ty = baml_type::RuntimeTy::Null {
                            attr: baml_type::TyAttr::default(),
                        };
                        (null_ty.clone(), baml_type::TyTemplate::Concrete(null_ty))
                    }
                };
                let (field_desc, field_alias, field_skip) = extract_schema_attrs(attrs.as_slice());
                fields.push(ClassField {
                    name: name.clone(),
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

            // BEP-042: does this class define a magic `cleanup(self) -> void`
            // finalizer? This MUST stay in lockstep with the canonical
            // `cleanup_guard::has_cleanup_shape` (which validates the AST and
            // emits E0144): same shape — one `self` param with no default, no
            // generics, `-> void` return, and no propagating `throws` — on the lowered
            // HIR `Function`. The `throws` part reuses the shared helper; the rest
            // is mirrored (the two share field types but not the struct).
            //
            // Only DIRECT class methods count. `class_data.methods` is flattened
            // to include `implements`-block methods (recorded in
            // `method_to_iface_target`), but those are interface members: the AST
            // guard injector and the `{class_fqn}.cleanup` GC resolution only
            // cover direct methods, so an `implements`-block `cleanup` must NOT
            // mark the class finalizable (it would set the flag for a method the
            // GC can neither guard nor resolve).
            let has_cleanup = class_data.methods.iter().any(|method_id| {
                if item_tree.method_to_iface_target.contains_key(method_id) {
                    return false;
                }
                let func = &item_tree[*method_id];
                func.name.as_str() == baml_compiler2_ast::cleanup_guard::CLEANUP_METHOD
                    && func.generic_params.is_empty()
                    && func.params.len() == 1
                    && func.params[0].name.as_str() == "self"
                    && func.params[0].default.is_none()
                    && baml_compiler2_ast::cleanup_guard::throws_is_effectively_none(
                        func.throws.as_ref(),
                    )
                    && matches!(
                        func.return_type.as_ref().map(|st| &st.kind),
                        Some(baml_compiler2_ast::ast::TypeExprKind::Void { .. })
                    )
            });

            let class_obj_idx = program.add_object(Object::Class(Box::new(Class {
                name: fq_to_type_name(&fq_name),
                fields,
                description: class_desc,
                alias: class_alias,
                type_tag,
                ty_attr: TyAttr::default(),
                has_cleanup,
                generic_param_count: class_data.generic_params.len(),
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
            // The display- and short-name maps must agree with the emitted
            // runtime field indices used by the Class object above. Use a
            // closure that rebuilds the same ordering.
            let rebuild_indices = || {
                let merged =
                    collect_class_fields_with_implements(&pkg_info.namespace_path, class_data);
                let mut m = HashMap::new();
                for (idx, (name, _, _, _, _)) in merged.iter().enumerate() {
                    m.insert(name.clone(), idx);
                }
                m
            };
            classes.entry(display_name).or_insert_with(rebuild_indices);
            classes.entry(short_name).or_insert_with(rebuild_indices);
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

    // --- Pass 3b: Build interface objects + start the per-package structure ---
    // Each interface becomes an `Object::Interface` so impl rules can point at it
    // (`interface_head`) and packages can reference it by index. Only `.name` is
    // read at runtime today, so the signature (args/requires/assoc/fields/methods)
    // is left empty and filled when reflection needs it. `program_packages` is the
    // per-package structure the loader builds `Object::Package` + `vm.packages`
    // from; `build_packages` fills in each package's impl rules below.
    let mut interface_object_indices: HashMap<baml_type::TypeName, usize> = HashMap::new();
    let mut program_packages: indexmap::IndexMap<Name, bex_vm_types::types::ProgramPackage> =
        indexmap::IndexMap::new();
    for file in &all_files {
        let item_tree = file_item_tree(db, *file);
        let pkg_info = file_package(db, *file);
        for (iface_id, iface_data) in &item_tree.interfaces {
            let iface_tn = baml_compiler2_tir::lower_type_expr::qualify_def(
                db,
                Definition::Interface(InterfaceLoc::new(db, *file, *iface_id)),
                &iface_data.name,
            );
            let iface_obj_idx = program.add_object(Object::Interface(Box::new(
                bex_vm_types::types::InterfaceDef {
                    name: iface_tn.clone(),
                    args: Vec::new(),
                    requires: Vec::new(),
                    assoc: Vec::new(),
                    fields: Vec::new(),
                    methods: Vec::new(),
                },
            )));
            interface_object_indices.insert(iface_tn, iface_obj_idx);
            program_packages
                .entry(pkg_info.package.clone())
                .or_default()
                .interfaces
                .insert(
                    bex_vm_types::types::LocalName {
                        namespace: pkg_info.namespace_path.clone(),
                        name: iface_data.name.clone(),
                    },
                    ObjectIndex::from_raw(iface_obj_idx),
                );
        }
    }

    // --- Pass 4: Compile each function ---
    for file in &all_files {
        let line_starts = build_line_starts(file.text(db));
        let item_tree = file_item_tree(db, *file);
        let pkg_info_pass4 = file_package(db, *file);
        let is_builtin_file = file.path(db).to_string_lossy().starts_with("<builtin>/");
        let cache_pass4 = &alias_caches[&pkg_info_pass4.package];
        for (local_id, func_data) in &item_tree.functions {
            let func_loc = FunctionLoc::new(db, *file, *local_id);
            let mir = lower_function(db, func_loc, opt);
            let fq_name = mir.item_ref.to_string();

            let mut compiled_fn = match &mir.kind {
                MirFunctionKind::Bytecode(body) => {
                    // Compile lambda children first, collecting their ObjectPool indices.
                    let source_file = file.path(db).display().to_string();
                    let empty_capture_types = Vec::new();
                    let empty_spawn_capture_indices = HashSet::new();
                    let lambda_info = compile_lambdas_flat(
                        &mir.lambdas,
                        Some(body),
                        &empty_capture_types,
                        &empty_spawn_capture_indices,
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
                        capture_types: &empty_capture_types,
                        spawn_capture_indices: &empty_spawn_capture_indices,
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
                MirFunctionKind::Builtin(BuiltinKind::AwaitAny) => {
                    // BEP-034 `__await_any` has no callable body — call sites
                    // lower to a `Terminator::AwaitAny` suspend point directly.
                    // Skip compilation entirely (like an intrinsic).
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
                        return_type: baml_type::RuntimeTy::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                        param_names: Vec::new(),
                        param_types: Vec::new(),
                        param_has_default: Vec::new(),
                        display_type_params: Vec::new(),
                        display_param_types: Vec::new(),
                        display_return_type: "null".to_string(),
                        throws_type: None,
                        origin: FunctionOrigin::Builtin,
                        body_meta: None,
                        capture: FunctionCaptureProps::disabled(),
                        function_id: 0, // assigned at engine init (interim provider)
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
                    return_type: baml_type::RuntimeTy::Null {
                        attr: baml_type::TyAttr::default(),
                    },
                    param_names: Vec::new(),
                    param_types: Vec::new(),
                    param_has_default: Vec::new(),
                    display_type_params: Vec::new(),
                    display_param_types: Vec::new(),
                    display_return_type: "null".to_string(),
                    throws_type: None,
                    origin: FunctionOrigin::Builtin,
                    body_meta: None,
                    capture: FunctionCaptureProps::disabled(),
                    function_id: 0, // assigned at engine init (interim provider)
                },
            };

            // Set function metadata from signature
            let parameter_defaults = baml_compiler2_ppir::function_parameter_defaults(db, func_loc);
            let signature_metadata = compute_function_metadata_from_item_tree(
                db,
                *file,
                *local_id,
                func_data,
                &parameter_defaults,
                cache_pass4,
            );
            compiled_fn.return_type = signature_metadata.return_type;
            compiled_fn.param_names = signature_metadata.param_names;
            compiled_fn.param_types = signature_metadata.param_types;
            compiled_fn.param_has_default = signature_metadata.param_has_default;
            compiled_fn.display_type_params = signature_metadata.display_type_params;
            compiled_fn.display_param_types = signature_metadata.display_param_types;
            compiled_fn.display_return_type = signature_metadata.display_return_type;

            // Set inferred throws type from TIR throw inference
            compiled_fn.throws_type = compute_throws_type(db, *file, &func_data.name, cache_pass4);
            compiled_fn.origin =
                emitted_function_origin(&fq_name, is_builtin_file, func_data.origin);

            // Set LLM-specific body_meta if this is an LLM function
            if let Some(baml_compiler2_ast::DeclarativeMeta::Llm(llm_meta)) =
                &func_data.declarative_meta
            {
                // NOTE (canary merge): canary removed the runtime `Function.stream_return_type`
                // field and its plumbing (the pre-existing streaming infra from PRs #3362/#3755).
                // The stream return type is now carried by the synthesized `$stream` companion's
                // own `return_type` (see ppir's `companion_stream_return_type`), so the old
                // emit-side pre-computation block was dropped. BEP-049 M5e stream-path rendering
                // of `ctx.output_format` should be re-verified against canary's streaming.
                if let Some(client) = &llm_meta.client {
                    // New-mode (BEP-049 M5) functions have no Jinja `prompt`
                    // text — the compiled closure renders it — but they still
                    // need a registry entry so `get_return_type` /
                    // `get_stream_return_type` (used by `__make_stream` and the
                    // streaming `Context`) resolve by name. The empty template
                    // is never read for them (their `prompt_closure` is non-null,
                    // so the Jinja `get_jinja_template` branch is skipped).
                    let prompt_template = llm_meta
                        .prompt
                        .as_ref()
                        .map(|p| p.text.clone())
                        .unwrap_or_default();
                    compiled_fn.body_meta = Some(FunctionMeta::Llm {
                        prompt_template,
                        client: client.to_string(),
                    });
                    compiled_fn.capture = FunctionCaptureProps::disabled()
                        .with_auto(CaptureCategory::Input)
                        .with_auto(CaptureCategory::Output)
                        .with_auto(CaptureCategory::Error);
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
    // Cross-file aggregation of per-file $init_test_<path> functions.
    // This must happen at emit level because:
    //   - AST layer (lower_file_with_path) is per-file only
    //   - MIR (lower_function) is per-function only
    //   - Only emit iterates all_files and has the compiled program
    // Follows the exact $init pattern at Pass 4.5 above.
    {
        // Discover per-file $init_test_<path> functions using structured
        // compiler metadata (HIR item trees), group by package.
        let mut pkg_init_tests: HashMap<String, Vec<(String, usize)>> = HashMap::new();

        for file in &all_files {
            let item_tree = file_item_tree(db, *file);
            let pkg_info = file_package(db, *file);
            for local_id in item_tree.functions.keys() {
                let func_loc = FunctionLoc::new(db, *file, *local_id);
                let fq_name = def_to_item_ref(db, Definition::Function(func_loc)).to_string();
                // Match per-file $init_test_<path> functions synthesized by
                // `synthesize_init_test_function`. The trailing underscore in the
                // filter is intentional: all real files produce a path-derived
                // suffix. The no-path branch in lower_cst.rs produces bare
                // `$init_test` (no suffix), but that only runs in unit tests, PPIR
                // intermediate processing, and codegen — none of which produce
                // functions that reach program.function_indices at emit time. So
                // `contains("$init_test_")` safely matches only per-file functions
                // without risk of collision with the chainer name we're about to
                // synthesize.
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
                return_type: baml_type::RuntimeTy::Null {
                    attr: baml_type::TyAttr::default(),
                },
                param_names: vec!["registry".to_string()],
                param_types: vec![baml_type::RuntimeTy::unknown()], // type not needed for chainer dispatch
                param_has_default: vec![false],
                display_type_params: Vec::new(),
                display_param_types: vec!["unknown".to_string()],
                display_return_type: "null".to_string(),
                throws_type: None,
                origin: FunctionOrigin::Internal,
                body_meta: None,
                capture: FunctionCaptureProps::disabled(),
                function_id: 0, // assigned at engine init (interim provider)
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
    // by `convert_tir_ty_for_runtime`. This is required for correct output_format rendering at runtime.
    for cache in alias_caches.values() {
        for (qtn, tir_ty) in &cache.aliases {
            if cache.recursive.contains(qtn) {
                let mir_ty = cache.convert(tir_ty);
                let type_name = qtn.clone();
                program.recursive_type_alias_defs.insert(type_name, mir_ty);
            }
        }
    }

    build_packages(
        db,
        &all_files,
        &alias_caches,
        &program.function_indices,
        &interface_object_indices,
        &mut program_packages,
    );
    program_packages.sort_keys();
    program.packages = program_packages;

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
                element_type: baml_type::RuntimeTy::Null {
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
                key_type: baml_type::RuntimeTy::String {
                    attr: baml_type::TyAttr::default(),
                },
                value_type: baml_type::RuntimeTy::Null {
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
) -> Option<baml_type::RuntimeTy> {
    let pkg_info = file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package);
    let throw_sets = baml_compiler2_tir::throw_inference::function_throw_sets(db, pkg_id);

    let key =
        baml_compiler2_tir::throw_inference::throw_set_key(&pkg_info.namespace_path, func_name);

    let facts = throw_sets.transitive_for(&key)?;
    if facts.is_empty() {
        return None;
    }

    let converted: Vec<baml_type::RuntimeTy> =
        facts.iter().map(|tir_ty| cache.convert(tir_ty)).collect();

    if converted.len() == 1 {
        Some(converted.into_iter().next().unwrap())
    } else {
        Some(baml_type::RuntimeTy::Union(
            converted,
            baml_type::TyAttr::default(),
        ))
    }
}

#[derive(Debug, Clone)]
struct FunctionSignatureMetadata {
    param_names: Vec<String>,
    param_types: Vec<baml_type::RuntimeTy>,
    param_has_default: Vec<bool>,
    return_type: baml_type::RuntimeTy,
    display_type_params: Vec<String>,
    display_param_types: Vec<String>,
    display_return_type: String,
}

fn type_expr_for_name_with_generic_args(name: Name, generic_params: &[Name]) -> TypeExpr {
    baml_compiler2_ast::TypeExprKind::Path {
        segments: vec![name],
        generic_args: generic_params
            .iter()
            .cloned()
            .map(baml_compiler2_tir::lower_type_expr::type_expr_for_name)
            .collect(),
        associated_type_bindings: Vec::new(),
        attrs: Vec::new(),
    }
    .at(baml_compiler2_ast::TextRange::default())
}

/// Extract runtime and display signature metadata from an `item_tree` Function.
///
/// Type resolution delegates to TIR's `lower_type_expr` (single source of truth)
/// then converts via MIR's `convert_tir_ty_for_runtime` to produce runtime `baml_type::RuntimeTy`.
/// The display fields keep generic type variables and unresolved projections
/// intact for self-documenting surfaces like `baml run --list`.
fn compute_function_metadata_from_item_tree(
    db: &dyn baml_compiler2_mir::Db,
    file: baml_base::SourceFile,
    func_id: baml_compiler2_hir::ids::LocalItemId<baml_compiler2_hir::ids::FunctionMarker>,
    func_data: &baml_compiler2_hir::item_tree::Function,
    parameter_defaults: &baml_compiler2_hir::signature::FunctionParameterDefaults,
    cache: &ResolvedAliases,
) -> FunctionSignatureMetadata {
    let param_names: Vec<String> = func_data
        .params
        .iter()
        .map(|p| p.name.to_string())
        .collect();
    let param_has_default: Vec<bool> = parameter_defaults
        .params
        .iter()
        .map(Option::is_some)
        .collect();

    let pkg_info = file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package);
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let null_ty = || baml_type::RuntimeTy::Null {
        attr: baml_type::TyAttr::default(),
    };

    let item_tree = file_item_tree(db, file);
    let enclosing_impl = item_tree
        .implements_for
        .iter()
        .find(|imp| imp.methods.contains(&func_id));
    let enclosing_class = item_tree
        .classes
        .values()
        .find(|class_data| class_data.methods.contains(&func_id));
    let enclosing_interface = item_tree
        .interfaces
        .values()
        .find(|iface_data| iface_data.default_methods.contains(&func_id));
    let self_replacement = enclosing_impl
        .map(|imp| imp.for_target.clone())
        .or_else(|| {
            enclosing_class.map(|class_data| {
                type_expr_for_name_with_generic_args(
                    class_data.name.clone(),
                    &class_data.generic_params,
                )
            })
        })
        .or_else(|| {
            enclosing_interface.map(|iface_data| {
                type_expr_for_name_with_generic_args(
                    iface_data.name.clone(),
                    &iface_data.generic_params,
                )
            })
        });

    // For methods on generic classes/interfaces/impls, the enclosing generic
    // params are in scope inside the method signature. Mirror
    // `MirLowerer::enclosing_generic_params`: enclosing params come first, then
    // function-level params.
    let (scoped_generic_param_names, scoped_generic_bound_exprs) = if let Some(imp) = enclosing_impl
    {
        let mut names = imp.generic_params.clone();
        names.extend(func_data.generic_params.iter().cloned());
        let mut bounds = imp.generic_param_bounds.clone();
        bounds.extend(func_data.generic_param_bounds.iter().cloned());
        (names, bounds)
    } else if let Some(iface_data) = enclosing_interface {
        let mut names = iface_data.generic_params.clone();
        names.extend(func_data.generic_params.iter().cloned());
        let mut bounds = iface_data.generic_param_bounds.clone();
        bounds.extend(func_data.generic_param_bounds.iter().cloned());
        (names, bounds)
    } else {
        let mut names = enclosing_class
            .map(|class_data| class_data.generic_params.clone())
            .unwrap_or_default();
        names.extend(func_data.generic_params.iter().cloned());
        let mut bounds = enclosing_class
            .map(|class_data| class_data.generic_param_bounds.clone())
            .unwrap_or_default();
        bounds.extend(func_data.generic_param_bounds.iter().cloned());
        (names, bounds)
    };
    let enclosing_generics = scoped_generic_param_names.clone();

    // A method declared inside an interface resolves its associated types
    // (`Item`/`Error`) and `Self` against the rigid `Self` type variable, the
    // same way the method body does (`infer_scope_types`). Build those bindings
    // once so the signature keeps associated-type references as faithful
    // `Self.<name>` projections; a bare `lower_type_expr_in_ns` would erase each
    // one to `Ty::Unknown`, which then trips the runtime lowering boundary.
    // Empty for non-interface methods (the `self_replacement` path below
    // handles class/impl receivers).
    let interface_signature_bindings: rustc_hash::FxHashMap<Name, baml_compiler2_tir::ty::Ty> =
        match enclosing_interface.and_then(|iface_data| {
            item_tree
                .interfaces
                .iter()
                .find(|(_, data)| data.default_methods.contains(&func_id))
                .map(|(iface_id, _)| {
                    (
                        baml_compiler2_hir::loc::InterfaceLoc::new(db, file, *iface_id),
                        iface_data,
                    )
                })
        }) {
            Some((iface_loc, iface_data)) => {
                let mut bindings: rustc_hash::FxHashMap<Name, baml_compiler2_tir::ty::Ty> =
                    enclosing_generics
                        .iter()
                        .map(|p| {
                            (
                                p.clone(),
                                baml_compiler2_tir::ty::Ty::TypeVar(
                                    p.clone(),
                                    baml_type::TyAttr::default(),
                                ),
                            )
                        })
                        .collect();
                bindings.extend(
                    baml_compiler2_tir::inference::interface_self_projection_bindings(
                        db,
                        iface_loc,
                        iface_data,
                        pkg_items,
                        &pkg_info.namespace_path,
                    ),
                );
                bindings
            }
            None => rustc_hash::FxHashMap::default(),
        };

    let lower_tir_type = |te: &TypeExpr| -> baml_compiler2_tir::ty::Ty {
        let mut diags = Vec::new();
        if enclosing_interface.is_some() {
            // Interface method: lower with the `Self`/associated-type bindings so
            // `Item`/`Error`/`Self` resolve to projections and type variables
            // rather than `Ty::Unknown`.
            return baml_compiler2_tir::generics::lower_type_expr_with_generics(
                db,
                te,
                pkg_items,
                &pkg_info.namespace_path,
                &interface_signature_bindings,
                &mut diags,
            );
        }
        // Use `lower_type_expr_in_ns` so unqualified references (e.g. `MyLorem`
        // in a function signature under `ns_lorem/`) resolve against the
        // defining file's namespace before falling back to the package root.
        // `lower_type_expr` passes `&[]` as the ns context, which would lose
        // parameter types to `Ty::Unknown` → runtime `unknown` for any
        // non-root-ns class — surfacing as "expected instance, got map" in the
        // runtime because the coercion layer can't see the declared type.
        let resolved_te = if let Some(replacement) = &self_replacement {
            baml_compiler2_tir::lower_type_expr::substitute_self_in(te, replacement)
        } else {
            te.clone()
        };
        baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
            db,
            &resolved_te,
            pkg_items,
            &pkg_info.namespace_path,
            &enclosing_generics,
            &mut diags,
        )
    };

    let raw_generic_param_bounds: HashMap<Name, baml_compiler2_tir::ty::Ty> =
        scoped_generic_param_names
            .iter()
            .enumerate()
            .filter_map(|(idx, name)| {
                let bound_te = scoped_generic_bound_exprs.get(idx)?.as_ref()?;
                let mut diags = Vec::new();
                // Lower bounds with the same `Self`/associated-type bindings as
                // the signature (mirroring `lower_tir_type`) so an interface
                // default-method bound like `U extends Self.Item` resolves to a
                // projection instead of erasing `Self` to `Ty::Unknown` — which
                // the `diags.is_empty()` gate below would then silently drop from
                // the emitted metadata.
                let bound_ty = if enclosing_interface.is_some() {
                    baml_compiler2_tir::generics::lower_type_expr_with_generics(
                        db,
                        bound_te,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &interface_signature_bindings,
                        &mut diags,
                    )
                } else {
                    let resolved_te = if let Some(replacement) = &self_replacement {
                        baml_compiler2_tir::lower_type_expr::substitute_self_in(
                            bound_te,
                            replacement,
                        )
                    } else {
                        bound_te.clone()
                    };
                    baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                        db,
                        &resolved_te,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &enclosing_generics,
                        &mut diags,
                    )
                };
                diags.is_empty().then(|| (name.clone(), bound_ty))
            })
            .collect();
    let raw_bound_resolver =
        baml_compiler2_tir::associated_projection::AssociatedProjectionResolver::new(
            db,
            &cache.aliases,
            &raw_generic_param_bounds,
        );
    let generic_param_bounds: HashMap<Name, baml_compiler2_tir::ty::Ty> = raw_generic_param_bounds
        .iter()
        .map(|(name, bound)| (name.clone(), raw_bound_resolver.resolve_deep(bound)))
        .collect();

    let resolve_display_tir = |te: &TypeExpr| -> baml_compiler2_tir::ty::Ty {
        let tir_ty = lower_tir_type(te);
        baml_compiler2_tir::associated_projection::AssociatedProjectionResolver::new(
            db,
            &cache.aliases,
            &generic_param_bounds,
        )
        .resolve_deep(&tir_ty)
    };

    let runtime_from_display_tir = |tir_ty: &baml_compiler2_tir::ty::Ty| -> baml_type::RuntimeTy {
        // `resolve_display_tir` already resolved associated projections against
        // statically-known bounds. Convert what remains faithfully — type
        // variables and symbolic projections are carried by `RuntimeTy` so the
        // runtime can resolve them; erasing them to `unknown` would discard the
        // information needed to do that.
        cache.convert(tir_ty)
    };

    let display_type_params: Vec<String> = scoped_generic_param_names
        .iter()
        .map(|name| {
            if let Some(bound) = generic_param_bounds.get(name) {
                format!("{} extends {}", name.as_str(), bound.render_user_facing())
            } else {
                name.to_string()
            }
        })
        .collect();

    let mut param_types = Vec::with_capacity(func_data.params.len());
    let mut display_param_types = Vec::with_capacity(func_data.params.len());
    for param in &func_data.params {
        let resolved = if let Some(te) = &param.type_expr {
            Some(resolve_display_tir(te))
        } else if param.name.as_str() == "self" {
            self_replacement.as_ref().map(&resolve_display_tir)
        } else {
            None
        };
        if let Some(tir_ty) = resolved {
            display_param_types.push(tir_ty.render_user_facing());
            param_types.push(runtime_from_display_tir(&tir_ty));
        } else {
            display_param_types.push("null".to_string());
            param_types.push(null_ty());
        }
    }

    let resolved_return_type = func_data.return_type.as_ref().map(resolve_display_tir);
    let (return_type, display_return_type) = if let Some(tir_ty) = resolved_return_type {
        (
            runtime_from_display_tir(&tir_ty),
            tir_ty.render_user_facing(),
        )
    } else {
        (null_ty(), "null".to_string())
    };

    FunctionSignatureMetadata {
        param_names,
        param_types,
        param_has_default,
        return_type,
        display_type_params,
        display_param_types,
        display_return_type,
    }
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

#[derive(Clone, Default)]
struct LambdaCaptureInfo {
    capture_types: Vec<RuntimeTy>,
    spawn_capture_indices: HashSet<usize>,
}

fn unknown_capture_ty() -> RuntimeTy {
    RuntimeTy::BuiltinUnknown {
        attr: TyAttr::default(),
    }
}

fn local_def_rvalue(body: &MirFunctionBody, local: Local) -> Option<&Rvalue> {
    body.blocks
        .iter()
        .flat_map(|block| &block.statements)
        .find_map(|statement| match &statement.kind {
            StatementKind::Assign {
                destination: Place::Local(dest),
                value,
            } if *dest == local => Some(value),
            _ => None,
        })
}

fn resolve_capture_operand_type(
    body: &MirFunctionBody,
    parent_capture_types: &[RuntimeTy],
    operand: &Operand,
) -> Option<RuntimeTy> {
    match operand {
        Operand::Constant(c) => match c {
            baml_compiler2_mir::Constant::Int(_) => Some(RuntimeTy::int()),
            baml_compiler2_mir::Constant::Bigint(_) => Some(RuntimeTy::bigint()),
            baml_compiler2_mir::Constant::Float(_) => Some(RuntimeTy::float()),
            baml_compiler2_mir::Constant::String(_) => Some(RuntimeTy::string()),
            baml_compiler2_mir::Constant::Bool(_) => Some(RuntimeTy::bool()),
            baml_compiler2_mir::Constant::Null => Some(RuntimeTy::null()),
            _ => None,
        },
        Operand::Copy(place) | Operand::Move(place) => {
            resolve_capture_place_type(body, parent_capture_types, place)
        }
    }
}

fn resolve_capture_place_type(
    body: &MirFunctionBody,
    parent_capture_types: &[RuntimeTy],
    place: &Place,
) -> Option<RuntimeTy> {
    match place {
        Place::Local(local) => body.locals.get(local.0).map(|decl| decl.ty.clone()),
        Place::Capture(idx) => parent_capture_types.get(*idx).cloned(),
        Place::Field { .. } | Place::Index { .. } => None,
    }
}

fn operand_reads_spawn_capture(
    body: &MirFunctionBody,
    parent_spawn_capture_indices: &HashSet<usize>,
    operand: &Operand,
    seen: &mut HashSet<Local>,
) -> bool {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => {
            place_reads_spawn_capture(body, parent_spawn_capture_indices, place, seen)
        }
        Operand::Constant(_) => false,
    }
}

fn place_reads_spawn_capture(
    body: &MirFunctionBody,
    parent_spawn_capture_indices: &HashSet<usize>,
    place: &Place,
    seen: &mut HashSet<Local>,
) -> bool {
    match place {
        Place::Local(local) => {
            if !seen.insert(*local) {
                return false;
            }
            match local_def_rvalue(body, *local) {
                Some(Rvalue::Use(operand)) => {
                    operand_reads_spawn_capture(body, parent_spawn_capture_indices, operand, seen)
                }
                _ => false,
            }
        }
        Place::Capture(idx) => parent_spawn_capture_indices.contains(idx),
        Place::Field { base, .. } => {
            place_reads_spawn_capture(body, parent_spawn_capture_indices, base, seen)
        }
        Place::Index { base, index, .. } => {
            place_reads_spawn_capture(body, parent_spawn_capture_indices, base, seen)
                || place_reads_spawn_capture(
                    body,
                    parent_spawn_capture_indices,
                    &Place::Local(*index),
                    seen,
                )
        }
    }
}

fn make_closure_for_operand<'a>(
    body: &'a MirFunctionBody,
    operand: &'a Operand,
    seen: &mut HashSet<Local>,
) -> Option<(usize, &'a [Operand])> {
    match operand {
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
            if !seen.insert(*local) {
                return None;
            }
            match local_def_rvalue(body, *local)? {
                Rvalue::MakeClosure {
                    lambda_idx,
                    captures,
                    ..
                } => Some((*lambda_idx, captures)),
                Rvalue::Use(operand) => make_closure_for_operand(body, operand, seen),
                _ => None,
            }
        }
        _ => None,
    }
}

fn mark_spawned_closure_operand(
    body: &MirFunctionBody,
    infos: &mut [LambdaCaptureInfo],
    operand: &Operand,
    seen_locals: &mut HashSet<Local>,
    seen_lambdas: &mut HashSet<usize>,
) {
    let Some((lambda_idx, captures)) = make_closure_for_operand(body, operand, seen_locals) else {
        return;
    };
    let Some(info) = infos.get_mut(lambda_idx) else {
        return;
    };

    if !seen_lambdas.insert(lambda_idx) {
        return;
    }

    info.spawn_capture_indices.extend(0..captures.len());

    for capture in captures {
        mark_spawned_closure_operand(body, infos, capture, seen_locals, seen_lambdas);
    }
}

fn collect_lambda_capture_infos(
    body: &MirFunctionBody,
    lambda_count: usize,
    parent_capture_types: &[RuntimeTy],
    parent_spawn_capture_indices: &HashSet<usize>,
) -> Vec<LambdaCaptureInfo> {
    let mut infos = vec![LambdaCaptureInfo::default(); lambda_count];

    for block in &body.blocks {
        for statement in &block.statements {
            let StatementKind::Assign { value, .. } = &statement.kind else {
                continue;
            };
            let Rvalue::MakeClosure {
                lambda_idx,
                captures,
                ..
            } = value
            else {
                continue;
            };
            let Some(info) = infos.get_mut(*lambda_idx) else {
                continue;
            };

            info.capture_types = captures
                .iter()
                .map(|capture| {
                    resolve_capture_operand_type(body, parent_capture_types, capture)
                        .unwrap_or_else(unknown_capture_ty)
                })
                .collect();

            for (capture_idx, capture) in captures.iter().enumerate() {
                if operand_reads_spawn_capture(
                    body,
                    parent_spawn_capture_indices,
                    capture,
                    &mut HashSet::new(),
                ) {
                    info.spawn_capture_indices.insert(capture_idx);
                }
            }
        }
    }

    for block in &body.blocks {
        let Some(Terminator::Spawn { closure, .. }) = &block.terminator else {
            continue;
        };
        mark_spawned_closure_operand(
            body,
            &mut infos,
            closure,
            &mut HashSet::new(),
            &mut HashSet::new(),
        );
    }

    infos
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
    parent_body: Option<&MirFunctionBody>,
    parent_capture_types: &[RuntimeTy],
    parent_spawn_capture_indices: &HashSet<usize>,
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
    let capture_infos = parent_body.map_or_else(
        || vec![LambdaCaptureInfo::default(); lambdas.len()],
        |body| {
            collect_lambda_capture_infos(
                body,
                lambdas.len(),
                parent_capture_types,
                parent_spawn_capture_indices,
            )
        },
    );
    let mut result = Vec::with_capacity(lambdas.len());
    for (lambda_idx, lambda) in lambdas.iter().enumerate() {
        let capture_info = capture_infos.get(lambda_idx).cloned().unwrap_or_default();
        let lambda_name = lambda.item_ref.to_string();
        let obj_idx = match &lambda.kind {
            MirFunctionKind::Bytecode(body) => {
                // Recursively compile any nested lambdas within this lambda.
                let nested_info = compile_lambdas_flat(
                    &lambda.lambdas,
                    Some(body),
                    &capture_info.capture_types,
                    &capture_info.spawn_capture_indices,
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
                    capture_types: &capture_info.capture_types,
                    spawn_capture_indices: &capture_info.spawn_capture_indices,
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
                let empty_capture_types = Vec::new();
                let empty_spawn_capture_indices = HashSet::new();
                let lambda_info = compile_lambdas_flat(
                    &lambdas,
                    Some(&mir_body),
                    &empty_capture_types,
                    &empty_spawn_capture_indices,
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
                    capture_types: &empty_capture_types,
                    spawn_capture_indices: &empty_spawn_capture_indices,
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
                    return_type: baml_type::RuntimeTy::Null {
                        attr: baml_type::TyAttr::default(),
                    },
                    param_names: Vec::new(),
                    param_types: Vec::new(),
                    param_has_default: Vec::new(),
                    display_type_params: Vec::new(),
                    display_param_types: Vec::new(),
                    display_return_type: "null".to_string(),
                    throws_type: None,
                    origin: FunctionOrigin::Internal,
                    body_meta: None,
                    capture: FunctionCaptureProps::disabled(),
                    function_id: 0, // assigned at engine init (interim provider)
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
        return_type: baml_type::RuntimeTy::Null {
            attr: baml_type::TyAttr::default(),
        },
        param_names: Vec::new(),
        param_types: Vec::new(),
        param_has_default: Vec::new(),
        display_type_params: Vec::new(),
        display_param_types: Vec::new(),
        display_return_type: "null".to_string(),
        throws_type: None,
        origin: FunctionOrigin::Internal,
        body_meta: None,
        capture: FunctionCaptureProps::disabled(),
        function_id: 0, // assigned at engine init (interim provider)
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
            generic_param_bounds: Vec::new(),
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
            is_tagged_template_tag: false,
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

        let metadata = compute_function_metadata_from_item_tree(
            &db,
            file,
            function_id,
            &func_data,
            &parameter_defaults,
            &cache,
        );

        assert_eq!(metadata.param_has_default, vec![false, true, false]);
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
