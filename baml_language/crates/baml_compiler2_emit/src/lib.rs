//! Code generation for BAML (compiler2 pipeline).
//!
//! Compiles MIR2 to bytecode for the BAML VM using stackification.

mod analysis;
mod emit;
mod pull_semantics;
mod stack_carry;
#[cfg(any(debug_assertions, test))]
mod verifier;

use std::collections::{HashMap, HashSet};

pub use analysis::OptLevel;
use baml_base::{Name, Span};
use baml_compiler2_ast::{TypeExpr, parse_string_attr_value};
use baml_compiler2_hir::{
    compiler2_all_files,
    contributions::Definition,
    file_package::file_package,
    loc::{FunctionLoc, LetLoc},
    package::PackageId,
};
use baml_compiler2_mir::{
    BuiltinKind, Local, MirFunctionBody, MirFunctionKind, Operand, Place, ResolvedAliases, Rvalue,
    StatementKind, Terminator, def_to_item_ref, lower_function, lower_let_body,
};
// PPIR item-data firewall (canonical / post-expansion view, including synthetic
// `*$stream` items) — enumeration + lookup queries in place of the raw item tree.
use baml_compiler2_ppir::{
    function_body,
    item_data::{
        GenericParamData, class_data, enum_data, file_classes, file_enums, file_free_impls,
        file_functions, file_interfaces, file_lets, file_tests, function_data, function_llm_meta,
        impl_block_data, interface_data, method_interface_target, test_data,
    },
};
use baml_type::{ParamTy, RuntimeTy, TyAttr};
use bex_vm_types::{
    Bytecode, CaptureCategory, Class, ClassField, ConstValue, Enum, EnumVariant, Function,
    FunctionCaptureProps, FunctionKind, FunctionMeta, FunctionOrigin, GlobalIndex, Instruction,
    Object, ObjectIndex, ObjectPool, Program,
    unit::{
        CompilationUnit, LocalRef, ProgramImplRuleFrag, ProgramMethodImplFrag, ProgramPackageFrag,
        Symbol, SymbolKind,
    },
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

/// Build the runtime [`InterfaceDef`](bex_vm_types::types::InterfaceDef) signature
/// — generic-param bounds, `requires`, associated-type bounds, fields, and method
/// signatures — for one interface, from its item-tree declaration.
///
/// Type expressions are lowered in the interface's own scope (so its generic
/// params resolve to `TypeVar`s) and narrowed to runtime types via
/// [`baml_type::lower_to_runtime`] — the faithful converter that preserves type
/// vars for reflection, NOT the value-erasing `convert_tir_ty_for_runtime`. Bound
/// and `requires` targets become [`baml_type::RuntimeInterface`] (via
/// `RuntimeInterface::new`, which canonicalizes associated-binding order).
fn build_interface_def(
    db: &dyn baml_compiler2_mir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'_>,
    iface_tn: baml_type::TypeName,
    // Minted by the caller through `claim_type_tag`, not derived here, so every
    // head passes the one collision detector.
    type_tag: baml_type::typetag::TypeTag,
    resolved: &ResolvedAliases,
) -> bex_vm_types::types::InterfaceDef {
    use baml_compiler2_hir::type_ref::{TypeRefId, TypeRefStore};
    use baml_compiler2_ppir::item_data::{FunctionParamData, function_data, interface_data};
    use baml_type::RuntimeInterface;
    use bex_vm_types::types::{InterfaceDef, InterfaceFieldDef, InterfaceMethodDef};

    let file = iface_loc.file(db);
    let interface = interface_data(db, iface_loc);
    let generics = &interface.generic_params;
    let interface_frame_params = baml_compiler2_hir_ty::lower::interface_frame(db, iface_loc);
    // The interface scope's own param env: `Self` (slot 0) bounded by the
    // interface itself, declared param bounds, and the associated slots -
    // the single env every interface-scoped lowering shares, so `Self.Item`
    // projections resolve here exactly as in signature lowering.
    let decl_ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file)
        .with_frame(interface_frame_params.clone())
        .with_bounds(baml_compiler2_hir_ty::lower::interface_scope_bounds(
            db, iface_loc,
        ));

    // Lower a type ref in `ctx` and narrow to a runtime type. Diagnostics are
    // not collected: emit only runs on a checked program, so the declaration
    // is already validated. `lower_to_runtime` rejects only the
    // error-recovery sentinels, which a checked program cannot contain, so a
    // failure means a compiler bug and must not be papered over by dropping
    // the entry (that would renumber positional arguments).
    let lower_rt = |ctx: &baml_compiler2_hir_ty::lower::LowerCtx<'_>,
                    store: &TypeRefStore,
                    id: TypeRefId|
     -> bex_vm_types::RuntimeTy {
        let ty = baml_compiler2_hir_ty::lower::reject_holes(&ctx.lower_type_ref(store, id));
        let runtime = baml_type::lower_to_runtime(&ty, resolved).unwrap_or_else(|e| {
            unreachable!("interface `{iface_tn}` declares a non-runtime type: {e:?}")
        });
        bex_vm_types::anchor_runtime_ty(&runtime)
    };
    // Lower an interface bound / `requires` target / associated-type bound.
    // These are constraint heads: hir keeps written pins only (no eager
    // default realization). A target that is not an interface was rejected
    // upstream (E0145 / E0133) and yields `None`.
    let lower_iface = |ctx: &baml_compiler2_hir_ty::lower::LowerCtx<'_>,
                       store: &TypeRefStore,
                       id: TypeRefId|
     -> Option<bex_vm_types::RuntimeInterface> {
        // ConstraintHead, per the contract above: the default (existential)
        // position eagerly realizes omitted associated defaults and fills
        // the rest with Error sentinels — a bound like `type A extends
        // Iface` with unpinned associated types would panic `to_runtime`.
        let lowered = baml_compiler2_hir_ty::lower::reject_holes(&ctx.lower_type_ref_at(
            store,
            id,
            baml_compiler2_hir_ty::lower::TypePosition::ConstraintHead,
        ));
        let baml_type::Ty::Interface(qtn, args, assoc, _) = lowered else {
            return None;
        };
        let to_runtime = |t: &baml_type::Ty| {
            baml_type::lower_to_runtime(t, resolved).unwrap_or_else(|e| {
                unreachable!("interface `{iface_tn}` declares a non-runtime constraint: {e:?}")
            })
        };
        let generics = args.iter().map(to_runtime).collect();
        let associated_types = assoc
            .iter()
            .map(|(n, t)| (n.clone(), to_runtime(t)))
            .collect();
        Some(bex_vm_types::anchor_interface(&RuntimeInterface::new(
            qtn,
            generics,
            associated_types,
        )))
    };
    // A method's runtime signature: Required params -> positional `args`,
    // optional (defaulted) params -> `kwargs`; the `self` receiver is
    // dropped; absent return/throws lower to `Void`.
    let build_method = |ctx: &baml_compiler2_hir_ty::lower::LowerCtx<'_>,
                        store: &TypeRefStore,
                        name: &Name,
                        params: &[FunctionParamData],
                        return_type: Option<TypeRefId>,
                        throws: Option<TypeRefId>|
     -> InterfaceMethodDef {
        // An untyped parameter is a syntax-level error, so it cannot reach
        // emit; the top type keeps the positional layout intact if one did.
        let unannotated = || bex_vm_types::RuntimeTy::Unknown {
            attr: TyAttr::default(),
        };
        let void = || bex_vm_types::RuntimeTy::Void {
            attr: TyAttr::default(),
        };
        let mut args = Vec::new();
        let mut kwargs = Vec::new();
        for p in params {
            if p.name.as_str() == "self" {
                continue;
            }
            let ty = p
                .type_ref
                .map_or_else(unannotated, |id| lower_rt(ctx, store, id));
            if p.has_default {
                kwargs.push((p.name.clone(), ty));
            } else {
                args.push(ty);
            }
        }
        InterfaceMethodDef {
            name: name.clone(),
            args,
            kwargs,
            returns: return_type.map_or_else(void, |id| lower_rt(ctx, store, id)),
            errors: throws.map_or_else(void, |id| lower_rt(ctx, store, id)),
            // Functions are pooled after interfaces, so the default's object
            // index is not known yet; `build_packages` back-fills it once the
            // pool is complete (the same pass that folds defaults into rules).
            default: None,
            default_fn: bex_vm_types::HeapPtr::null(),
        }
    };

    // `T extends A & B` is a conjunction; every conjunct that resolves to an
    // interface is emitted so the runtime enforces all of them.
    let store = &interface.type_refs;
    let args = generics
        .iter()
        .map(|declared| {
            let bounds = declared
                .bounds
                .iter()
                .filter_map(|&id| lower_iface(&decl_ctx, store, id))
                .collect();
            (declared.name.clone(), bounds)
        })
        .collect();
    let requires = interface
        .requires
        .iter()
        .filter_map(|&id| lower_iface(&decl_ctx, store, id))
        .collect();
    let assoc = interface
        .associated_types
        .iter()
        .filter_map(|at| {
            at.bound
                .and_then(|id| lower_iface(&decl_ctx, store, id))
                .map(|ri| (at.name.clone(), ri))
        })
        .collect();
    // This list is the interface's field *index space*: `RuntimeImplRule::field_links`
    // is baked parallel to it, so every declared field keeps its slot. A field always
    // carries a type — an untyped one is a syntax-level error that cannot reach emit.
    let fields = interface
        .fields
        .iter()
        .map(|f| InterfaceFieldDef {
            name: f.name.clone(),
            ty: lower_rt(&decl_ctx, store, f.type_ref),
        })
        .collect();
    let mut methods: Vec<InterfaceMethodDef> = interface
        .required_methods
        .iter()
        .map(|m| {
            let mut scope = interface_frame_params.clone();
            let own_names: Vec<Name> = m.generic_params.iter().map(|p| p.name.clone()).collect();
            ParamTy::extend_frame(&mut scope, &own_names);
            let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file)
                .with_frame(scope)
                .with_bounds(baml_compiler2_hir_ty::lower::interface_scope_bounds(
                    db, iface_loc,
                ));
            build_method(&ctx, store, &m.name, &m.params, m.return_type, m.throws)
        })
        .collect();
    methods.extend(interface.default_methods.iter().map(|&loc| {
        let f = function_data(db, loc);
        // The method's full frame and bounds (its own params plus the
        // interface scope's, `function_generic_bounds`' interface arm).
        let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, loc.file(db))
            .with_frame(baml_compiler2_hir_ty::lower::function_generic_frame(
                db, loc,
            ))
            .with_bounds(baml_compiler2_hir_ty::lower::function_generic_bounds(
                db, loc,
            ));
        build_method(
            &ctx,
            &f.type_refs,
            &f.name,
            &f.params,
            f.return_type,
            f.throws,
        )
    }));

    InterfaceDef {
        type_tag,
        name: iface_tn,
        args,
        requires,
        assoc,
        fields,
        methods,
        // Static declarations have no owning runtime package.
        owner: bex_vm_types::HeapPtr::null(),
    }
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
/// An interface's generic parameter names paired with its declared
/// associated-type members in order, each with its lowered default (if any) —
/// the [`build_packages`] prepass entry per assoc-carrying interface.
type IfaceAssocDecls = (ParamTy, Vec<ParamTy>, Vec<(Name, Option<baml_type::Ty>)>);

/// The source-less package surface captured before MIR/codegen starts.
///
/// Some interface leaves share salsa queries with function lowering. Capture
/// the artifact serially at the emit boundary so codegen scheduling cannot
/// affect the package metadata persisted in the executable image.
struct PackageExportArtifact {
    interface_blob: Vec<u8>,
    exported_names: Vec<bex_vm_types::types::LocalName>,
    functions: Vec<(bex_vm_types::types::LocalName, String)>,
}

/// Read-only whole-project facts consumed while assembling runtime packages.
struct PackageBuildMetadata<'a> {
    /// Field-name → slot for each emitted class, keyed by rendered FQN.
    class_field_indices: &'a HashMap<String, HashMap<String, usize>>,
    /// Typed source-less export artifacts captured before parallel codegen.
    package_exports: &'a indexmap::IndexMap<Name, PackageExportArtifact>,
}

fn external_call_target_name(
    target: &baml_compiler2_hir_ty::callable::ExternalCallTarget,
) -> String {
    use baml_compiler2_hir_ty::callable::ExternalCallTarget;
    let (package, namespace, owner, name) = match target {
        ExternalCallTarget::Free {
            package,
            namespace,
            name,
        } => (package, namespace.as_slice(), None, name),
        ExternalCallTarget::Method {
            package,
            namespace,
            class,
            name,
        } => (package, namespace.as_slice(), Some(class), name),
        ExternalCallTarget::Interface { interface, method } => (
            interface.package(),
            interface.namespace().as_slice(),
            Some(interface.name()),
            method,
        ),
    };
    let mut parts = Vec::with_capacity(2 + namespace.len());
    parts.push(package.as_str());
    parts.extend(namespace.iter().map(Name::as_str));
    if let Some(owner) = owner {
        parts.push(owner.as_str());
    }
    parts.push(name.as_str());
    parts.join(".")
}

fn capture_package_exports(
    db: &dyn baml_compiler2_mir::Db,
    all_files: &[baml_base::SourceFile],
) -> indexmap::IndexMap<Name, PackageExportArtifact> {
    let package_names: std::collections::BTreeSet<_> = all_files
        .iter()
        .map(|file| file_package(db, *file).package)
        .collect();
    package_names
        .into_iter()
        .map(|package_name| {
            let package_id = PackageId::new(db, package_name.clone());
            let interface =
                baml_compiler2_hir_ty::package_interface::package_interface(db, package_id);
            // Runtime compilers already own the exact stdlib sources, so only
            // mountable packages need to carry a serialized compiler surface.
            let interface_blob =
                if baml_builtins2::stdlib_package_names().contains(&package_name.as_str()) {
                    Vec::new()
                } else {
                    baml_artifact::encode(baml_artifact::ArtifactKind::PackageInterface, interface)
                        .expect("PackageInterface artifact serialization into Vec is infallible")
                };
            let functions = interface
                .functions
                .iter()
                .flat_map(|(namespace, functions)| {
                    functions.iter().map(|(name, function)| {
                        (
                            bex_vm_types::types::LocalName {
                                namespace: namespace.clone(),
                                name: name.clone(),
                            },
                            external_call_target_name(&function.target),
                        )
                    })
                })
                .collect::<Vec<_>>();
            let exported_names = interface
                .types
                .iter()
                .flat_map(|(namespace, types)| {
                    types.keys().map(|name| bex_vm_types::types::LocalName {
                        namespace: namespace.clone(),
                        name: name.clone(),
                    })
                })
                .chain(functions.iter().map(|(name, _)| name.clone()))
                .collect::<indexmap::IndexSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            (
                package_name,
                PackageExportArtifact {
                    interface_blob,
                    exported_names,
                    functions,
                },
            )
        })
        .collect()
}

fn build_packages(
    db: &dyn baml_compiler2_mir::Db,
    all_files: &[baml_base::SourceFile],
    alias_caches: &HashMap<Name, ResolvedAliases>,
    function_indices: &HashMap<String, usize>,
    interface_indices: &HashMap<baml_type::TypeName, usize>,
    // Field-name → slot for every emitted class, keyed by rendered fully-qualified
    // name. This is the *same* map the class pass built `Class::fields` from, threaded
    // in rather than recomputed: a second derivation of the layout that drifted would
    // make every virtual field access read the wrong slot, silently.
    metadata: &PackageBuildMetadata<'_>,
    program_packages: &mut indexmap::IndexMap<Name, bex_vm_types::types::ProgramPackage>,
) -> Vec<InterfaceDefaultBackfill> {
    use baml_compiler2_hir::type_ref::{TypeRefId, TypeRefStore};
    use baml_compiler2_hir_ty::lower::qualify_def;
    use baml_compiler2_ppir::item_data::{AssociatedTypeBindingData, ImplSubjectData};
    use baml_type as ty;
    use rustc_hash::FxHashMap;
    type BoundsMap = FxHashMap<ty::ParamTy, Vec<baml_type::Interface>>;
    use bex_vm_types::{
        ObjectIndex,
        types::{InterfaceBound, ProgramImplRule, ProgramMethodImpl},
    };

    type IfaceParts = (
        baml_type::TypeName,
        Vec<bex_vm_types::TyTemplate>,
        Vec<(Name, bex_vm_types::TyTemplate)>,
    );
    // Split a lowered interface type into its base `TypeName` plus its args /
    // associated bindings as `TyTemplate`s (generic params → `TypeArgRef`).
    fn split_interface(
        iface_ty: &ty::Ty,
        resolved: &ResolvedAliases,
        generics: &[ParamTy],
    ) -> Option<IfaceParts> {
        let ty::Ty::Interface(qtn, args, assoc, _) = iface_ty else {
            return None;
        };
        let arg_templates = args
            .iter()
            .map(|a| {
                bex_vm_types::anchor_template(&baml_compiler2_mir::tir2_to_template(
                    a, resolved, generics,
                ))
            })
            .collect();
        let assoc_templates = assoc
            .iter()
            .map(|(n, t)| {
                (
                    n.clone(),
                    bex_vm_types::anchor_template(&baml_compiler2_mir::tir2_to_template(
                        t, resolved, generics,
                    )),
                )
            })
            .collect();
        Some((qtn.clone(), arg_templates, assoc_templates))
    }

    let class_field_indices = metadata.class_field_indices;
    let package_exports = metadata.package_exports;

    // Resolve a function FQN to its emitted object index. `function_indices` holds
    // every function except `$compiler_intrinsic` / `$await_any` bodies, which
    // Pass 4 does not emit as callable objects — so a `None` here means the impl/
    // default method has such a body. The stdlib only ever uses those bodies on
    // free functions, never interface methods, so this is currently unreachable;
    // but that convention isn't enforced, so we drop just that method (losing a
    // dispatch, never adding a wrong one) rather than panic, and the `debug_assert`
    // catches any regression in the tested corpus.
    // TODO: make this unrepresentable — reject `$compiler_intrinsic`/`$await_any`
    // bodies on interface impl and default methods upstream (a check-time
    // diagnostic), after which this can become a hard `expect`.
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
    // Per interface, its generic parameter names and its declared
    // associated-type members *in order*, each with its default (lowered once
    // with symbolic `Self` by the shared query) where one is declared. Rule
    // construction bakes the default into every impl that leaves the member
    // unpinned, so the registry answers every declared member identically —
    // pinned or defaulted — and an inherited default's frame layout
    // (`[Self ++ interface generic args ++ associated types]`, matching MIR's
    // `enclosing_generic_params` for interface-owned bodies) carries a real
    // binding in every slot.
    let mut iface_assoc_decls: indexmap::IndexMap<baml_type::TypeName, IfaceAssocDecls> =
        indexmap::IndexMap::new();
    // Per field-bearing interface, its declared field names **in declaration order** —
    // the index space `RuntimeImplRule::field_links` is baked against, and the same
    // order `build_interface_def` gives `InterfaceDef::fields`. Interfaces with no
    // fields are absent (their impls get an empty table).
    let mut iface_field_decls: indexmap::IndexMap<baml_type::TypeName, Vec<Name>> =
        indexmap::IndexMap::new();
    for file in all_files {
        for &iface_loc in file_interfaces(db, *file) {
            let iface_data = interface_data(db, iface_loc);
            let iface_tn = qualify_def(db, Definition::Interface(iface_loc), &iface_data.name);
            if !iface_data.fields.is_empty() {
                iface_field_decls
                    .entry(iface_tn.clone())
                    .or_insert_with(|| iface_data.fields.iter().map(|f| f.name.clone()).collect());
            }
            if !iface_data.associated_types.is_empty() {
                iface_assoc_decls
                    .entry(iface_tn.clone())
                    .or_insert_with(|| {
                        let frame_params =
                            baml_compiler2_hir_ty::lower::interface_frame(db, iface_loc);
                        let self_param = frame_params
                            .first()
                            .expect("interface frame starts with Self")
                            .clone();
                        (
                            self_param,
                            baml_compiler2_hir_ty::lower::interface_declared_params(db, iface_loc),
                            iface_data
                                .associated_types
                                .iter()
                                .map(|assoc| {
                                    (
                                        assoc.name.clone(),
                                        baml_compiler2_hir_ty::interfaces::
                                            interface_associated_type_default(
                                                db,
                                                iface_loc,
                                                assoc.name.clone(),
                                            )
                                            .map(|(ty, _decl_site_diags)| ty),
                                    )
                                })
                                .collect(),
                        )
                    });
            }
            if iface_data.default_methods.is_empty() {
                continue;
            }
            let entry = iface_defaults.entry(iface_tn).or_default();
            for &m in &iface_data.default_methods {
                entry.insert(
                    function_data(db, m).name.clone(),
                    def_to_item_ref(db, Definition::Function(m)).to_string(),
                );
            }
        }
    }
    // Source-less interfaces participate in consumer-owned impl rules through
    // exactly the same runtime tables.  Seed their declaration facts from the
    // mounted artifact before walking the consumer's source blocks: otherwise
    // `class Local { implements dep.I {} }` would prove membership at check
    // time but emit neither inherited defaults nor virtual-field links.
    for package in baml_compiler2_hir::package::external_package_names(db) {
        let Some(interface) =
            baml_compiler2_hir_ty::package_interface::mounted_interface(db, &package)
        else {
            continue;
        };
        for exported in interface
            .types
            .values()
            .flat_map(|namespace| namespace.values())
        {
            let baml_compiler2_hir_ty::package_interface::ExportedType::Interface {
                qtn,
                self_param,
                generic_params,
                associated_types,
                fields,
                default_methods,
                ..
            } = exported
            else {
                continue;
            };
            if !fields.is_empty() {
                iface_field_decls
                    .entry(qtn.clone())
                    .or_insert_with(|| fields.iter().map(|(name, ..)| name.clone()).collect());
            }
            if !associated_types.is_empty() {
                iface_assoc_decls.entry(qtn.clone()).or_insert_with(|| {
                    (
                        self_param.clone(),
                        generic_params.clone(),
                        associated_types
                            .iter()
                            .map(|assoc| (assoc.name.clone(), assoc.default.clone()))
                            .collect(),
                    )
                });
            }
            if !default_methods.is_empty() {
                let defaults = iface_defaults.entry(qtn.clone()).or_default();
                for method in default_methods {
                    defaults
                        .entry(method.name.clone())
                        .or_insert_with(|| external_call_target_name(&method.target));
                }
            }
        }
    }
    // The frame an inherited default of `iface_tn` is invoked with, for a rule
    // implementing it at `for_ty_pattern` / `interface_args` / `interface_assoc`:
    // the implementor type (`Self`) at slot 0, then the interface's generic args,
    // then its associated types in declared order (all templates over the impl's
    // generics). `realize_frame` substitutes the rule's bound args — recovered by
    // matching `for_ty_pattern` against the receiver's concrete type — so slot 0
    // realizes to exactly that concrete type. A non-generic interface with no
    // associated types (`Equals`/`Compare`) yields just the `Self` slot.
    let interface_frame = |iface_tn: &baml_type::TypeName,
                           for_ty_pattern: &bex_vm_types::TyTemplate,
                           interface_args: &[bex_vm_types::TyTemplate],
                           interface_assoc: &[(Name, bex_vm_types::TyTemplate)]|
     -> Vec<bex_vm_types::TyTemplate> {
        let mut frame: Vec<bex_vm_types::TyTemplate> = Vec::with_capacity(1 + interface_args.len());
        frame.push(for_ty_pattern.clone());
        frame.extend(interface_args.iter().cloned());
        if let Some((_, _, decls)) = iface_assoc_decls.get(iface_tn) {
            for (name, _) in decls {
                // One slot per *declared* associated type, in order — so the frame
                // width is always `1 (Self) + interface_args + assoc_count` and the
                // method-level type args (appended after this frame at the call
                // site) land at the De Bruijn indices the callee expects. The
                // rule's bindings are complete (pinned or baked from the declared
                // default), so every slot carries a real binding; a member absent
                // here is a diagnosed incomplete impl (no pin, no default), kept
                // at the top type for error recovery.
                let slot = interface_assoc
                    .iter()
                    .find(|(an, _)| an == name)
                    .map(|(_, t)| t.clone())
                    .unwrap_or_else(|| {
                        bex_vm_types::TyTemplate::from(baml_type::RealizedTy::Unknown {
                            attr: TyAttr::default(),
                        })
                    });
                frame.push(slot);
            }
        }
        frame
    };
    // Complete a rule's associated bindings: every declared member the impl
    // leaves unpinned is baked from the interface's declared default,
    // substituted at this impl — `Self` := the for-type, the interface's
    // params := the target's arguments — so the registry answers every
    // declared member identically, pinned or defaulted. A Self-referencing
    // default (`type Items = Self.Item[]`) keeps its projections symbolic in
    // the baked template; the runtime reduces them back through this same
    // rule at realization time (fuel-bounded against cycles). A member with
    // neither pin nor default is a diagnosed incomplete impl and stays absent.
    let complete_interface_assoc = |interface_assoc: &mut Vec<(Name, bex_vm_types::TyTemplate)>,
                                    iface_tn: &baml_type::TypeName,
                                    iface_arg_tys: &[ty::Ty],
                                    for_ty: &ty::Ty,
                                    generics: &[ParamTy],
                                    resolved: &ResolvedAliases| {
        let Some((self_param, params, decls)) = iface_assoc_decls.get(iface_tn) else {
            return;
        };
        for (name, default) in decls {
            if interface_assoc.iter().any(|(an, _)| an == name) {
                continue;
            }
            let Some(default) = default else {
                continue;
            };
            let mut bindings: rustc_hash::FxHashMap<ParamTy, ty::Ty> =
                rustc_hash::FxHashMap::default();
            bindings.insert(self_param.clone(), for_ty.clone());
            for (param, arg) in params.iter().zip(iface_arg_tys) {
                bindings.insert(param.clone(), arg.clone());
            }
            let completed = baml_type::unify::substitute_ty(default, &bindings);
            interface_assoc.push((
                name.clone(),
                bex_vm_types::anchor_template(&baml_compiler2_mir::tir2_to_template(
                    &completed, resolved, generics,
                )),
            ));
        }
    };
    // Fill a rule's method table with the interface's defaults (override winning),
    // each carrying the interface frame it is invoked with.
    let merge_defaults = |methods: &mut indexmap::IndexMap<Name, ProgramMethodImpl>,
                          iface_tn: &baml_type::TypeName,
                          interface_frame: &[bex_vm_types::TyTemplate]| {
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
    // The interface objects themselves were pooled before their default bodies
    // were, so they still carry `default: None`; hand back what to fill in now
    // that every function has an index. Only interfaces pooled *by this emit*
    // are addressed — a mounted artifact's interfaces already carry theirs.
    let interface_default_backfill: Vec<InterfaceDefaultBackfill> = iface_defaults
        .iter()
        .filter_map(|(iface_tn, defaults)| {
            let iface_idx = *interface_indices.get(iface_tn)?;
            Some(defaults.iter().filter_map(move |(name, fqn)| {
                Some(InterfaceDefaultBackfill {
                    interface: ObjectIndex::from_raw(iface_idx),
                    method: name.clone(),
                    default: resolve_fqn(fqn)?,
                })
            }))
        })
        .flatten()
        .collect();

    for file in all_files {
        let pkg_info = file_package(db, *file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let _pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        let resolved = &alias_caches[&pkg_info.package];
        // Lower a type ref (in the owner's `TypeRefStore`) in this file's
        // namespace, discarding diagnostics (these targets were already validated
        // upstream). `bounds` carries the enclosing impl's/class's generic-param
        // bounds so a bound-typevar projection in a binding value
        // (`type SortError = T.CompareError`) determines its interface instead of
        // erasing.
        let lower = |store: &TypeRefStore,
                     id: TypeRefId,
                     generics: &[ParamTy],
                     bounds: &BoundsMap|
         -> ty::Ty {
            baml_compiler2_hir_ty::lower::reject_holes(
                &baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, *file)
                    .with_frame(generics.to_vec())
                    .with_bounds(bounds.clone())
                    .lower_type_ref(store, id),
            )
        };
        // [`lower`] for a constraint head — a generic bound or an `implements`
        // target pins only the associated members it writes (unwritten members
        // bake their declared defaults into the rule; a pinning impl can still
        // discharge a bare bound at runtime).
        let lower_constraint_head = |store: &TypeRefStore,
                                     id: TypeRefId,
                                     generics: &[ParamTy],
                                     bounds: &BoundsMap|
         -> ty::Ty {
            baml_compiler2_hir_ty::lower::reject_holes(
                &baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, *file)
                    .with_frame(generics.to_vec())
                    .with_bounds(bounds.clone())
                    .lower_type_ref_at(
                        store,
                        id,
                        baml_compiler2_hir_ty::lower::TypePosition::ConstraintHead,
                    ),
            )
        };
        // Each generic param's interface bound set (`T extends A & B` → {A, B}).
        // A bound is an interface, possibly generic or carrying associated
        // bindings — `split_interface` captures its args/assoc as templates over
        // the impl's params. A non-interface bound, rejected upstream, has no
        // interface to record, so skip the whole rule (`None`); dropping a rule
        // only ever loses a dispatch, never adds a wrong one. Every conjunct is
        // emitted: a rule narrowed by two bounds must stay narrowed by both.
        let bound_sets = |store: &TypeRefStore,
                          declared: &[GenericParamData],
                          generics: &[ParamTy],
                          bounds: &BoundsMap|
         -> Option<Vec<Vec<InterfaceBound>>> {
            declared
                .iter()
                .map(|param| {
                    param
                        .bounds
                        .iter()
                        .map(|&id| {
                            let bound_ty = lower_constraint_head(store, id, generics, bounds);
                            split_interface(&bound_ty, resolved, generics).map(
                                |(interface, args, assoc)| InterfaceBound {
                                    interface: bex_vm_types::TypeHead::of_name(&interface),
                                    args,
                                    assoc,
                                },
                            )
                        })
                        .collect()
                })
                .collect()
        };
        // Associated-type bindings written in an `implements` block body
        // (`type Item = int`) live beside the target, not in it (`split_interface`
        // only sees the target), so lower them here to fold into the implemented
        // interface's bindings.
        let lower_assoc = |store: &TypeRefStore,
                           bindings: &[AssociatedTypeBindingData],
                           generics: &[ParamTy],
                           bounds: &BoundsMap|
         -> Vec<(Name, bex_vm_types::TyTemplate)> {
            bindings
                .iter()
                .filter_map(|b| {
                    let id = b.type_ref?;
                    Some((
                        b.name.clone(),
                        bex_vm_types::anchor_template(&baml_compiler2_mir::tir2_to_template(
                            &lower(store, id, generics, bounds),
                            resolved,
                            generics,
                        )),
                    ))
                })
                .collect()
        };

        // (a) Out-of-body `implement<G> I for FOR { ... }`: primitives,
        // containers, generic classes, and blanket `for T`. (A non-generic
        // concrete class's out-of-body impl folds onto the class — see (b).)
        for &impl_loc in file_free_impls(db, *file) {
            let block = impl_block_data(db, impl_loc);
            let ImplSubjectData::Free {
                for_target,
                generics,
            } = &block.subject
            else {
                continue;
            };
            let store = &block.type_refs;
            let impl_params = baml_compiler2_hir_ty::lower::impl_frame(db, impl_loc);
            let impl_bounds = baml_compiler2_hir_ty::lower::impl_generic_bounds(db, impl_loc);
            // The target is a constraint, not an existential: it carries only
            // its written inline pins (block-level pins append below; unpinned
            // members bake their declared defaults).
            let iface_ty =
                lower_constraint_head(store, block.interface_target, &impl_params, &impl_bounds);
            let Some((iface_tn, interface_args, mut interface_assoc)) =
                split_interface(&iface_ty, resolved, &impl_params)
            else {
                continue;
            };
            interface_assoc.extend(lower_assoc(
                store,
                &block.associated_type_bindings,
                &impl_params,
                &impl_bounds,
            ));
            let for_ty = lower(store, *for_target, &impl_params, &impl_bounds);
            let iface_arg_tys = match &iface_ty {
                ty::Ty::Interface(_, args, _, _) => args.clone(),
                _ => unreachable!("split_interface matched an interface"),
            };
            complete_interface_assoc(
                &mut interface_assoc,
                &iface_tn,
                &iface_arg_tys,
                &for_ty,
                &impl_params,
                resolved,
            );
            let for_ty_pattern = bex_vm_types::anchor_template(
                &baml_compiler2_mir::tir2_to_template(&for_ty, resolved, &impl_params),
            );
            let Some(generic_param_bounds) =
                bound_sets(store, generics, &impl_params, &impl_bounds)
            else {
                continue;
            };
            // An impl's own method is compiled against the impl's own generics.
            let impl_frame: Vec<bex_vm_types::TyTemplate> = (0..u32::try_from(impl_params.len())
                .expect("generic arity fits u32"))
                .map(bex_vm_types::TyTemplate::TypeArgRef)
                .collect();
            let Some(interface_head) = interface_indices
                .get(&iface_tn)
                .copied()
                .map(ObjectIndex::from_raw)
            else {
                continue;
            };
            let mut methods = indexmap::IndexMap::new();
            for &m in &block.methods {
                let fqn = def_to_item_ref(db, Definition::Function(m)).to_string();
                let Some(fqn) = resolve_fqn(&fqn) else {
                    continue;
                };
                methods.insert(
                    function_data(db, m).name.clone(),
                    ProgramMethodImpl {
                        fqn,
                        frame: impl_frame.clone(),
                    },
                );
            }
            let iface_frame = interface_frame(
                &iface_tn,
                &for_ty_pattern,
                &interface_args,
                &interface_assoc,
            );
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
                    // An out-of-body impl of a field-bearing interface is E0126, so a
                    // rule built here never has fields to link — its `for` target need
                    // not even be a class.
                    field_links: {
                        debug_assert!(
                            !iface_field_decls.contains_key(&iface_tn),
                            "out-of-body impl of field-bearing interface `{iface_tn}` should be \
                             rejected by E0126",
                        );
                        Box::default()
                    },
                });
        }

        // (b) In-body `class C { implements I { ... } }` and folded non-generic
        // out-of-body `implement I for C` impls. Drive off the impl *blocks* so a
        // field-only (method-less) impl is still registered (membership matters
        // for reflection and bound checks even when there's nothing to dispatch);
        // attach any folded methods, grouped by their interface target.
        for &class_loc in file_classes(db, *file) {
            let class = class_data(db, class_loc);
            if class.implements.is_empty() {
                continue;
            }
            let store = &class.type_refs;
            let class_tn = qualify_def(db, Definition::Class(class_loc), &class.name);
            let generics = baml_compiler2_hir_ty::lower::class_generic_frame(db, class_loc);
            let class_bounds = baml_compiler2_hir_ty::lower::class_generic_bounds(db, class_loc);

            // Each folded method tagged with the full interface instantiation it
            // implements (name + args). A class may implement the same interface
            // at several instantiations (e.g. `Converter<int>` + `Converter<float>`),
            // each with its own methods; keying only by interface name would let
            // one block's method overwrite the other's, so methods are matched to
            // their block by the full instantiation below.
            let class_method_impls: Vec<(
                baml_type::TypeName,
                Vec<bex_vm_types::TyTemplate>,
                Name,
                String,
            )> = class
                .methods
                .iter()
                .filter_map(|&m| {
                    let target = method_interface_target(db, m).as_ref()?;
                    // A constraint head, like the block's own target below — the
                    // two lowerings must agree for the instantiation key match.
                    let (m_iface_tn, m_args, _m_assoc) = split_interface(
                        &lower_constraint_head(
                            &target.type_refs,
                            target.target,
                            &generics,
                            &class_bounds,
                        ),
                        resolved,
                        &generics,
                    )?;
                    Some((
                        m_iface_tn,
                        m_args,
                        function_data(db, m).name.clone(),
                        def_to_item_ref(db, Definition::Function(m)).to_string(),
                    ))
                })
                .collect();

            // The implementor pattern is the class at its own parameters; bounds
            // come from the class's generic parameters. Shared by all its blocks.
            let for_ty_pattern = if generics.is_empty() {
                bex_vm_types::TyTemplate::from(bex_vm_types::RealizedTy::Class(
                    bex_vm_types::TypeHead::of_name(&class_tn),
                    Box::new([]),
                    TyAttr::default(),
                ))
            } else {
                bex_vm_types::TyTemplate::Class(
                    bex_vm_types::TypeHead::of_name(&class_tn),
                    (0..u32::try_from(generics.len()).expect("generic arity fits u32"))
                        .map(bex_vm_types::TyTemplate::TypeArgRef)
                        .collect(),
                    TyAttr::default(),
                )
            };
            let Some(generic_param_bounds) =
                bound_sets(store, &class.generic_params, &generics, &class_bounds)
            else {
                continue;
            };
            // An impl block's own methods are compiled against the class's generics.
            let impl_frame: Vec<bex_vm_types::TyTemplate> = (0..u32::try_from(generics.len())
                .expect("generic arity fits u32"))
                .map(bex_vm_types::TyTemplate::TypeArgRef)
                .collect();

            // The receiver type `Self` denotes for this class's blocks, in
            // `Ty` space for default-binding completion (structural sugar for
            // the builtin containers, matching TIR's receiver typing).
            let class_receiver_ty = baml_compiler2_hir_ty::lower::class_self_ty(db, class_loc);
            for block in &class.implements {
                // Constraint position: written inline pins only (see the
                // free-impl site above).
                let iface_ty = lower_constraint_head(store, block.target, &generics, &class_bounds);
                let Some((iface_tn, interface_args, mut interface_assoc)) =
                    split_interface(&iface_ty, resolved, &generics)
                else {
                    continue;
                };
                interface_assoc.extend(lower_assoc(
                    store,
                    &block.associated_type_bindings,
                    &generics,
                    &class_bounds,
                ));
                let iface_arg_tys = match &iface_ty {
                    ty::Ty::Interface(_, args, _, _) => args.clone(),
                    _ => unreachable!("split_interface matched an interface"),
                };
                complete_interface_assoc(
                    &mut interface_assoc,
                    &iface_tn,
                    &iface_arg_tys,
                    &class_receiver_ty,
                    &generics,
                    resolved,
                );
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
                let iface_frame = interface_frame(
                    &iface_tn,
                    &for_ty_pattern,
                    &interface_args,
                    &interface_assoc,
                );
                merge_defaults(&mut methods, &iface_tn, &iface_frame);
                // The field table for this block, positional over the interface's own
                // declared fields. Each entry is the class slot the interface field
                // reads: the block's explicit `field as class_field` link, else the
                // same-named class field (the default that
                // `concrete_interface_field_sources` applies in TIR).
                //
                // A name that resolves to no class slot means the class does not cover
                // the interface field — already E0124, so this program has diagnostics
                // and cannot reach a runnable artifact. Drop the whole rule rather than
                // bake a partial table: losing a dispatch is recoverable, a table whose
                // positions no longer line up with the interface silently reads the
                // wrong field. Matches the `resolve_fqn` convention above.
                let field_links: Option<Box<[u32]>> = match iface_field_decls.get(&iface_tn) {
                    None => Some(Box::default()),
                    Some(declared) => {
                        let class_slots = class_field_indices.get(&class_tn.to_string());
                        declared
                            .iter()
                            .map(|iface_field| {
                                let class_field = block
                                    .field_links
                                    .iter()
                                    .find(|link| link.interface_field == *iface_field)
                                    .map_or(iface_field, |link| &link.class_field);
                                let slot = class_slots
                                    .and_then(|slots| slots.get(class_field.as_str()))
                                    .copied();
                                debug_assert!(
                                    slot.is_some(),
                                    "interface `{iface_tn}` field `{iface_field}` links to \
                                     `{class_tn}.{class_field}`, which has no runtime slot",
                                );
                                slot.map(|s| u32::try_from(s).expect("class field count fits u32"))
                            })
                            .collect()
                    }
                };
                let Some(field_links) = field_links else {
                    continue;
                };
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
                        field_links,
                    });
            }
        }
    }

    // Project the compiler's enriched export surface into the runtime package
    // record. A functions-only package has no type/impl pass to create its row,
    // so establish every source-backed package here before copying the table.
    for (package_name, exports) in package_exports {
        let package = program_packages.entry(package_name.clone()).or_default();
        package.interface_blob.clone_from(&exports.interface_blob);
        package.exported_names.clone_from(&exports.exported_names);
        package.functions.clear();
        for (local_name, callable_fqn) in &exports.functions {
            let Some(&index) = function_indices.get(callable_fqn) else {
                // Compiler intrinsics deliberately have no callable object.
                continue;
            };
            package
                .functions
                .insert(local_name.clone(), ObjectIndex::from_raw(index));
        }
        let test_init_name = if package_name.as_str() == "user" {
            "$init_test".to_string()
        } else {
            format!("{package_name}.$init_test")
        };
        package.test_init = function_indices
            .get(&test_init_name)
            .copied()
            .map(ObjectIndex::from_raw);
    }

    // Impl rules are keyed by their interface's object index (assigned in
    // deterministic emission order); within one interface a `for_ty_pattern` is
    // unique (overlap is a coherence error). The primary rule key is the rendered
    // pattern; its `Display` drops module paths, so two distinct same-short-name
    // for-types tie. `{:?}` carries the module-qualified identity and breaks the
    // tie. The interface instantiation (args + associated bindings) is folded in
    // last so the same for-type implementing one interface at several
    // instantiations (e.g. `Converter<int>` + `Converter<float>`) orders by
    // content rather than declaration order. Package-level ordering is finalized
    // by the caller once every map is built.
    for pkg in program_packages.values_mut() {
        // Impl rules are not a declaration-order surface; canonicalize them so
        // full and incremental compilation stay byte-identical.
        pkg.canonicalize_impl_rules();
    }
    interface_default_backfill
}

/// One `InterfaceMethodDef::default` slot `build_packages` could not fill when
/// the interface was pooled (functions are pooled later), keyed by the pooled
/// interface and the method's name.
struct InterfaceDefaultBackfill {
    interface: ObjectIndex,
    method: Name,
    default: ObjectIndex,
}

/// Write each back-filled default into its pooled `Object::Interface`.
fn apply_interface_default_backfill(program: &mut Program, backfill: &[InterfaceDefaultBackfill]) {
    for entry in backfill {
        let Object::Interface(iface) = &mut program.objects[entry.interface] else {
            unreachable!(
                "interface index {} is not an Object::Interface",
                entry.interface.raw()
            )
        };
        let method = iface
            .methods
            .iter_mut()
            .find(|method| method.name == entry.method)
            .unwrap_or_else(|| {
                unreachable!(
                    "interface `{}` declares no method `{}` for its default",
                    iface.name, entry.method
                )
            });
        method.default = Some(entry.default);
    }
}

pub(crate) use emit::compile_mir_function;

fn is_builtin_function_name(name: &str) -> bool {
    matches!(
        name.split('.').next(),
        Some(
            "baml"
                | "boundary"
                | "reflect"
                | "assert"
                | "testing"
                | "log"
                | "env"
                | "ai"
                | "openai"
                | "anthropic"
                | "google"
                | "aws"
                | "vercel"
                | "claude_code"
        )
    )
}

/// Is `name` a synthesized `$init` / `$init_test` chainer (per package)? These
/// are the functions that make up the whole-group `$init` tail (design §9 R2).
fn is_synth_init_name(name: &str) -> bool {
    name == "$init"
        || name == "$init_test"
        || name.ends_with(".$init")
        || name.ends_with(".$init_test")
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

/// Read-only snapshot of pooled class field metadata: every name registered in
/// A class's fields (name + type, in field order), keyed by the class's own
/// [`TypeTag`](baml_type::typetag::TypeTag) — the identity its `TypeHead`
/// carries, so a lookup is an integer compare and needs no name spelling.
///
/// Built once from the `Object::Class` entries before function bodies are
/// compiled, so codegen resolves field names/types without reading the object
/// pool (a hard requirement for parallel emit, whose workers compile against
/// fragment pools that don't contain the pre-existing objects).
pub(crate) type ClassFieldSnapshot =
    HashMap<baml_type::typetag::TypeTag, Vec<(String, bex_vm_types::RuntimeTy)>>;

/// Context for MIR codegen.
pub(crate) struct MirCodegenContext<'ctx, 'obj> {
    pub globals: &'ctx HashMap<String, usize>,
    pub classes: &'ctx HashMap<String, HashMap<String, usize>>,
    pub class_object_indices: &'ctx HashMap<String, usize>,
    pub enum_object_indices: &'ctx HashMap<String, usize>,
    pub enum_variants: &'ctx HashMap<String, HashMap<String, usize>>,
    pub class_fields: &'ctx ClassFieldSnapshot,
    pub objects: &'obj mut ObjectPool,
    /// Program-absolute index of `objects[0]`: 0 when `objects` is the whole
    /// program pool (serial emit), the Stage-B watermark when it is a
    /// worker-local fragment pool (parallel emit).
    pub objects_base: usize,
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
pub trait Db: baml_compiler2_mir::Db {
    /// Mint an owned database handle that shares this database's storage, for
    /// MOVING into a worker thread.
    ///
    /// Parallel MIR lowering (Stage A of `emit_functions_parallel`) clones one
    /// handle per work chunk on the calling thread — the database type is
    /// expected to be `Send` but not `Sync`, so workers can never share `&db`.
    /// All clones share one salsa memo table (the rust-analyzer concurrency
    /// model), so a query computed by one worker is a cache hit for the rest.
    ///
    /// The default returns `None`, which keeps every salsa read on the calling
    /// thread (Stage A stays serial). `ProjectDatabase` overrides this with
    /// `Clone` (an `Arc` bump).
    fn parallel_db_handle(&self) -> Option<Box<dyn baml_compiler2_mir::Db + Send>> {
        None
    }
}

/// Compile options.
pub struct CompileOptions {
    pub emit_test_cases: bool,
}

/// Errors that can occur during bytecode generation.
#[derive(Debug)]
pub enum LoweringError {
    /// An internal invariant was violated during lowering/decomposition/linking
    /// (a compiler bug), carrying a diagnostic message.
    Internal(String),
    /// The incremental (dirty-only) reuse path cannot reuse the cached image for
    /// this project. Raised in two cases: a caller-clean file is missing from
    /// `prev_units` (a corrupt / stale cached image); or a dirty top-level `let`
    /// initializer interns a generic-function value already owned by a clean file
    /// (the design §9 R1 tail edge). Not a compiler fault: callers silently fall
    /// back to a full compile, which is byte-identical.
    ReuseUnsupported(String),
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
            Self::ReuseUnsupported(msg) => {
                write!(f, "per-file bytecode reuse unsupported: {msg}")
            }
            Self::ProjectHasErrors { error_count } => write!(
                f,
                "cannot generate bytecode: project has {error_count} unresolved compile error(s)"
            ),
        }
    }
}

impl std::error::Error for LoweringError {}

#[derive(Debug)]
pub enum MountedPackageLinkError {
    DependencyLink(bex_vm_types::link::LinkError),
    Consumer(LoweringError),
}

impl std::fmt::Display for MountedPackageLinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DependencyLink(error) => write!(f, "link mounted dependency units: {error}"),
            Self::Consumer(error) => write!(f, "compile mounted-package consumer: {error}"),
        }
    }
}

impl std::error::Error for MountedPackageLinkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DependencyLink(error) => Some(error),
            Self::Consumer(error) => Some(error),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SchemaAttrs {
    description: Option<String>,
    alias: Option<String>,
    docstring: Option<String>,
    other: indexmap::IndexMap<String, String>,
    skip: bool,
}

/// Extract schema metadata from span-free HIR attributes and docstrings.
fn extract_schema_attrs(
    attrs: &[baml_compiler2_hir::item_tree::Attribute],
    docstring: Option<&str>,
) -> SchemaAttrs {
    let mut result = SchemaAttrs {
        docstring: docstring.map(str::to_owned),
        ..SchemaAttrs::default()
    };
    for attr in attrs {
        match attr.name.as_str() {
            "description" | "alias" if attr.args.len() == 1 => {
                let raw = attr.args[0].value.as_str();
                let value = parse_string_attr_value(raw);
                if attr.name.as_str() == "description" {
                    result.description = value;
                } else {
                    result.alias = value;
                }
            }
            "description" | "alias" => {}
            "skip" => {
                result.skip = true;
            }
            _ => {
                let value = match attr.args.as_slice() {
                    [] => "true".to_string(),
                    [arg] if arg.key.is_none() => {
                        parse_string_attr_value(&arg.value).unwrap_or_else(|| arg.value.clone())
                    }
                    args => args
                        .iter()
                        .map(|arg| {
                            let value = parse_string_attr_value(&arg.value)
                                .unwrap_or_else(|| arg.value.clone());
                            arg.key
                                .as_ref()
                                .map_or(value.clone(), |key| format!("{}={value}", key.as_str()))
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                };
                result.other.insert(attr.name.to_string(), value);
            }
        }
    }
    result
}

pub use bex_vm_types::Program as ProgramAlias;

/// One entry in the emitted runtime field list for a class. The field type is a
/// `TypeRefId` into the owning class's `TypeRefStore` (carried alongside).
type MergedFieldEntry = (
    String,
    baml_compiler2_hir::type_ref::TypeRefId,
    Vec<baml_compiler2_hir::item_tree::Attribute>,
    Option<String>,
    Vec<Name>,
    Vec<Name>,
);

/// BEP-044: collect actual runtime fields. Interface fields are views over
/// class fields, so they never add qualified runtime slots.
fn collect_class_fields_with_implements(
    pkg_ns: &[Name],
    class: &baml_compiler2_ppir::item_data::ClassData,
) -> Vec<MergedFieldEntry> {
    let mut out: Vec<MergedFieldEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for field in &class.fields {
        let name = field.name.to_string();
        if !seen.insert(name.clone()) {
            continue;
        }
        out.push((
            name,
            field.type_ref,
            field.attributes.clone(),
            field.docstring.clone(),
            class
                .generic_params
                .iter()
                .map(|param| param.name.clone())
                .collect(),
            pkg_ns.to_vec(),
        ));
    }

    out
}

/// Build a `TypeName` from a fully-qualified dotted path.
///
/// Emit always fully qualifies — `display_name` keeps the literal package
/// prefix (`"user.Point"`, `"baml.http.Response"`, `"<vendor>.<…>"`). The
/// Mint the head identity for the declaration named `fq_name`, rejecting a
/// hash collision with any head already claimed.
///
/// Every declaration kind that can be the head of a nominal type — class, enum,
/// interface — draws from one space, so the detector is shared rather than
/// per-kind: fully-qualified names are unique *across* kinds, so a collision
/// between a class and an enum is exactly as impossible-by-intent, and exactly
/// as fatal, as one between two classes.
fn claim_type_tag(
    claimed: &mut HashMap<baml_type::typetag::TypeTag, String>,
    fq_name: &str,
) -> Result<baml_type::typetag::TypeTag, LoweringError> {
    let tag = baml_type::typetag::TypeTag::of_head(fq_name);
    if let Some(previous) = claimed.insert(tag, fq_name.to_string())
        && previous != fq_name
    {
        return Err(LoweringError::Internal(format!(
            "the fully-qualified type names `{previous}` and `{fq_name}` hash to \
             the same 47-bit type tag. This is an extremely rare hash collision \
             between two names, not a compiler bug; renaming either declaration \
             (or moving one to a different namespace/package) resolves it. This \
             is a known limitation of content-addressed type tags."
        )));
    }
    Ok(tag)
}

/// codegen-output Python and the runtime see the same `<pkg>.<…>` form
/// end-to-end. See `12a-namespace-rules.md §5` for the rationale.
fn fq_to_type_name(fq: &str) -> baml_type::TypeName {
    baml_type::QualifiedTypeName::from_dotted_path(fq)
}

/// Generate bytecode for the entire project (default: `OptLevel::Two`).
pub fn generate_project_bytecode(
    db: &dyn crate::Db,
    options: &CompileOptions,
) -> Result<Program, LoweringError> {
    generate_project_bytecode_with_opt(db, options, OptLevel::Two)
}

/// Generate bytecode for the entire project with a specific optimization level.
pub fn generate_project_bytecode_with_opt(
    db: &dyn crate::Db,
    options: &CompileOptions,
    opt: OptLevel,
) -> Result<Program, LoweringError> {
    let mut program = generate_impl(db, options, opt, None, false, None)?;
    program.source_content_hash = Some(project_source_content_hash(db));
    Ok(program)
}

/// The conservative source-content identity of this compile's project file
/// set (profiling streams spec §2.3): any byte change in any project file, or
/// a compiler version change, yields a new hash. Stdlib stubs are excluded —
/// they are a compiler-build constant already covered by the version input.
pub fn project_source_content_hash(db: &dyn crate::Db) -> [u8; 32] {
    // The `<builtin>/` path prefix is the wire-contract spelling of "stdlib
    // stub" (and of runtime mount stubs), the same rule `builtin_count`
    // keys on — filtering by root KIND here would diverge for databases
    // that hold both source stdlib and mount-stub roots.
    let files: Vec<(String, String)> = compiler2_all_files(db)
        .iter()
        .filter(|file| !file.path(db).to_string_lossy().starts_with("<builtin>/"))
        .map(|file| {
            (
                file.path(db).to_string_lossy().into_owned(),
                file.text(db).clone(),
            )
        })
        .collect();
    bex_vm_types::identity::program_content_hash(
        files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_bytes())),
    )
}

/// Compile ONLY the builtin stdlib into a standalone `Program` slice.
///
/// Because builtins occupy a contiguous, user-independent prefix of every
/// index space (see `emit_file_group`), this Program is byte-identical to the
/// stdlib prefix of any full project compile at the same `opt` — regardless
/// of what user code the `db` holds. It is the cacheable artifact (keyed by
/// compiler build + opt level) that `generate_project_bytecode_with_stdlib`
/// splices into project compiles.
pub fn generate_stdlib_program(
    db: &dyn crate::Db,
    opt: OptLevel,
) -> Result<Program, LoweringError> {
    generate_impl(
        db,
        &CompileOptions {
            emit_test_cases: false,
        },
        opt,
        None,
        true,
        None,
    )
}

/// Generate project bytecode on top of a precompiled stdlib `Program` slice
/// (from [`generate_stdlib_program`], same compiler build and `opt`).
///
/// Skips all builtin-group lowering — the dominant fixed cost of a compile —
/// by seeding the output program and the emit tables from `base`, then
/// emitting only the user file group on top. The result is byte-identical to
/// a full [`generate_project_bytecode_with_opt`] run (asserted by the
/// `emit_determinism` integration tests).
pub fn generate_project_bytecode_with_stdlib(
    db: &dyn crate::Db,
    options: &CompileOptions,
    opt: OptLevel,
    base: &Program,
) -> Result<Program, LoweringError> {
    let mut program = generate_impl(db, options, opt, Some(base), false, None)?;
    program.source_content_hash = Some(project_source_content_hash(db));
    Ok(program)
}

/// Compile and link a source consumer against independently emitted mounted
/// dependency units. The database's package-interface blobs provide semantic
/// resolution; `dependency_units` provide the matching runtime symbols.
pub fn generate_project_bytecode_with_mounted_units(
    db: &dyn crate::Db,
    options: &CompileOptions,
    opt: OptLevel,
    dependency_units: &[CompilationUnit],
) -> Result<Program, MountedPackageLinkError> {
    let base = bex_vm_types::link::link(dependency_units)
        .map_err(MountedPackageLinkError::DependencyLink)?;
    generate_impl(db, options, opt, Some(&base), false, None)
        .map_err(MountedPackageLinkError::Consumer)
}

/// Incremental compile that lowers function bodies only for dirty files, reuses
/// clean files' symbolic units from the cached image, and links.
///
/// Declaration/layout passes still walk the project because dirty bytecode must
/// use the same whole-program indices as a full compile. Pass 4 skips every clean
/// file (`take_lowered_files` reports only dirty paths); decomposition's temporary
/// clean units are discarded in favor of `prev_units`. Whole-program products are
/// recomputed (package fragments freshly decomposed; the `$init`/`$init_test` tail
/// **freshly synthesized** from every file's `let`s / `test` blocks — design §9
/// R2 — whose symbolic imports the linker re-resolves against the shifted
/// layout).
///
/// `clean_files` is the caller's optimistic clean set; a file is only truly
/// reused when its inferred transitive `throws` still match the previous compile
/// (design §4 — the throws gate). `prev_units` must come from the same compiler
/// build / options / stdlib base.
///
/// # Errors
///
/// Returns [`LoweringError::Internal`] if `prev_units` fail to link (a corrupt /
/// incompatible previous units) and propagates any [`LoweringError`] from the
/// dirty-file emit.
pub fn generate_project_bytecode_with_reuse_units(
    db: &dyn crate::Db,
    options: &CompileOptions,
    opt: OptLevel,
    base: &Program,
    prev_units: &[CompilationUnit],
    clean_files: &HashSet<String>,
) -> Result<Program, LoweringError> {
    generate_project_bytecode_with_reuse_artifacts(db, options, opt, base, prev_units, clean_files)
        .map(|(program, _)| program)
}

/// Reuse compile variant that also returns the already-assembled symbolic
/// units. Cache-aware callers can persist these directly instead of decomposing
/// the linked program a second time.
pub fn generate_project_bytecode_with_reuse_artifacts(
    db: &dyn crate::Db,
    options: &CompileOptions,
    opt: OptLevel,
    base: &Program,
    prev_units: &[CompilationUnit],
    clean_files: &HashSet<String>,
) -> Result<(Program, Vec<CompilationUnit>), LoweringError> {
    let mismatches = reuse_throws_mismatches(db, prev_units, clean_files);
    let effective_clean;
    let clean_files = if mismatches.is_empty() {
        clean_files
    } else {
        effective_clean = clean_files
            .iter()
            .filter(|path| !mismatches.contains_key(path.as_str()))
            .cloned()
            .collect();
        &effective_clean
    };

    // Direct per-file emit: lower ONLY the dirty files (clean files are skipped in
    // Pass 4), producing a partial program whose dirty content decomposes into
    // fresh units. The partial DOES synthesize the whole-project `$init`/
    // `$init_test` tail (design §9 R2): it is rebuilt from every file's `let`s /
    // `test` blocks (clean `let` initializers re-lowered off salsa-cached MIR),
    // so a dirty tail-producing file no longer aborts reuse.
    let partial = generate_impl(db, options, opt, Some(base), false, Some(clean_files))?;

    let mut fresh_units = decompose_units(db, options, &partial)?;

    // The freshly-synthesized (symbolic) tail: whichever fresh unit the
    // decomposition placed it on. It reflects the *current* project's lets/tests
    // (clean + dirty), not the previous compile's, so a changed dirty tail is
    // captured. Its object/global imports are names, so the linker re-resolves
    // them against this compile's shifted layout.
    let fresh_tail = fresh_units.iter_mut().find_map(|u| u.init_tail.take());

    // R1 tail edge (design §9): a dirty top-level `let` initializer can intern a
    // generic-function VALUE into the freshly-synthesized tail. The linker dedups
    // generic values across `code` buckets, but a tail-local copy that duplicates
    // a *clean* file's code-owned copy is not covered — it would place both and
    // break byte-identity. This is rare (a generic value as a top-level `let`);
    // detect it precisely and fall back to a full compile for that case only.
    if !clean_files.is_empty()
        && let Some(tail) = &fresh_tail
        && tail_generic_dupes_clean(tail, prev_units, clean_files)
    {
        return Err(LoweringError::ReuseUnsupported(
            "a dirty top-level `let` initializer interns a generic-function value \
             already owned by a clean file (design §9 R1 tail edge)"
                .to_string(),
        ));
    }

    // Assemble: clean files verbatim from `prev_units`, dirty files fresh. The
    // per-package fragment is always recomputed (it reflects every file in the
    // package), so a clean carrier unit never carries a stale fragment. The tail
    // is placed once, below.
    let prev_by_source: HashMap<&str, &CompilationUnit> = prev_units
        .iter()
        .map(|u| (u.source_file.as_str(), u))
        .collect();
    let mut assembled: Vec<CompilationUnit> = Vec::with_capacity(fresh_units.len());
    for fresh in &mut fresh_units {
        let mut unit = if clean_files.contains(&fresh.source_file) {
            let prev = prev_by_source
                .get(fresh.source_file.as_str())
                .ok_or_else(|| {
                    LoweringError::ReuseUnsupported(format!(
                        "clean file `{}` missing from previous units",
                        fresh.source_file
                    ))
                })?;
            let mut unit = (*prev).clone();
            unit.package_fragment = std::mem::take(&mut fresh.package_fragment);
            unit
        } else {
            std::mem::take(fresh)
        };
        // The tail is a single whole-group product placed on one carrier below;
        // clear any per-unit copy first (a clean carrier would otherwise carry a
        // stale tail).
        unit.init_tail = None;
        assembled.push(unit);
    }

    // Place the freshly-synthesized tail on the last user unit (the linker only
    // requires it to be on *some* unit of the user group).
    if let Some(tail) = fresh_tail
        && let Some(carrier) = assembled
            .iter_mut()
            .rev()
            .find(|u| !u.source_file.starts_with("<builtin>/"))
    {
        carrier.init_tail = Some(tail);
    }

    let program = bex_vm_types::link::link(&assembled)
        .map_err(|e| LoweringError::Internal(format!("link reused units: {e}")))?;
    let mut program = program;
    program.source_content_hash = Some(project_source_content_hash(db));
    Ok((program, assembled))
}

/// Find clean files whose inferred-throws invariant no longer matches the
/// previous units. Callers demote these files before serving diagnostics or
/// splicing units. Previous metadata is read directly from the units, avoiding
/// a full link solely for this comparison.
pub fn reuse_throws_mismatches(
    db: &dyn baml_compiler2_mir::Db,
    prev_units: &[CompilationUnit],
    clean_files: &HashSet<String>,
) -> HashMap<String, String> {
    let previous: HashMap<&str, &bex_vm_types::TyTemplate> = prev_units
        .iter()
        .flat_map(|unit| &unit.code)
        .filter_map(|object| match object {
            Object::Function(function) => Some((function.name.as_str(), &function.throws_type)),
            _ => None,
        })
        .collect();
    let all_files = compiler2_all_files(db);
    let alias_caches = build_alias_caches(db, &all_files);
    let mut mismatches = HashMap::new();

    for file in all_files {
        let rel = relative_source_path(db, file);
        if !clean_files.contains(&rel) {
            continue;
        }
        let pkg = file_package(db, file);
        if let Err(detail) = spliced_throws_match(db, file, &previous, &alias_caches[&pkg.package])
        {
            mismatches.insert(rel, detail);
        }
    }
    mismatches
}

/// B-693 Stage 2: emit every source file as a relocatable [`CompilationUnit`].
///
/// Runs the ordinary full compile ([`generate_project_bytecode_with_opt`]) and
/// then *decomposes* the flat `Program` into per-file symbolic units: each
/// unit's own compiled objects are bucketed by emit pass (classes / enums /
/// interfaces / code), every cross-file `ObjectIndex`/`GlobalIndex` reference is
/// rewritten to the per-unit local/import convention (§2a) captured in the
/// unit's import table, and the whole-program `packages` map is split into
/// per-package fragments.
///
/// The invariant Stage 2 gates is
/// `borsh(link(emit_units(p))) == borsh(generate_project_bytecode(p))`: because
/// the object *content* comes straight from the real compile, only the index
/// remapping and the linker's reassembly order can diverge.
///
/// # Errors
///
/// Propagates any [`LoweringError`] from the underlying full compile, and
/// returns [`LoweringError::Internal`] if the flat program contains a construct
/// the Stage 2 decomposition does not yet handle (a `$init`/generic-function
/// tail — see design §9 R1/R2 — or an unattributable pool object).
pub fn emit_units(
    db: &dyn crate::Db,
    options: &CompileOptions,
    opt: OptLevel,
) -> Result<Vec<CompilationUnit>, LoweringError> {
    let program = generate_project_bytecode_with_opt(db, options, opt)?;
    decompose_units(db, options, &program)
}

/// Emit relocatable source units on top of a compiler-built stdlib prefix.
///
/// The prefix is used for semantic/runtime symbol indices during lowering but
/// is not decomposed back into units: every reference from a returned source
/// unit to the prefix becomes a normal symbolic import. Linking that unit into
/// the host image therefore resolves stdlib symbols to the host's immutable
/// objects and impl rules instead of copying them into a runtime package.
pub fn emit_units_with_stdlib(
    db: &dyn crate::Db,
    options: &CompileOptions,
    opt: OptLevel,
    stdlib: &Program,
) -> Result<Vec<CompilationUnit>, LoweringError> {
    let program = generate_project_bytecode_with_stdlib(db, options, opt, stdlib)?;
    decompose_units_after_prefix(db, options, &program, stdlib.objects.len())
}

/// Per-object attribution kind, computed during the pool walk.
enum PoolObjKind {
    Class,
    Enum,
    Interface,
    TypeAlias,
    /// A named function (fully-qualified name interned in `function_indices`).
    NamedFn(String),
    /// A lambda / interned literal — attributed to a file by proximity, not name.
    CodeAnon,
}

/// Decompose an already-compiled `Program` into per-file symbolic
/// [`CompilationUnit`]s — the inverse of [`link`](bex_vm_types::link::link).
///
/// Used by the CLI to persist content-addressed units after a compile so the
/// next incremental compile can reuse clean files independently.
/// `program` must be the output of a compile over `db` with `options` (a full
/// compile, a stdlib splice, or a reuse relink — all byte-identical), so the
/// decomposition's file-attribution invariants hold.
///
/// # Errors
///
/// [`LoweringError::Internal`] if the program holds a pool object the
/// decomposition cannot attribute to a source file.
#[allow(clippy::too_many_lines)]
pub fn decompose_units(
    db: &dyn baml_compiler2_mir::Db,
    options: &CompileOptions,
    program: &Program,
) -> Result<Vec<CompilationUnit>, LoweringError> {
    decompose_units_after_prefix(db, options, program, 0)
}

#[allow(clippy::too_many_lines)]
fn decompose_units_after_prefix(
    db: &dyn baml_compiler2_mir::Db,
    options: &CompileOptions,
    program: &Program,
    prefix_objects: usize,
) -> Result<Vec<CompilationUnit>, LoweringError> {
    let all_files = compiler2_all_files(db);
    let n_files = all_files.len();

    // ---- Per-file identity maps ---------------------------------------------
    let mut unit_source: Vec<String> = Vec::with_capacity(n_files);
    let mut unit_package: Vec<Name> = Vec::with_capacity(n_files);
    let mut rel_to_file: HashMap<String, usize> = HashMap::new();
    for (fi, file) in all_files.iter().enumerate() {
        let rel = relative_source_path(db, *file);
        rel_to_file.insert(rel.clone(), fi);
        unit_source.push(rel);
        unit_package.push(file_package(db, *file).package);
    }

    // Ordered owners for the pass-major definition buckets: the k-th class /
    // enum / interface object in the pool belongs to the file that defines it,
    // in the exact iteration order Passes 2/3/3b use.
    let mut class_owner: Vec<usize> = Vec::new();
    let mut enum_owner: Vec<usize> = Vec::new();
    let mut iface_owner: Vec<usize> = Vec::new();
    // Only *recursive* aliases become pool objects, and which ones those are is a
    // whole-package property (see Pass 3c) — so aliases are attributed by name
    // rather than positionally, the way named functions and lets are.
    let mut alias_name_to_file: HashMap<baml_type::TypeName, usize> = HashMap::new();
    // fq name -> file for named functions / lets (matches `Function::name`).
    let mut func_name_to_file: HashMap<String, usize> = HashMap::new();
    let mut let_name_to_file: HashMap<String, usize> = HashMap::new();
    for (fi, file) in all_files.iter().enumerate() {
        for &alias_loc in baml_compiler2_ppir::item_data::file_type_aliases(db, *file) {
            let alias_data = baml_compiler2_ppir::item_data::type_alias_data(db, alias_loc);
            let qtn = baml_compiler2_hir_ty::lower::qualify_def(
                db,
                Definition::TypeAlias(alias_loc),
                &alias_data.name,
            );
            alias_name_to_file.entry(qtn).or_insert(fi);
        }
        // Owner vectors are consumed by walking the object pool in emission order,
        // so they must be built in the SAME order the `Object::Class`/`Enum`/
        // `Interface` entries are emitted — the firewall enumeration (source order),
        // matching Passes 2/3/3b.
        for _ in file_classes(db, *file) {
            class_owner.push(fi);
        }
        for _ in file_enums(db, *file) {
            enum_owner.push(fi);
        }
        for _ in file_interfaces(db, *file) {
            iface_owner.push(fi);
        }
        for &func_loc in file_functions(db, *file) {
            // Required interface methods are signature-only items: nothing
            // to compile or index (mirrors their pre-item invisibility here).
            if baml_compiler2_ppir::item_data::is_required_interface_method(db, func_loc) {
                continue;
            }
            let fq = def_to_item_ref(db, Definition::Function(func_loc)).to_string();
            func_name_to_file.insert(fq, fi);
        }
        for &let_loc in file_lets(db, *file) {
            let fq = def_to_item_ref(db, Definition::Let(let_loc)).to_string();
            let_name_to_file.insert(fq, fi);
        }
    }

    // obj idx -> fq name for named functions (reverse of `function_indices`).
    let mut fn_obj_name: HashMap<usize, String> = HashMap::new();
    for (name, &idx) in &program.function_indices {
        fn_obj_name.insert(idx, name.clone());
    }

    let n_obj = program.objects.len();

    // ---- Locate the `$init`/`$init_test` tail (design §9 R2) ----------------
    // Passes 4.5/4.6 append the tail after all of Pass 4's regular functions, so
    // the tail is the contiguous suffix after the last *regular* named function
    // (a function in `func_name_to_file`; the synthesized `$init`/`$init_test`
    // chainers and `$init_let_*` helpers are not). Only the user group carries a
    // tail today (builtins define no top-level `let`s), so the tail is a suffix
    // of the whole pool.
    // A tail exists only if the program actually synthesized `$init`/`$init_test`
    // functions (a project with top-level `let`s or `test` blocks). Without that
    // guard, a program whose last pool objects are trailing class/enum/interface
    // defs (e.g. a dirty-only Stage 6 emit with no user functions) would be
    // mis-read as having a tail.
    let has_init_tail = program
        .function_indices
        .keys()
        .any(|n| is_synth_init_name(n));
    let mut tail_start = n_obj;
    if has_init_tail {
        let mut last_regular = None;
        for idx in 0..n_obj {
            if let Object::Function(_) = &program.objects[ObjectIndex::from_raw(idx)]
                && let Some(name) = fn_obj_name.get(&idx)
                && func_name_to_file.contains_key(name)
            {
                last_regular = Some(idx);
            }
        }
        if let Some(l) = last_regular {
            tail_start = l + 1;
        } else {
            // No regular functions at all (e.g. a dirty-only emit whose sole
            // tail-producing file declares only a top-level `let`): the tail
            // begins right after the class/enum/interface definition prefix.
            // Count only the dirty region's alias objects: the prefix's
            // (builtins' and prior submissions' recursive aliases) are already
            // inside `prefix_objects`, and double-counting them pushes
            // `tail_start` past the real `$init` tail.
            let n_aliases = program
                .objects
                .iter()
                .skip(prefix_objects)
                .filter(|o| matches!(o, Object::TypeAlias(_)))
                .count();
            tail_start = prefix_objects
                + class_owner.len()
                + enum_owner.len()
                + iface_owner.len()
                + n_aliases;
        }
    }

    // ---- Attribute every regular pool object to a file ----------------------
    let mut obj_owner: Vec<usize> = vec![usize::MAX; n_obj];
    let mut obj_kind: Vec<PoolObjKind> = Vec::with_capacity(tail_start - prefix_objects);
    let (mut ci, mut ei, mut ii) = (0usize, 0usize, 0usize);
    // The index drives three sequences (pool read, `obj_owner` write, `obj_kind`
    // push), so a range loop is clearer than juggling parallel iterators.
    #[allow(clippy::needless_range_loop)]
    for idx in prefix_objects..tail_start {
        let obj = &program.objects[ObjectIndex::from_raw(idx)];
        let (owner, kind) = match obj {
            Object::Class(_) => {
                let o = *class_owner.get(ci).ok_or_else(|| {
                    LoweringError::Internal(format!("class object {idx} has no owning file"))
                })?;
                ci += 1;
                (o, PoolObjKind::Class)
            }
            Object::Enum(_) => {
                let o = *enum_owner.get(ei).ok_or_else(|| {
                    LoweringError::Internal(format!("enum object {idx} has no owning file"))
                })?;
                ei += 1;
                (o, PoolObjKind::Enum)
            }
            Object::TypeAlias(alias) => {
                let o = *alias_name_to_file.get(&alias.name).ok_or_else(|| {
                    LoweringError::Internal(format!(
                        "type-alias object {idx} (`{}`) is declared in no source file",
                        alias.name.render_dotted(false)
                    ))
                })?;
                (o, PoolObjKind::TypeAlias)
            }
            Object::Interface(_) => {
                let o = *iface_owner.get(ii).ok_or_else(|| {
                    LoweringError::Internal(format!("interface object {idx} has no owning file"))
                })?;
                ii += 1;
                (o, PoolObjKind::Interface)
            }
            Object::Function(f) => {
                if let Some(name) = fn_obj_name.get(&idx) {
                    // Named function: attribute by fq name. A synthesized
                    // function ($init/$init_test) has no source file — reject
                    // (Stage 2 does not yet reproduce the $init tail; §9 R2).
                    let o = func_name_to_file.get(name).copied().ok_or_else(|| {
                        LoweringError::Internal(format!(
                            "named function `{name}` (obj {idx}) is synthesized \
                             ($init/$init_test); Stage 2 decomposition does not \
                             handle the $init tail yet (design §9 R2)"
                        ))
                    })?;
                    (o, PoolObjKind::NamedFn(name.clone()))
                } else {
                    // Lambda: attribute by its (relative) source file.
                    let o = rel_to_file
                        .get(f.source_file.as_str())
                        .copied()
                        .ok_or_else(|| {
                            LoweringError::Internal(format!(
                                "lambda object {idx} has source_file `{}` matching no file",
                                f.source_file
                            ))
                        })?;
                    (o, PoolObjKind::CodeAnon)
                }
            }
            Object::String(_)
            | Object::Bigint(_)
            | Object::Uint8Array(_)
            | Object::GenericFunction(_) => {
                // Codegen-interned literal (strings/bigints/byte-arrays) or a
                // cross-unit-interned generic-function value (§9 R1). Owner is
                // filled by the leading-literal pass: it belongs to whichever
                // function is compiled next.
                (usize::MAX, PoolObjKind::CodeAnon)
            }
            other => {
                return Err(LoweringError::Internal(format!(
                    "pool object {idx} is an unexpected compiled kind: {}",
                    obj_variant_name(other)
                )));
            }
        };
        obj_owner[idx] = owner;
        obj_kind.push(kind);
    }
    // Leading-literal attribution: a literal belongs to the NEXT function object
    // in the pool (a function's constants are interned before its own object is
    // pushed). Scan backwards, carrying the most recent function's owner.
    let mut next_func_owner = usize::MAX;
    for idx in (prefix_objects..tail_start).rev() {
        match &program.objects[ObjectIndex::from_raw(idx)] {
            Object::Function(_) => next_func_owner = obj_owner[idx],
            Object::String(_)
            | Object::Bigint(_)
            | Object::Uint8Array(_)
            | Object::GenericFunction(_) => {
                if next_func_owner == usize::MAX {
                    return Err(LoweringError::Internal(format!(
                        "interned literal object {idx} has no following function \
                         to attribute it to"
                    )));
                }
                obj_owner[idx] = next_func_owner;
            }
            _ => {}
        }
    }

    // ---- Bucket objects into units + record local layout --------------------
    let mut units: Vec<CompilationUnit> = (0..n_files)
        .map(|fi| CompilationUnit {
            source_file: unit_source[fi].clone(),
            package: unit_package[fi].clone(),
            ..CompilationUnit::default()
        })
        .collect();
    // Per pool object: its LocalRef within its owning unit (bucket + offset).
    let mut obj_localref: Vec<LocalRef> = Vec::with_capacity(tail_start - prefix_objects);
    for (offset, kind) in obj_kind.iter().enumerate() {
        let idx = prefix_objects + offset;
        let u = obj_owner[idx];
        let obj = program.objects[ObjectIndex::from_raw(idx)].clone();
        let local_ref = match kind {
            PoolObjKind::Class => {
                let off = units[u].classes.len();
                units[u].classes.push(obj);
                LocalRef::Class(u32::try_from(off).expect("class offset fits u32"))
            }
            PoolObjKind::Enum => {
                let off = units[u].enums.len();
                units[u].enums.push(obj);
                LocalRef::Enum(u32::try_from(off).expect("enum offset fits u32"))
            }
            PoolObjKind::Interface => {
                let off = units[u].interfaces.len();
                units[u].interfaces.push(obj);
                LocalRef::Interface(u32::try_from(off).expect("interface offset fits u32"))
            }
            PoolObjKind::TypeAlias => {
                let off = units[u].type_alias_objects.len();
                units[u].type_alias_objects.push(obj);
                LocalRef::TypeAlias(u32::try_from(off).expect("type-alias offset fits u32"))
            }
            PoolObjKind::NamedFn(_) | PoolObjKind::CodeAnon => {
                let off = units[u].code.len();
                units[u].code.push(obj);
                LocalRef::Code(u32::try_from(off).expect("code offset fits u32"))
            }
        };
        obj_localref.push(local_ref);
    }

    // ---- Global slot -> owner + local flat index ----------------------------
    // slot -> fq name (functions and lets).
    let mut slot_to_name: Vec<Option<String>> = vec![None; program.globals.len()];
    for (name, &slot) in &program.function_global_indices {
        slot_to_name[slot] = Some(name.clone());
    }
    for (name, &slot) in &program.let_global_indices {
        slot_to_name[slot] = Some(name.clone());
    }
    // name -> (unit, flat local global index). The local global space is
    // [functions 0..F_u][lets F_u..]; function ordinals follow pool (= Pass 1)
    // order, `let` ordinals follow file order.
    let mut name_to_local_global: HashMap<String, (usize, u32)> = HashMap::new();
    let mut func_next: Vec<u32> = vec![0; n_files];
    for idx in prefix_objects..tail_start {
        if let PoolObjKind::NamedFn(name) = &obj_kind[idx - prefix_objects] {
            // Only functions that own a global slot participate.
            if program.function_global_indices.contains_key(name) {
                let u = obj_owner[idx];
                let flat = func_next[u];
                func_next[u] += 1;
                name_to_local_global.insert(name.clone(), (u, flat));
            }
        }
    }
    let mut local_let_count = vec![0u32; n_files];
    for (fi, file) in all_files.iter().enumerate() {
        let mut let_ord = 0u32;
        for &let_loc in file_lets(db, *file) {
            let fq = def_to_item_ref(db, Definition::Let(let_loc)).to_string();
            if program.let_global_indices.contains_key(&fq) {
                name_to_local_global.insert(fq, (fi, func_next[fi] + let_ord));
                let_ord += 1;
            }
        }
        local_let_count[fi] = let_ord;
    }

    // ---- Rewrite code operands to the symbolic convention + build imports ---
    for (u, unit) in units.iter_mut().enumerate() {
        let n_classes = unit.classes.len();
        let n_enums = unit.enums.len();
        let n_ifaces = unit.interfaces.len();
        let n_aliases = unit.type_alias_objects.len();
        // Every bucket the unit owns, in the flat-local order `flat_local`
        // encodes — imports start right after. Must stay in step with the
        // linker's own `n_local_objects`, or an import decodes as a local.
        let n_local_objects = n_classes + n_enums + n_ifaces + n_aliases + unit.code.len();
        // The unit's local global space is [functions 0..F_u][lets F_u..]; its
        // size is where import globals start (§2a).
        let n_local_globals = (func_next[u] + local_let_count[u]) as usize;
        // Dedup maps for imports (fq name -> import index).
        let mut obj_import_idx: HashMap<String, usize> = HashMap::new();
        let mut glob_import_idx: HashMap<String, usize> = HashMap::new();
        let mut object_imports: Vec<Symbol> = Vec::new();
        let mut global_imports: Vec<Symbol> = Vec::new();

        // Precompute this unit's flat-local index for each pool object it owns.
        // (Captured references keep the closure `Fn`.)
        let flat_local = |abs: usize| -> usize {
            match obj_localref[abs - prefix_objects] {
                LocalRef::Class(k) => k as usize,
                LocalRef::Enum(k) => n_classes + k as usize,
                LocalRef::Interface(k) => n_classes + n_enums + k as usize,
                LocalRef::TypeAlias(k) => n_classes + n_enums + n_ifaces + k as usize,
                LocalRef::Code(k) => n_classes + n_enums + n_ifaces + n_aliases + k as usize,
            }
        };

        // Interfaces carry cross-object operands too (each default method's
        // pooled body), so they take the same symbolic rewrite as code objects.
        for object in unit.interfaces.iter_mut().chain(unit.code.iter_mut()) {
            rewrite_pool_operands(
                object,
                |target| {
                    if target >= prefix_objects && obj_owner[target] == u {
                        Ok(flat_local(target))
                    } else {
                        let sym = object_symbol(program, target, &fn_obj_name, &slot_to_name)?;
                        let import_idx = intern_import(
                            &mut object_imports,
                            &mut obj_import_idx,
                            sym.fq_name.clone(),
                            sym,
                        );
                        Ok(n_local_objects + import_idx)
                    }
                },
                |target| {
                    let Some(name) = slot_to_name.get(target).and_then(Option::as_ref) else {
                        return Err(LoweringError::Internal(format!(
                            "global slot {target} referenced by a unit object owns \
                             no function/let name (synthesized $init slot?); \
                             Stage 2 does not handle it yet (design §9 R2)"
                        )));
                    };
                    // Local iff this unit owns the slot; otherwise an import.
                    // A name absent from `name_to_local_global` is a reference
                    // to a definition not lowered into this (partial) pool —
                    // i.e. a clean file's function/let in the Stage 6 dirty-only
                    // emit — which is likewise an import.
                    let owned_local = name_to_local_global
                        .get(name)
                        .filter(|&&(owner, _)| owner == u)
                        .map(|&(_, flat)| flat as usize);
                    if let Some(flat) = owned_local {
                        Ok(flat)
                    } else {
                        let is_let = let_name_to_file.contains_key(name);
                        let sym = Symbol {
                            kind: if is_let {
                                SymbolKind::Let
                            } else {
                                SymbolKind::Function
                            },
                            fq_name: name.clone(),
                            generic: None,
                        };
                        let import_idx = intern_import(
                            &mut global_imports,
                            &mut glob_import_idx,
                            name.clone(),
                            sym,
                        );
                        Ok(n_local_globals + import_idx)
                    }
                },
            )?;
        }
        unit.object_imports = object_imports;
        unit.global_imports = global_imports;
    }

    // ---- Export tables ------------------------------------------------------
    for idx in prefix_objects..tail_start {
        let u = obj_owner[idx];
        match &obj_kind[idx - prefix_objects] {
            PoolObjKind::Class
            | PoolObjKind::Enum
            | PoolObjKind::Interface
            | PoolObjKind::TypeAlias => {
                let fq = def_object_fq(&program.objects[ObjectIndex::from_raw(idx)]);
                units[u]
                    .exports
                    .objects
                    .push((fq, obj_localref[idx - prefix_objects]));
            }
            PoolObjKind::NamedFn(name) => {
                units[u]
                    .exports
                    .objects
                    .push((name.clone(), obj_localref[idx - prefix_objects]));
            }
            PoolObjKind::CodeAnon => {}
        }
    }
    for (name, &(u, flat)) in &name_to_local_global {
        units[u].exports.globals.push((name.clone(), flat));
    }
    // Deterministic export order (not load-bearing for the linked Program, which
    // re-derives everything, but keeps the serialized unit stable).
    for unit in &mut units {
        unit.exports
            .objects
            .sort_by_key(|a| local_ref_sort_key(a.1));
        unit.exports.globals.sort_by_key(|a| a.1);
    }

    // ---- $init / $init_test tail extraction (design §9 R2) ------------------
    if tail_start < n_obj {
        let tail = build_init_tail(
            program,
            tail_start,
            &fn_obj_name,
            &slot_to_name,
            &let_name_to_file,
        )?;
        // The tail belongs to the user group; carry it on the last user unit.
        let carrier = (0..n_files)
            .rev()
            .find(|&fi| !unit_source[fi].starts_with("<builtin>/"))
            .unwrap_or(n_files.saturating_sub(1));
        units[carrier].init_tail = Some(tail);
    }

    // ---- Package fragments --------------------------------------------------
    // Each package's fragment is carried by the first unit (lowest file index)
    // whose file belongs to that package; the linker's merge re-sorts, so this
    // attribution is order-free.
    let mut package_first_unit: HashMap<Name, usize> = HashMap::new();
    for (u, pkg) in unit_package.iter().enumerate() {
        package_first_unit.entry(pkg.clone()).or_insert(u);
    }
    for (pkg_name, pkg) in &program.packages {
        let Some(&carrier) = package_first_unit.get(pkg_name) else {
            // Source-less dependency packages are supplied by the linked
            // prefix. Their package fragments must stay in that immutable
            // image rather than being copied onto a consumer unit.
            continue;
        };
        let frag = build_package_fragment(program, pkg, &fn_obj_name)?;
        units[carrier].package_fragment = frag;
    }

    // ---- Test cases (Pass 8 fragment, per file by source path) --------------
    if options.emit_test_cases {
        for test in &program.test_cases {
            if let Some(&fi) = rel_to_file.get(test.source_file.as_str()) {
                units[fi].test_cases.push(test.clone());
            } else {
                return Err(LoweringError::Internal(format!(
                    "test case `{}` has source_file `{}` matching no file",
                    test.name, test.source_file
                )));
            }
        }
    }

    // ---- Interface fragments (Phase 2b, per user file) ----------------------
    // Carry each user file's typed interface fragment (opaque borsh) beside its
    // bytecode so a warm compile can project a `callable_throws` seed from clean
    // files' units. Builtins (empty or `<builtin>/…` source paths) are covered
    // by the B-694 stdlib interface blob and only user files appear in the
    // manifest the seed reads, so they carry no fragment — matching
    // `user_files_with_rel_paths`' predicate. The fragment holds no absolute
    // paths (design §9 R7). Best-effort: a fragment that fails to serialize stays
    // empty (the file is then treated as unseeded on load).
    for (fi, file) in all_files.iter().enumerate() {
        if units[fi].source_file.is_empty() || units[fi].source_file.starts_with("<builtin>/") {
            continue;
        }
        let fragment =
            baml_compiler2_hir_ty::package_interface::file_callable_throws_fragment(db, *file);
        if let Ok(bytes) = borsh::to_vec(fragment) {
            units[fi].callable_throws_fragment = bytes;
        }
    }

    Ok(units)
}

/// Sort key that orders `LocalRef`s by bucket then offset.
fn local_ref_sort_key(r: LocalRef) -> (u8, u32) {
    match r {
        LocalRef::Class(k) => (0, k),
        LocalRef::Enum(k) => (1, k),
        LocalRef::Interface(k) => (2, k),
        LocalRef::TypeAlias(k) => (3, k),
        LocalRef::Code(k) => (4, k),
    }
}

/// Fully-qualified name of a class/enum/interface definition object.
fn def_object_fq(obj: &Object) -> String {
    match obj {
        Object::Class(c) => c.name.to_string(),
        Object::Enum(e) => e.name.to_string(),
        Object::Interface(i) => i.name.to_string(),
        Object::TypeAlias(a) => a.name.to_string(),
        _ => unreachable!("def_object_fq on non-definition object"),
    }
}

/// Intern `sym` into `imports`, deduplicated by `key`, returning its import
/// index. A repeated key reuses the first slot, so a unit's import table holds
/// one entry per referenced symbol.
fn intern_import(
    imports: &mut Vec<Symbol>,
    dedup: &mut HashMap<String, usize>,
    key: String,
    sym: Symbol,
) -> usize {
    *dedup.entry(key).or_insert_with(|| {
        let n = imports.len();
        imports.push(sym);
        n
    })
}

/// Rewrite every cross-object index operand of `object` from whole-program space
/// into the per-unit / import-relative encoding. `resolve_object` and
/// `resolve_global` map a raw program index to its rewritten value, or a
/// [`LoweringError`] for an operand that cannot be attributed; the first such
/// error stops the walk and is returned. Shared by the per-file decomposition
/// and the `$init`-tail encoding — only the local-vs-import decision differs.
fn rewrite_pool_operands(
    object: &mut Object,
    mut resolve_object: impl FnMut(usize) -> Result<usize, LoweringError>,
    mut resolve_global: impl FnMut(usize) -> Result<usize, LoweringError>,
) -> Result<(), LoweringError> {
    let mut err: Option<LoweringError> = None;
    bex_vm_types::relink::visit_object_operands(object, |operand| {
        if err.is_some() {
            return;
        }
        match operand {
            bex_vm_types::relink::IndexOperand::Object(idx) => match resolve_object(idx.raw()) {
                Ok(new) => *idx = ObjectIndex::from_raw(new),
                Err(e) => err = Some(e),
            },
            bex_vm_types::relink::IndexOperand::Global(slot) => match resolve_global(slot.raw()) {
                Ok(new) => *slot = GlobalIndex::from_raw(new),
                Err(e) => err = Some(e),
            },
        }
    });
    match err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn obj_variant_name(obj: &Object) -> &'static str {
    match obj {
        Object::Function(_) => "Function",
        Object::Class(_) => "Class",
        Object::Enum(_) => "Enum",
        Object::TypeAlias(_) => "TypeAlias",
        Object::Interface(_) => "Interface",
        Object::Package(_) => "Package",
        Object::ImplRule(_) => "ImplRule",
        Object::String(_) => "String",
        Object::Bigint(_) => "Bigint",
        Object::Uint8Array(_) => "Uint8Array",
        Object::Type(_) => "Type",
        Object::GenericFunction(_) => "GenericFunction",
        _ => "runtime-only",
    }
}

/// Resolve the fully-qualified base-function name of a generic-function value in
/// a [`CompilationUnit`]'s per-unit encoding (§2a): a local-global base resolves
/// through the unit's export table, an imported base through its global imports.
fn unit_generic_base_name(unit: &CompilationUnit, base_raw: usize) -> Option<String> {
    let n_local_globals = unit.exports.globals.len();
    if base_raw < n_local_globals {
        unit.exports
            .globals
            .iter()
            .find(|(_, flat)| *flat as usize == base_raw)
            .map(|(name, _)| name.clone())
    } else {
        unit.global_imports
            .get(base_raw - n_local_globals)
            .map(|sym| sym.fq_name.clone())
    }
}

/// Does the freshly-synthesized `$init`/`$init_test` tail intern a
/// generic-function value that a *clean* file already owns in its `code` bucket
/// (design §9 R1 tail edge)? Such a duplicate cannot be deduped by the linker's
/// code-bucket interning, so the reuse path must fall back for it.
fn tail_generic_dupes_clean(
    tail: &bex_vm_types::InitTail,
    prev_units: &[CompilationUnit],
    effective_clean: &HashSet<String>,
) -> bool {
    // (base fn fq name, type args) of every generic value clean files own.
    // `GenericFunction::type_args` is `RealizedTy` (runtime narrowing, #3998).
    let mut clean_keys: Vec<(String, Vec<bex_vm_types::RealizedTy>)> = Vec::new();
    for unit in prev_units {
        if !effective_clean.contains(&unit.source_file) {
            continue;
        }
        for obj in &unit.code {
            if let Object::GenericFunction(gf) = obj
                && let Some(base) = unit_generic_base_name(unit, gf.function.raw())
            {
                clean_keys.push((base, gf.type_args.to_vec()));
            }
        }
    }
    if clean_keys.is_empty() {
        return false;
    }
    // A tail generic's base is always a function, so it is a tail global import.
    let n_tail_slots = tail.slot_objects.len();
    for obj in &tail.objects {
        if let Object::GenericFunction(gf) = obj {
            let base_raw = gf.function.raw();
            if base_raw < n_tail_slots {
                continue; // a helper slot is never a generic base
            }
            let Some(sym) = tail.global_imports.get(base_raw - n_tail_slots) else {
                continue;
            };
            if clean_keys.iter().any(|(name, args)| {
                name == &sym.fq_name && args.as_slice() == gf.type_args.as_ref()
            }) {
                return true;
            }
        }
    }
    false
}

/// Build the import [`Symbol`] for a cross-unit object reference by inspecting
/// the target pool object.
fn object_symbol(
    program: &Program,
    target: usize,
    fn_obj_name: &HashMap<usize, String>,
    slot_to_name: &[Option<String>],
) -> Result<Symbol, LoweringError> {
    let obj = &program.objects[ObjectIndex::from_raw(target)];
    match obj {
        Object::GenericFunction(gf) => {
            // A generic-function value (`foo<int>`) interned in another unit
            // (design §9 R1). The intern key is `(base function, type_args)`; the
            // linker re-resolves it from the base function's name.
            let base_slot = gf.function.raw();
            let base_fn = slot_to_name
                .get(base_slot)
                .and_then(Option::clone)
                .ok_or_else(|| {
                    LoweringError::Internal(format!(
                        "generic-function object {target} has base slot {base_slot} \
                     with no function name"
                    ))
                })?;
            Ok(Symbol {
                kind: SymbolKind::GenericFn,
                fq_name: base_fn.clone(),
                generic: Some(bex_vm_types::GenericFnKey {
                    base_fn,
                    type_args: gf.type_args.to_vec(),
                }),
            })
        }
        _ => {
            let (kind, fq_name) = match obj {
                Object::Class(c) => (SymbolKind::Class, c.name.to_string()),
                Object::Enum(e) => (SymbolKind::Enum, e.name.to_string()),
                Object::Interface(i) => (SymbolKind::Interface, i.name.to_string()),
                Object::Function(_) => match fn_obj_name.get(&target) {
                    Some(name) => (SymbolKind::Function, name.clone()),
                    None => {
                        return Err(LoweringError::Internal(format!(
                            "cross-unit reference to lambda object {target} (lambdas \
                             are never cross-unit)"
                        )));
                    }
                },
                _ => {
                    return Err(LoweringError::Internal(format!(
                        "cross-unit reference to a non-def object {target} \
                         ({}); only classes/enums/interfaces/functions are importable",
                        obj_variant_name(obj)
                    )));
                }
            };
            Ok(Symbol {
                kind,
                fq_name,
                generic: None,
            })
        }
    }
}

/// Convert a whole-program [`ProgramPackage`](bex_vm_types::types::ProgramPackage)
/// into its symbolic fragment by resolving every `ObjectIndex` back to its
/// object's fully-qualified name.
fn build_package_fragment(
    program: &Program,
    pkg: &bex_vm_types::types::ProgramPackage,
    fn_obj_name: &HashMap<usize, String>,
) -> Result<ProgramPackageFrag, LoweringError> {
    let obj_fq = |idx: ObjectIndex| -> Result<String, LoweringError> {
        let raw = idx.raw();
        // A function reference resolves by name directly — this covers both real
        // pool functions and the placeholder indices Stage 6 assigns to skipped
        // clean-file functions (which must never be pool-indexed).
        if let Some(name) = fn_obj_name.get(&raw) {
            return Ok(name.clone());
        }
        match &program.objects[idx] {
            Object::Class(c) => Ok(c.name.to_string()),
            Object::Enum(e) => Ok(e.name.to_string()),
            Object::Interface(i) => Ok(i.name.to_string()),
            Object::TypeAlias(a) => Ok(a.name.to_string()),
            Object::Function(_) => Err(LoweringError::Internal(format!(
                "package refs unnamed function object {raw}"
            ))),
            other => Err(LoweringError::Internal(format!(
                "package refs a non-def object {raw} ({})",
                obj_variant_name(other)
            ))),
        }
    };
    let mut frag = ProgramPackageFrag::default();
    frag.exported_names.clone_from(&pkg.exported_names);
    for (local, &idx) in &pkg.classes {
        frag.classes.push((local.clone(), obj_fq(idx)?));
    }
    for (local, &idx) in &pkg.enums {
        frag.enums.push((local.clone(), obj_fq(idx)?));
    }
    for (local, &idx) in &pkg.interfaces {
        frag.interfaces.push((local.clone(), obj_fq(idx)?));
    }
    for (local, &idx) in &pkg.functions {
        frag.functions.push((local.clone(), obj_fq(idx)?));
    }
    for (local, &idx) in &pkg.type_aliases {
        frag.type_aliases.push((local.clone(), obj_fq(idx)?));
    }
    for (&iface_idx, rules) in &pkg.impl_rules {
        let iface_fq = obj_fq(iface_idx)?;
        let mut rule_frags = Vec::with_capacity(rules.len());
        for rule in rules {
            let mut methods = Vec::with_capacity(rule.methods.len());
            for (name, method) in &rule.methods {
                methods.push((
                    name.clone(),
                    ProgramMethodImplFrag {
                        fqn: obj_fq(method.fqn)?,
                        frame: method.frame.clone(),
                    },
                ));
            }
            rule_frags.push(ProgramImplRuleFrag {
                interface_head: obj_fq(rule.interface_head)?,
                for_ty_pattern: rule.for_ty_pattern.clone(),
                generic_param_bounds: rule.generic_param_bounds.clone(),
                interface_args: rule.interface_args.clone(),
                interface_assoc: rule.interface_assoc.clone(),
                methods,
                field_links: rule.field_links.clone(),
            });
        }
        frag.impl_rules.push((iface_fq, rule_frags));
    }
    frag.interface_blob.clone_from(&pkg.interface_blob);
    frag.test_init = pkg.test_init.map(obj_fq).transpose()?;
    Ok(frag)
}

/// Extract the `$init`/`$init_test` tail (design §9 R2) from a flat `Program`:
/// the objects in `[tail_start, program.objects.len())`, with operands rewritten
/// to the tail-local/import convention of [`bex_vm_types::InitTail`].
#[allow(clippy::too_many_lines)]
fn build_init_tail(
    program: &Program,
    tail_start: usize,
    fn_obj_name: &HashMap<usize, String>,
    slot_to_name: &[Option<String>],
    let_name_to_file: &HashMap<String, usize>,
) -> Result<bex_vm_types::InitTail, LoweringError> {
    let n_obj = program.objects.len();
    let n_tail_objects = n_obj - tail_start;

    // Object index -> global slot (a function/helper slot holds `Object(obj)`).
    let mut obj_slot: HashMap<usize, usize> = HashMap::new();
    for (s, val) in program.globals.iter().enumerate() {
        if let ConstValue::Object(o) = val {
            obj_slot.insert(o.raw(), s);
        }
    }
    // Tail slots: those owned by a tail object. They form a dense suffix.
    let mut tail_slots: Vec<(usize, usize)> = Vec::new(); // (abs slot, tail obj idx)
    for tidx in tail_start..n_obj {
        if let Some(&s) = obj_slot.get(&tidx) {
            tail_slots.push((s, tidx));
        }
    }
    tail_slots.sort_by_key(|&(s, _)| s);
    let tail_slot_base = tail_slots
        .first()
        .map_or(program.globals.len(), |&(s, _)| s);
    for (ord, &(s, _)) in tail_slots.iter().enumerate() {
        if s != tail_slot_base + ord {
            return Err(LoweringError::Internal(format!(
                "$init tail slots are not a dense suffix: slot {s} at ordinal {ord} \
                 (base {tail_slot_base})"
            )));
        }
    }
    let n_tail_slots = tail_slots.len();
    let slot_objects: Vec<u32> = tail_slots
        .iter()
        .map(|&(_, tidx)| u32::try_from(tidx - tail_start).expect("tail offset fits u32"))
        .collect();

    // Named tail functions: `$init` / `$init_test` chainers (helpers are nameless).
    // Guard against clean-file placeholder indices (injected past the real pool in
    // a Stage 6 dirty-only emit), which are `>= n_obj` and are not tail objects.
    let mut named: Vec<(String, u32)> = Vec::new();
    for (name, &idx) in &program.function_indices {
        if idx >= tail_start && idx < n_obj {
            named.push((
                name.clone(),
                u32::try_from(idx - tail_start).expect("tail offset fits u32"),
            ));
        }
    }
    named.sort_by_key(|a| a.1);

    // Encode each tail object's operands.
    let mut obj_import_idx: HashMap<String, usize> = HashMap::new();
    let mut glob_import_idx: HashMap<String, usize> = HashMap::new();
    let mut object_imports: Vec<Symbol> = Vec::new();
    let mut global_imports: Vec<Symbol> = Vec::new();
    let mut objects: Vec<Object> = Vec::with_capacity(n_tail_objects);
    for tidx in tail_start..n_obj {
        let mut object = program.objects[ObjectIndex::from_raw(tidx)].clone();
        rewrite_pool_operands(
            &mut object,
            |t| {
                if t >= tail_start {
                    Ok(t - tail_start)
                } else {
                    let sym = object_symbol(program, t, fn_obj_name, slot_to_name)?;
                    let import_idx = intern_import(
                        &mut object_imports,
                        &mut obj_import_idx,
                        sym.fq_name.clone(),
                        sym,
                    );
                    Ok(n_tail_objects + import_idx)
                }
            },
            |s| {
                if s >= tail_slot_base {
                    Ok(s - tail_slot_base)
                } else {
                    let Some(name) = slot_to_name.get(s).and_then(Option::as_ref) else {
                        return Err(LoweringError::Internal(format!(
                            "$init tail references unnamed non-tail slot {s}"
                        )));
                    };
                    let is_let = let_name_to_file.contains_key(name);
                    let sym = Symbol {
                        kind: if is_let {
                            SymbolKind::Let
                        } else {
                            SymbolKind::Function
                        },
                        fq_name: name.clone(),
                        generic: None,
                    };
                    let import_idx =
                        intern_import(&mut global_imports, &mut glob_import_idx, name.clone(), sym);
                    Ok(n_tail_slots + import_idx)
                }
            },
        )?;
        objects.push(object);
    }

    Ok(bex_vm_types::InitTail {
        objects,
        object_imports,
        global_imports,
        slot_objects,
        named,
        package_init_order: program.package_init_order.clone(),
    })
}

thread_local! {
    /// Project-relative paths whose bodies were MIR/bytecode-lowered in Pass 4
    /// since the last drain. B-693 Stage 6 evidence surface: after an incremental
    /// compile only the *dirty* files appear here — clean files are never lowered.
    static LOWERED_FILES: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Record that `rel_path`'s function bodies are being lowered in Pass 4.
fn record_lowered_file(rel_path: &str) {
    LOWERED_FILES.with(|f| f.borrow_mut().push(rel_path.to_string()));
}

/// Drain and return the source paths whose bodies were lowered since the last
/// call, on the current thread (B-693 Stage 6). A full compile returns every
/// file; an incremental reuse compile returns only the dirty files.
pub fn take_lowered_files() -> Vec<String> {
    LOWERED_FILES.with(|f| std::mem::take(&mut *f.borrow_mut()))
}

/// Stage 6 (`SkipClean`) phase 1: register clean (skipped) files' function/let
/// global **slots** so the whole-program `$init` / `$init_test` tail synthesis
/// (Passes 4.5/4.6) sees the entire project — a clean file's `$init_test_<path>`
/// must be chained by `$init_test`, and a clean `let` owns a slot the tail may
/// reference. Slots are the whole-project (Pass-1) values, identical to a full
/// compile. Clean function *object* placeholders are injected separately, after
/// the tail is emitted (see [`inject_clean_object_placeholders`]), so they land
/// past the real pool.
fn inject_clean_slots(
    db: &dyn baml_compiler2_mir::Db,
    files: &[baml_base::SourceFile],
    clean: &HashSet<String>,
    globals: &HashMap<String, usize>,
    program: &mut Program,
) {
    for file in files {
        let rel = relative_source_path(db, *file);
        if !clean.contains(&rel) {
            continue;
        }
        for &func_loc in file_functions(db, *file) {
            // Required interface methods are signature-only items: nothing
            // to compile or index (mirrors their pre-item invisibility here).
            if baml_compiler2_ppir::item_data::is_required_interface_method(db, func_loc) {
                continue;
            }
            let fq = def_to_item_ref(db, Definition::Function(func_loc)).to_string();
            // Intrinsic / await-any functions own no slot (Pass 1 skips them);
            // the `globals` guard drops them here too.
            if let Some(&slot) = globals.get(&fq) {
                program.function_global_indices.insert(fq, slot);
            }
        }
        for &let_loc in file_lets(db, *file) {
            let fq = def_to_item_ref(db, Definition::Let(let_loc)).to_string();
            if let Some(&slot) = globals.get(&fq) {
                program.let_global_indices.insert(fq, slot);
            }
        }
    }
}

/// Stage 6 (`SkipClean`) phase 2: register clean functions' **object index
/// placeholders** so `build_packages` (impl-rule method FQNs) and the
/// decomposition's operand reversal can map a clean function's index back to its
/// name. Each placeholder is past the real pool (only ever reversed to a name,
/// never pool-indexed). Must run **after** the `$init`/`$init_test` tail is
/// emitted so the placeholders do not collide with the tail's real objects.
fn inject_clean_object_placeholders(
    db: &dyn baml_compiler2_mir::Db,
    files: &[baml_base::SourceFile],
    clean: &HashSet<String>,
    globals: &HashMap<String, usize>,
    program: &mut Program,
) {
    let mut placeholder = program.objects.len();
    for file in files {
        let rel = relative_source_path(db, *file);
        if !clean.contains(&rel) {
            continue;
        }
        for &func_loc in file_functions(db, *file) {
            // Required interface methods are signature-only items: nothing
            // to compile or index (mirrors their pre-item invisibility here).
            if baml_compiler2_ppir::item_data::is_required_interface_method(db, func_loc) {
                continue;
            }
            let fq = def_to_item_ref(db, Definition::Function(func_loc)).to_string();
            if globals.get(&fq).is_some() {
                program.function_indices.entry(fq).or_insert_with(|| {
                    let idx = placeholder;
                    placeholder += 1;
                    idx
                });
            }
        }
    }
}

/// Emit the whole project (B-693 Stage 6 core).
///
/// `skip_clean`, when `Some`, is the set of project-relative paths whose files
/// are **clean** in an incremental compile: Pass 4 skips them entirely (they are
/// neither lowered nor cloned — their units come verbatim from the cached image),
/// and only the dirty files are lowered. Dirty functions are written to their
/// whole-project (Pass-1) global slots so the decomposition reverses their
/// operands to names identically to a full compile. `None` is a full compile.
fn generate_impl(
    db: &dyn crate::Db,
    options: &CompileOptions,
    opt: OptLevel,
    base: Option<&Program>,
    stdlib_only: bool,
    skip_clean: Option<&HashSet<String>>,
) -> Result<Program, LoweringError> {
    let mut all_files = compiler2_all_files(db);
    let builtin_count = if base.is_some()
        && !baml_compiler2_hir::package::precompiled_package_names(db).is_empty()
    {
        // A source-less stdlib database has no builtin sources to skip. Any
        // `<builtin>/…` files it does hold are link stubs for ordinary runtime
        // mounts and must be emitted into temporary dependency units.
        0
    } else {
        all_files
            .iter()
            .take_while(|f| f.path(db).to_string_lossy().starts_with("<builtin>/"))
            .count()
    };
    if stdlib_only {
        // The builtin prefix is user-independent, so compiling "just the
        // stdlib" is the full pipeline over the builtin files alone.
        all_files.truncate(builtin_count);
    }
    let alias_caches = build_alias_caches(db, &all_files);

    // Emit in two file groups — builtin stubs first, then user files — so the
    // stdlib occupies a contiguous, user-independent prefix of the ObjectPool
    // and the globals table (`compiler2_all_files` puts builtins first for the
    // same reason). The precompiled-stdlib splice depends on that prefix.
    let (builtin_files, user_files) = all_files.split_at(builtin_count.min(all_files.len()));
    let (mut program, mut tables) = match base {
        // Splice mode: the builtin group's output is taken wholesale from the
        // precompiled slice; whole-program products it carries (template
        // macros, packages) are recomputed by the trailing passes below,
        // exactly as a full compile would.
        Some(base) => (base.clone(), EmitTables::from_stdlib_program(base)),
        None => (Program::new(), EmitTables::default()),
    };
    if base.is_none() {
        emit_file_group(
            db,
            builtin_files,
            &mut tables,
            &mut program,
            &alias_caches,
            opt,
            None,
        )?;
    }
    emit_file_group(
        db,
        user_files,
        &mut tables,
        &mut program,
        &alias_caches,
        opt,
        skip_clean,
    )?;
    // Derive the serializable compiler surface after ordinary lowering has
    // populated Salsa's body/signature caches. The artifact is unchanged; only
    // the scheduling avoids a cold whole-package inference traversal before
    // the same bodies are lowered for emit.
    let package_exports = capture_package_exports(db, &all_files);

    // --- Pass 6: Retry policies ---
    // Retry policies are now synthesized as Item::Let bindings during CST lowering.
    // Their values flow through the $init pipeline instead of being parsed here.
    // Pass 6 is intentionally empty.

    // Client metadata is now synthesized as Item::Let bindings during CST lowering.
    // Client values (including sub-clients, retry policies) flow through the $init pipeline.
    // Pass 7 is intentionally empty.

    let interface_default_backfill = build_packages(
        db,
        &all_files,
        &alias_caches,
        &program.function_indices,
        &tables.interface_object_indices,
        &PackageBuildMetadata {
            class_field_indices: &tables.classes,
            package_exports: &package_exports,
        },
        &mut tables.program_packages,
    );
    apply_interface_default_backfill(&mut program, &interface_default_backfill);
    // Mounted packages contribute no source files to this database. Preserve
    // their compiled package records from the linked prefix after the ordinary
    // source-backed package pass rebuilds the consumer metadata.
    if let Some(base) = base {
        for pkg_name in baml_compiler2_hir::package::external_package_names(db) {
            let Some(base_pkg) = base.packages.get(&pkg_name) else {
                continue;
            };
            match tables.program_packages.get_mut(&pkg_name) {
                Some(pkg) => {
                    pkg.functions.clone_from(&base_pkg.functions);
                    pkg.exported_names.clone_from(&base_pkg.exported_names);
                    pkg.interface_blob.clone_from(&base_pkg.interface_blob);
                    pkg.test_init = base_pkg.test_init;
                    pkg.impl_rules.clone_from(&base_pkg.impl_rules);
                    pkg.type_aliases.clone_from(&base_pkg.type_aliases);
                }
                None => {
                    tables
                        .program_packages
                        .insert(pkg_name.clone(), base_pkg.clone());
                }
            }
        }
    }
    tables.program_packages.sort_keys();
    program.packages = tables.program_packages;

    // --- Pass 8: Test cases (only when requested) ---
    if options.emit_test_cases {
        for file in &all_files {
            for &test_loc in file_tests(db, *file) {
                let test = test_data(db, test_loc);
                let function_names: Vec<String> =
                    test.function_refs.iter().map(ToString::to_string).collect();
                let args: indexmap::IndexMap<String, bex_vm_types::TestArgValue> = test
                    .args
                    .iter()
                    .map(|(k, v)| (k.to_string(), convert_test_arg_value(v)))
                    .collect();
                program.test_cases.push(bex_vm_types::TestCase {
                    name: test.name.to_string(),
                    function_names,
                    args,
                    source_file: relative_source_path(db, *file),
                });
            }
        }
    }

    Ok(program)
}

/// Emit tables accumulated across file groups.
///
/// One instance is threaded through both [`emit_file_group`] calls (builtin
/// stubs, then user files) so the user group can reference builtin classes,
/// enums, interfaces, and globals by the indices the builtin group assigned.
#[derive(Default)]
struct EmitTables {
    /// Function/let fq-name → global slot (Pass 1).
    globals: HashMap<String, usize>,
    /// Class fq-name → (field name → field index) (Pass 2).
    classes: HashMap<String, HashMap<String, usize>>,
    /// Class fq-name → `ObjectPool` index (Pass 2).
    class_object_indices: HashMap<String, usize>,
    /// Collision detector for content-addressed head type tags: tag → fq-name
    /// of the declaration that claimed it. Shared across every kind that can
    /// head a nominal type — classes (Pass 2), enums (Pass 3), interfaces —
    /// since fq-names are unique across kinds and all three draw from one tag
    /// space. Tags are a pure function of the fully-qualified name
    /// (`typetag::TypeTag::of_head`), so MIR agrees by construction; a 47-bit
    /// hash collision is reported as a compile error via [`claim_type_tag`].
    type_tags: HashMap<baml_type::typetag::TypeTag, String>,
    /// Enum fq-name → (variant name → variant index) (Pass 3).
    enum_variants: HashMap<String, HashMap<String, usize>>,
    /// Enum fq-name → `ObjectPool` index (Pass 3).
    enum_object_indices: HashMap<String, usize>,
    /// Interface type-name → `ObjectPool` index (Pass 3b).
    interface_object_indices: HashMap<baml_type::TypeName, usize>,
    /// Per-package structure the loader builds `Object::Package` from.
    program_packages: indexmap::IndexMap<Name, bex_vm_types::types::ProgramPackage>,
}

impl EmitTables {
    /// Reconstruct the emit tables the builtin group would have produced,
    /// purely from a precompiled stdlib `Program` slice.
    ///
    /// Everything the user group needs to reference builtin items is
    /// recoverable from the artifact: name→slot maps are stored on the
    /// `Program`; class/enum/interface metadata (field order, variant order)
    /// lives in the pool objects; class type tags are content hashes stored on
    /// each class object. Per-package `impl_rules` are cleared — the trailing
    /// whole-program pass regenerates them from all files exactly as a full
    /// compile does (keeping them would double the rule vectors). Type-alias
    /// entries are kept, because their `Object::TypeAlias`es are emitted
    /// per declaring file (Pass 3c) and the builtin group is not re-emitted on
    /// this path; the spliced pool preserves the base's object indices, so the
    /// entries stay valid exactly as the class/enum/interface ones do.
    fn from_stdlib_program(base: &Program) -> Self {
        let mut tables = EmitTables::default();

        for (name, &slot) in &base.function_global_indices {
            tables.globals.insert(name.clone(), slot);
        }
        for (name, &slot) in &base.let_global_indices {
            tables.globals.insert(name.clone(), slot);
        }

        for (idx, obj) in base.objects.iter().enumerate() {
            match obj {
                Object::Class(class) => {
                    let fq = class.name.to_string();
                    let field_indices = class
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| (f.name.clone(), i))
                        .collect();
                    tables.classes.insert(fq.clone(), field_indices);
                    tables.class_object_indices.insert(fq.clone(), idx);
                    tables.type_tags.insert(class.type_tag, fq);
                }
                Object::Enum(enum_def) => {
                    let fq = enum_def.name.to_string();
                    let variant_indices = enum_def
                        .variants
                        .iter()
                        .enumerate()
                        .map(|(i, v)| (v.name.clone(), i))
                        .collect();
                    tables.enum_variants.insert(fq.clone(), variant_indices);
                    tables.enum_object_indices.insert(fq, idx);
                }
                Object::Interface(iface) => {
                    tables
                        .interface_object_indices
                        .insert(iface.name.clone(), idx);
                }
                _ => {}
            }
        }

        tables.program_packages = base
            .packages
            .iter()
            .map(|(pkg_name, pkg)| {
                let mut pkg = pkg.clone();
                pkg.impl_rules.clear();
                (pkg_name.clone(), pkg)
            })
            .collect();

        tables
    }
}

/// Dirty-set throws gate (design §4): a caller-clean file may only be reused if
/// every one of its functions' inferred transitive `throws` still matches the
/// previous compile.
///
/// `throws` is inferred from bodies, so it is interface that the
/// body-blanked signature hash cannot see: a body edit elsewhere in the
/// package can change a clean file's *transitive* throws — its stored
/// `throws_type` metadata and, through catch lowering, potentially its
/// bytecode. The package-wide throw graph is already solved on this path,
/// so the comparison costs only map lookups; any mismatch demotes the file
/// to a normal recompile. No fixpoint is needed: the graph is solved
/// globally, so every affected file's own transitive set differs and each
/// demotes independently.
fn spliced_throws_match(
    db: &dyn baml_compiler2_mir::Db,
    file: baml_base::SourceFile,
    previous: &HashMap<&str, &bex_vm_types::TyTemplate>,
    cache: &ResolvedAliases,
) -> Result<(), String> {
    for &func_loc in file_functions(db, file) {
        // Required interface methods are signature-only items: nothing
        // to compile or index (mirrors their pre-item invisibility here).
        if baml_compiler2_ppir::item_data::is_required_interface_method(db, func_loc) {
            continue;
        }
        // Mirror Pass 4's skip set: these never become callable objects.
        if matches!(
            function_body(db, func_loc).as_ref(),
            baml_compiler2_hir::body::FunctionBody::Builtin(
                BuiltinKind::Intrinsic | BuiltinKind::AwaitAny
            )
        ) {
            continue;
        }
        let fq = def_to_item_ref(db, Definition::Function(func_loc)).to_string();
        let Some(previous_throws) = previous.get(fq.as_str()) else {
            return Err(format!("previous units have no function `{fq}`"));
        };
        let current_throws = compute_throws_type(
            db,
            file,
            &function_data(db, func_loc).name,
            cache,
            &baml_compiler2_hir_ty::lower::function_generic_frame(db, func_loc),
        );
        if **previous_throws != bex_vm_types::anchor_template(&current_throws) {
            return Err(format!(
                "function `{fq}` changed from {:?} to {current_throws:?}",
                **previous_throws
            ));
        }
    }
    Ok(())
}

/// Run emit passes 1–4.6 over one file group.
///
/// `generate_project_bytecode_with_opt` calls this twice — builtin stubs
/// first, then user files — so the stdlib occupies a contiguous,
/// user-independent prefix of the `ObjectPool` and the globals table. That
/// prefix property is what makes a precompiled stdlib `Program` slice (keyed
/// only by the compiler build) spliceable into any project's compile.
#[allow(clippy::too_many_arguments)]
fn emit_file_group(
    db: &dyn crate::Db,
    files: &[baml_base::SourceFile],
    tables: &mut EmitTables,
    program: &mut Program,
    alias_caches: &HashMap<Name, ResolvedAliases>,
    opt: OptLevel,
    skip_clean: Option<&HashSet<String>>,
) -> Result<(), LoweringError> {
    let EmitTables {
        globals,
        classes,
        class_object_indices,
        type_tags,
        enum_variants,
        enum_object_indices,
        interface_object_indices,
        program_packages,
    } = tables;

    // --- Pass 1: Build globals map (function name -> global index) ---
    // Functions are allocated first (slots 0..N-1), then let bindings (slots N..M-1).
    // This ensures function slots match the order they're appended to program.globals
    // in Pass 4, and let binding slots don't interleave with function slots.
    // Continue after every slot earlier groups pushed (functions, lets,
    // $init/helpers) — synthesized functions consume slots beyond the named
    // items, so `program.globals.len()` is the only correct starting point.
    let mut global_idx = program.globals.len();

    // First sub-pass: assign slots to all functions across all files.
    // Intrinsic functions are skipped: they are lowered to StatementKind::Intrinsic
    // at call sites and never appear as callable objects in the globals pool.
    // Including them here would create a mismatch between Pass-1 indices and the
    // actual program.globals array built in Pass 4 (which also skips intrinsics).
    for file in files {
        for &func_loc in file_functions(db, *file) {
            // Required interface methods are signature-only items: nothing
            // to compile or index (mirrors their pre-item invisibility here).
            if baml_compiler2_ppir::item_data::is_required_interface_method(db, func_loc) {
                continue;
            }
            // Skip intrinsic and await-any functions — they are never called via
            // a Call instruction (intrinsics lower to StatementKind::Intrinsic;
            // `__await_any` lowers to a Terminator::AwaitAny). Pass 4 skips them
            // too, so they must be skipped here as well or the Pass-1 indices
            // desync from the program.globals array (off-by-one for everything
            // after the skipped function).
            //
            // The kind is read from the span-free `function_body` firewall query
            // rather than from `lower_function(..).kind`: `lower_function` copies
            // the body's `Builtin(kind)` into `MirFunctionKind::Builtin(kind)`
            // verbatim, and fully MIR-lowering every function here just to inspect
            // that one field would double the total lowering work (Pass 4 lowers
            // every function again).
            if matches!(
                function_body(db, func_loc).as_ref(),
                baml_compiler2_hir::body::FunctionBody::Builtin(
                    BuiltinKind::Intrinsic | BuiltinKind::AwaitAny
                )
            ) {
                continue;
            }
            let fq_name = def_to_item_ref(db, Definition::Function(func_loc)).to_string();
            globals.entry(fq_name).or_insert_with(|| {
                let idx = global_idx;
                global_idx += 1;
                idx
            });
        }
    }

    // Second sub-pass: assign slots to all let bindings across all files,
    // after all function slots have been reserved.
    for file in files {
        for &let_loc in file_lets(db, *file) {
            let fq_name = def_to_item_ref(db, Definition::Let(let_loc)).to_string();
            globals.entry(fq_name).or_insert_with(|| {
                let idx = global_idx;
                global_idx += 1;
                idx
            });
        }
    }

    // Stage 6 (`SkipClean`): clean files are not lowered, so their function slots
    // would never be pushed by Pass 4. Pre-size the globals array to the full
    // Pass-1 count so a dirty function can be *written* at its whole-project slot
    // (clean/let slots stay `Null` holes). The decomposition reverses operand
    // slots to names through these holes; the array values themselves are
    // intermediate (the final image is produced by `link`).
    if skip_clean.is_some() && program.globals.len() < global_idx {
        program.globals.resize(global_idx, ConstValue::Null);
    }

    // The per-package program structure the loader builds `Object::Package` +
    // `vm.packages` from. Accumulated across passes 2/3/3b (classes, enums,
    // interfaces) and `build_packages` (impl rules); interface object indices are
    // tracked alongside so impl rules can point at them by index.

    // --- Pass 2: Build classes table ---
    // Maps fully-qualified class name -> (field name -> field index).
    // Also builds class_object_indices: class fq_name -> object index in program.objects.

    for file in files {
        let pkg_info = file_package(db, *file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let _pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        let cache = &alias_caches[&pkg_info.package];
        for &class_loc in file_classes(db, *file) {
            let class = class_data(db, class_loc);
            let store = &class.type_refs;
            // Build the fully-qualified name ("user.MyClass" / "baml.ns.MyClass")
            // through the SAME renderer MIR uses in `class_type_tags_for_project`
            // (`QualifiedTypeName::render_dotted(false)`). The class type-tag is
            // derived from this string, so emit and MIR MUST produce
            // byte-identical output or `Switch`/`JumpTable` dispatch silently
            // mismatches; sharing the one renderer is what pins them together.
            let fq_name = baml_type::QualifiedTypeName::new(
                pkg_info.package.clone(),
                pkg_info.namespace_path.clone(),
                class.name.clone(),
            )
            .render_dotted(false);

            let mut field_indices = HashMap::new();
            let mut fields = Vec::new();
            // Class-level generic params, used to resolve `T`-references in
            // field type expressions to `TyTemplate::TypeArgRef(N)`.  When
            // empty, `tir2_to_template` produces a `Concrete`-equivalent leaf
            // for every leaf and `field_template == Concrete(field_type)`.
            let class_generic_params =
                baml_compiler2_hir_ty::lower::class_generic_frame(db, class_loc);
            // BEP-044: collect only the class's actual runtime fields.
            // Interface fields are typed views over class storage, and the
            // validator enforces/link-checks them before emit.
            let merged_fields =
                collect_class_fields_with_implements(&pkg_info.namespace_path, class);
            for (idx, (name, type_ref, attrs, docstring, _gen_params, _ns)) in
                merged_fields.iter().enumerate()
            {
                field_indices.insert(name.clone(), idx);
                let (field_type, field_template) = {
                    let id = type_ref;
                    {
                        // Pass `class_generic_params` as the binding context so
                        // `T`-references inside `class Container<T> { item: T }`
                        // lower to `Tir2Ty::TypeVar("T")` rather than
                        // `Tir2Ty::Error`.  This is the input both to the
                        // erased-`Ty` (TypeVar→Unknown) used by codegen and to
                        // the `TyTemplate` (TypeVar→TypeArgRef(N)) used by
                        // typed runtime walking.
                        let tir_ty = baml_compiler2_hir_ty::lower::reject_holes(
                            &baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, *file)
                                .with_frame(class_generic_params.clone())
                                .lower_type_ref(store, *id),
                        );
                        let resolved_ty = cache.convert(&tir_ty);
                        let template =
                            bex_vm_types::anchor_template(&baml_compiler2_mir::tir2_to_template(
                                &tir_ty,
                                cache,
                                &class_generic_params,
                            ));
                        (resolved_ty, template)
                    }
                };
                let meta = extract_schema_attrs(attrs.as_slice(), docstring.as_deref());
                fields.push(ClassField {
                    name: name.clone(),
                    field_type: bex_vm_types::anchor_runtime_ty(&field_type),
                    field_template,
                    description: meta.description,
                    alias: meta.alias,
                    docstring: meta.docstring,
                    other: meta.other,
                    skip: meta.skip,
                    runtime_type: None,
                });
            }

            let class_meta = extract_schema_attrs(&class.attributes, class.docstring.as_deref());

            let type_tag = claim_type_tag(type_tags, &fq_name)?;

            // BEP-042: does this class define a magic `cleanup(self) -> void`
            // finalizer? This MUST stay in lockstep with the canonical
            // `cleanup_guard::has_cleanup_shape` (which validates the AST and
            // emits E0144): same shape — one `self` param with no default, no
            // generics, `-> void` return, and no propagating `throws` — on the lowered
            // HIR `Function`. The `throws` part reuses the shared helper; the rest
            // is mirrored (the two share field types but not the struct).
            //
            // Only DIRECT class methods count. `class.methods` is flattened to
            // include `implements`-block methods (which have a
            // `method_interface_target`), but those are interface members: the AST
            // guard injector and the `{class_fqn}.cleanup` GC resolution only
            // cover direct methods, so an `implements`-block `cleanup` must NOT
            // mark the class finalizable (it would set the flag for a method the
            // GC can neither guard nor resolve).
            //
            // The shape mirrors `cleanup_guard::has_cleanup_shape` over the
            // span-free `FunctionData`: `throws` is effectively-none when absent or
            // `never`, and the return must be `void` — read off the function's own
            // `TypeRefStore`.
            let has_cleanup = class.methods.iter().any(|&method| {
                use baml_compiler2_hir::type_ref::TypeRefKind;
                if method_interface_target(db, method).is_some() {
                    return false;
                }
                let func = function_data(db, method);
                let throws_effectively_none = func
                    .throws
                    .is_none_or(|id| matches!(func.type_refs.get(id).kind, TypeRefKind::Never));
                let returns_void = func
                    .return_type
                    .is_some_and(|id| matches!(func.type_refs.get(id).kind, TypeRefKind::Void));
                func.name.as_str() == baml_compiler2_ast::cleanup_guard::CLEANUP_METHOD
                    && func.generic_params.is_empty()
                    && func.params.len() == 1
                    && func.params[0].name.as_str() == "self"
                    && !func.params[0].has_default
                    && throws_effectively_none
                    && returns_void
            });

            let class_obj_idx = program.add_object(Object::Class(Box::new(Class {
                name: bex_vm_types::DeclarationName::Declared(fq_to_type_name(&fq_name)),
                fields,
                description: class_meta.description,
                alias: class_meta.alias,
                docstring: class_meta.docstring,
                other: class_meta.other,
                type_tag,
                ty_attr: TyAttr::default(),
                has_cleanup,
                generic_param_count: class.generic_params.len(),
                owner: bex_vm_types::HeapPtr::null(),
            })));
            // Register with fully-qualified name for inter-package lookups.
            class_object_indices.insert(fq_name.clone(), class_obj_idx);
            program_packages
                .entry(pkg_info.package.clone())
                .or_default()
                .classes
                .insert(
                    bex_vm_types::types::LocalName {
                        namespace: pkg_info.namespace_path.clone(),
                        name: class.name.clone(),
                    },
                    ObjectIndex::from_raw(class_obj_idx),
                );
            classes.insert(fq_name.clone(), field_indices);
            // MIR TypeName display for user-defined classes omits the `user.`
            // package prefix in diagnostics/snapshots. Register the same key
            // so emit-time type checks can do a direct display-name lookup.
            let display_name = if pkg_info.package.as_str() == "user" {
                if pkg_info.namespace_path.is_empty() {
                    class.name.to_string()
                } else {
                    let ns: Vec<&str> = pkg_info
                        .namespace_path
                        .iter()
                        .map(baml_base::Name::as_str)
                        .collect();
                    format!("{}.{}", ns.join("."), class.name)
                }
            } else {
                fq_name.clone()
            };
            class_object_indices
                .entry(display_name.clone())
                .or_insert(class_obj_idx);
            // Also register with the short (unqualified) class name so that MIR aggregates,
            // which store only the local name (e.g., "Point" not "user.Point"), can find it.
            let short_name = class.name.to_string();
            class_object_indices
                .entry(short_name.clone())
                .or_insert(class_obj_idx);
            // The display- and short-name maps must agree with the emitted
            // runtime field indices used by the Class object above. Use a
            // closure that rebuilds the same ordering.
            let rebuild_indices = || {
                let merged = collect_class_fields_with_implements(&pkg_info.namespace_path, class);
                let mut m = HashMap::new();
                for (idx, (name, _, _, _, _, _)) in merged.iter().enumerate() {
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

    for file in files {
        let pkg_info = file_package(db, *file);
        for &enum_loc in file_enums(db, *file) {
            let enm = enum_data(db, enum_loc);
            // Same single renderer as the class pass / MIR (see above): keep the
            // fully-qualified name construction identical everywhere so the two
            // never drift.
            let fq_name = baml_type::QualifiedTypeName::new(
                pkg_info.package.clone(),
                pkg_info.namespace_path.clone(),
                enm.name.clone(),
            )
            .render_dotted(false);

            let mut variant_map = HashMap::new();
            let mut variants = Vec::new();
            for (idx, variant) in enm.variants.iter().enumerate() {
                let meta = extract_schema_attrs(&variant.attributes, variant.docstring.as_deref());
                variant_map.insert(variant.name.to_string(), idx);
                variants.push(EnumVariant {
                    name: variant.name.to_string(),
                    description: meta.description,
                    alias: meta.alias,
                    docstring: meta.docstring,
                    other: meta.other,
                    skip: meta.skip,
                });
            }

            let enum_meta = extract_schema_attrs(&enm.attributes, enm.docstring.as_deref());

            let enum_obj_idx = program.add_object(Object::Enum(Box::new(Enum {
                name: bex_vm_types::DeclarationName::Declared(fq_to_type_name(&fq_name)),
                type_tag: claim_type_tag(type_tags, &fq_name)?,
                variants,
                description: enum_meta.description,
                alias: enum_meta.alias,
                docstring: enum_meta.docstring,
                other: enum_meta.other,
                ty_attr: TyAttr::default(),
                owner: bex_vm_types::HeapPtr::null(),
            })));
            enum_object_indices.insert(fq_name.clone(), enum_obj_idx);
            program_packages
                .entry(pkg_info.package.clone())
                .or_default()
                .enums
                .insert(
                    bex_vm_types::types::LocalName {
                        namespace: pkg_info.namespace_path.clone(),
                        name: enm.name.clone(),
                    },
                    ObjectIndex::from_raw(enum_obj_idx),
                );
            enum_variants.insert(fq_name, variant_map);
        }
    }

    // --- Pass 3b: Build interface objects + start the per-package structure ---
    // Each interface becomes an `Object::Interface` so impl rules can point at it
    // (`interface_head`) and packages can reference it by index. The full signature
    // (args/requires/assoc/fields/methods) is filled by `build_interface_def`.
    // `program_packages` is the per-package structure the loader builds
    // `Object::Package` + `vm.packages` from; `build_packages` fills in each
    // package's impl rules below.
    for file in files {
        let pkg_info = file_package(db, *file);
        let resolved = &alias_caches[&pkg_info.package];
        for &iface_loc in baml_compiler2_ppir::item_data::file_interfaces(db, *file) {
            let iface_data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
            let iface_tn = baml_compiler2_hir_ty::lower::qualify_def(
                db,
                Definition::Interface(iface_loc),
                &iface_data.name,
            );
            // Same single renderer as the class and enum passes, so a head's
            // identity does not depend on which kind of declaration produced it.
            let iface_tag = claim_type_tag(type_tags, &iface_tn.render_dotted(false))?;
            let iface_def =
                build_interface_def(db, iface_loc, iface_tn.clone(), iface_tag, resolved);
            let iface_obj_idx = program.add_object(Object::Interface(Box::new(iface_def)));
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

    // Read-only snapshot of pooled class field metadata for function-body
    // codegen, built after Pass 3b so it covers every alias registered in
    // `class_object_indices` (including classes minted by earlier file
    // groups). See [`ClassFieldSnapshot`].
    let class_fields: ClassFieldSnapshot = class_object_indices
        .iter()
        .filter_map(|(_, &idx)| match program.objects.get(idx) {
            Some(Object::Class(class)) => Some((
                class.type_tag,
                class
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), f.field_type.clone()))
                    .collect(),
            )),
            _ => None,
        })
        .collect();

    // --- Pass 3c: Recursive type alias definitions ---
    // Each recursive alias becomes an `Object::TypeAlias` so a package can
    // reference it by index (non-recursive aliases are expanded inline at
    // lowering and never reach here). Emitted with the other declarations rather
    // than after the code pass: the pool is group-major/pass-major, so an object
    // appended later would fall outside every bucket the linker reproduces.
    let mut emitted_aliases = HashSet::new();
    for file in files {
        let pkg_info = file_package(db, *file);
        let cache = &alias_caches[&pkg_info.package];
        for &alias_loc in baml_compiler2_ppir::item_data::file_type_aliases(db, *file) {
            let alias_data = baml_compiler2_ppir::item_data::type_alias_data(db, alias_loc);
            let qtn = baml_compiler2_hir_ty::lower::qualify_def(
                db,
                Definition::TypeAlias(alias_loc),
                &alias_data.name,
            );
            // A package's alias cache also carries its dependencies' aliases
            // (`resolved_aliases_for_package` unions them in), so iterating the
            // cache would re-emit an imported alias under every importer. Walking
            // declarations instead emits each alias exactly once, in the file that
            // declares it — which is also what per-file dirty tracking needs.
            if !cache.recursive.contains(&qtn) || !emitted_aliases.insert(qtn.clone()) {
                continue;
            }
            let tir_ty = &cache.aliases[&qtn];
            let mir_ty = cache.convert(tir_ty);
            // Aliases have no type-parameter list, so nothing is in scope for the
            // right-hand side to reference — a non-realized alias body means
            // lowering produced something impossible, not a program to carry.
            let definition = baml_type::RealizedTy::try_from(&mir_ty).map_err(|e| {
                LoweringError::Internal(format!(
                    "type alias `{}` lowered to a non-realized type (`{}`); aliases \
                     cannot be generic, so this is a compiler bug",
                    qtn.render_dotted(false),
                    e.variant,
                ))
            })?;
            let fq_name = qtn.render_dotted(false);
            let obj_idx = program.add_object(Object::TypeAlias(Box::new(
                bex_vm_types::types::TypeAliasDef {
                    name: qtn.clone(),
                    type_tag: claim_type_tag(type_tags, &fq_name)?,
                    definition: bex_vm_types::anchor_realized(&definition),
                    owner: bex_vm_types::HeapPtr::null(),
                },
            )));
            program_packages
                .entry(qtn.package().clone())
                .or_default()
                .type_aliases
                .insert(
                    bex_vm_types::types::LocalName {
                        namespace: qtn.namespace().clone(),
                        name: qtn.name().clone(),
                    },
                    ObjectIndex::from_raw(obj_idx),
                );
        }
    }

    // --- Pass 4: Compile each function ---
    if rayon::current_num_threads() > 1 {
        // Default: compile function bodies across rayon workers. Byte-identical
        // to the serial pass — see `emit_functions_parallel` for the
        // fragment/watermark design. A single-threaded pool (RAYON_NUM_THREADS=1,
        // or a 1-thread `ThreadPool::install`, as the emit-determinism test uses)
        // takes the serial path, which is also the reference implementation.
        emit_functions_parallel(
            db,
            files,
            skip_clean,
            globals,
            classes,
            class_object_indices,
            enum_object_indices,
            enum_variants,
            &class_fields,
            alias_caches,
            program,
            opt,
        );
    } else {
        emit_functions_serial(
            db,
            files,
            skip_clean,
            globals,
            classes,
            class_object_indices,
            enum_object_indices,
            enum_variants,
            &class_fields,
            alias_caches,
            program,
            opt,
        );
    }

    // Stage 6 (`SkipClean`) / design §9 R2: the `$init` / `$init_test` tail is a
    // whole-*group* synthesis. Rather than reuse the previous compile's tail
    // verbatim (unsound when a dirty file is tail-producing), we synthesize it
    // fresh from the entire project's `let`s / `test` blocks. Register clean
    // files' function/let *slots* first so the tail passes see the whole project;
    // clean function *object* placeholders are injected after the tail (below).
    // Clean `let` initializers are re-lowered here (their MIR is salsa-cached);
    // this does not count as a lowered *file* (`record_lowered_file` is Pass-4
    // only) and is byte-identical to a full compile's tail.
    if let Some(clean) = skip_clean {
        inject_clean_slots(db, files, clean, globals, program);
    }

    // --- Pass 4.5: Populate let-binding global slots and synthesize $init ---
    // Collect all let bindings grouped by package.
    {
        let mut pkg_lets: HashMap<String, Vec<(String, LetLoc, baml_base::SourceFile)>> =
            HashMap::new();
        for file in files {
            let pkg_info = file_package(db, *file);
            for &let_loc in file_lets(db, *file) {
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
                globals,
                classes,
                class_object_indices,
                enum_object_indices,
                enum_variants,
                &class_fields,
                &mut *program,
                opt,
            )?;

            // The local (workspace) package's chainer is unprefixed — the
            // same `Package::Local` classification the type layer uses.
            let is_local_pkg = matches!(
                baml_type::Package::from_name(baml_base::Name::new(pkg_name.as_str())),
                baml_type::Package::Local
            );
            let init_fq_name = if is_local_pkg {
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

        program.package_init_order.extend(package_init_order);
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

        for file in files {
            let pkg_info = file_package(db, *file);
            for &func_loc in file_functions(db, *file) {
                // Required interface methods are signature-only items: nothing
                // to compile or index (mirrors their pre-item invisibility here).
                if baml_compiler2_ppir::item_data::is_required_interface_method(db, func_loc) {
                    continue;
                }
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

        // Deterministic package order — pkg_init_tests is a HashMap, and the
        // chainers allocate objects/globals, so iteration order is emitted
        // layout. (Latent nondeterminism before the group split: only one
        // package ever carried $init_test functions.)
        let mut chainer_pkgs: Vec<&String> = pkg_init_tests.keys().collect();
        chainer_pkgs.sort();
        for pkg_name in chainer_pkgs {
            let init_test_fns = &pkg_init_tests[pkg_name];
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
                docstring: None,
                declared_name: None,
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
                return_type: bex_vm_types::TyTemplate::Null {
                    attr: baml_type::TyAttr::default(),
                },
                param_names: vec!["registry".to_string()],
                param_types: vec![bex_vm_types::TyTemplate::Unknown {
                    attr: baml_type::TyAttr::default(),
                }], // type not needed for chainer dispatch
                param_has_default: vec![false],
                display_type_params: Vec::new(),
                generic_param_bounds: Vec::new(),
                display_param_types: vec!["unknown".to_string()],
                display_return_type: "null".to_string(),
                throws_type: bex_vm_types::TyTemplate::Never {
                    attr: baml_type::TyAttr::default(),
                },
                origin: FunctionOrigin::Internal,
                body_meta: None,
                capture: FunctionCaptureProps::disabled(),
                function_id: 0, // assigned at engine init (interim provider)
                runtime_package: bex_vm_types::HeapPtr::null(),
            };

            // The local (workspace) package's chainer is unprefixed — the
            // same `Package::Local` classification the type layer uses.
            let is_local_pkg = matches!(
                baml_type::Package::from_name(baml_base::Name::new(pkg_name.as_str())),
                baml_type::Package::Local
            );
            let chainer_name = if is_local_pkg {
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

    // Stage 6 (`SkipClean`) phase 2: the real pool is now complete (dirty
    // functions + the synthesized `$init`/`$init_test` tail), so register clean
    // functions' object-index placeholders past the end for name reversal.
    if let Some(clean) = skip_clean {
        inject_clean_object_placeholders(db, files, clean, globals, program);
    }

    Ok(())
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
    frame_params: &[baml_type::ParamTy],
) -> baml_type::TyTemplate {
    // An empty throw set is `never` — the empty error set — not an absent one.
    let never = || baml_type::TyTemplate::Never {
        attr: baml_type::TyAttr::default(),
    };
    let pkg_info = file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package);
    let throw_sets = baml_compiler2_hir_ty::package_interface::function_throw_sets(db, pkg_id);

    let key = baml_compiler2_hir_ty::package_interface::throw_set_key(
        &pkg_info.namespace_path,
        func_name,
    );

    let Some(facts) = throw_sets.transitive_for(&key) else {
        return never();
    };
    if facts.is_empty() {
        return never();
    }

    let converted: Vec<baml_type::TyTemplate> = facts
        .iter()
        .map(|tir_ty| baml_compiler2_mir::tir2_to_template(tir_ty, cache, frame_params))
        .collect();

    if converted.len() == 1 {
        converted.into_iter().next().unwrap()
    } else {
        baml_type::TyTemplate::Union(converted.into(), baml_type::TyAttr::default())
    }
}

/// Stamp signature metadata onto a compiled `Function` — the single writer
/// for both top-level declarations (metadata built by
/// `compute_function_metadata`) and lambdas (metadata recorded
/// by MIR's `lower_lambda` on `MirFunction::signature`).
fn apply_signature_metadata(f: &mut Function, sig: &baml_compiler2_mir::RuntimeSignature) {
    f.param_names.clone_from(&sig.param_names);
    f.param_types = sig
        .param_types
        .iter()
        .map(bex_vm_types::anchor_template)
        .collect();
    f.param_has_default.clone_from(&sig.param_has_default);
    f.return_type = bex_vm_types::anchor_template(&sig.return_type);
    f.throws_type = bex_vm_types::anchor_template(&sig.throws_type);
    f.docstring.clone_from(&sig.docstring);
    f.declared_name.clone_from(&sig.name);
    f.display_type_params.clone_from(&sig.display_type_params);
    f.generic_param_bounds = sig
        .generic_param_bounds
        .iter()
        .map(|bounds| {
            bounds
                .iter()
                .map(|bound| bex_vm_types::types::InterfaceBound {
                    interface: bex_vm_types::TypeHead::of_name(&bound.interface),
                    args: bound
                        .args
                        .iter()
                        .map(bex_vm_types::anchor_template)
                        .collect(),
                    assoc: bound
                        .assoc
                        .iter()
                        .map(|(name, ty)| (name.clone(), bex_vm_types::anchor_template(ty)))
                        .collect(),
                })
                .collect()
        })
        .collect();
    f.display_param_types.clone_from(&sig.display_param_types);
    f.display_return_type.clone_from(&sig.display_return_type);
}

/// A bare single-segment path type expression for `name`.
fn type_expr_for_name(name: Name) -> TypeExpr {
    baml_compiler2_ast::TypeExprKind::Path {
        segments: vec![name],
        generic_args: Vec::new(),
        associated_type_bindings: Vec::new(),
        attrs: Vec::new(),
    }
    .at(baml_compiler2_ast::TextRange::default())
}

/// `Name<P0, P1, …>` — the declaring item applied to its own parameters as type
/// variables. Only the parameter *names* matter; bounds are irrelevant to the
/// synthesized path.
fn type_expr_for_name_with_generic_args(
    name: Name,
    generic_params: &[GenericParamData],
) -> TypeExpr {
    baml_compiler2_ast::TypeExprKind::Path {
        segments: vec![name],
        generic_args: generic_params
            .iter()
            .map(|param| type_expr_for_name(param.name.clone()))
            .collect(),
        associated_type_bindings: Vec::new(),
        attrs: Vec::new(),
    }
    .at(baml_compiler2_ast::TextRange::default())
}

/// Extract runtime and display signature metadata for a function, off the
/// span-free firewall (`function_data` + the enclosing owner's `*_data`).
///
/// Type resolution delegates to TIR's `lower_type_ref` (single source of truth)
/// then converts via MIR's `convert_tir_ty_for_runtime` to produce runtime `baml_type::RuntimeTy`.
/// The display fields keep generic type variables and unresolved projections
/// intact for self-documenting surfaces like `baml run --list`.
fn compute_function_metadata<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    parameter_defaults: &baml_compiler2_hir::signature::FunctionParameterDefaults,
    cache: &ResolvedAliases,
) -> baml_compiler2_mir::RuntimeSignature {
    use baml_compiler2_hir::type_ref::{TypeRefId, TypeRefStore};
    use baml_compiler2_hir_ty::diagnostics::TirTypeError;
    use baml_compiler2_ppir::item_data::{ImplSubjectData, MethodOwner, method_owner};
    use baml_type::{Ty, unify::substitute_ty};

    /// One in-scope type variable's declared bound conjunction, as `(store, id)`
    /// refs into whichever arena declared it — enclosing (class/interface/impl)
    /// and function bounds live in different arenas. Empty when unbounded.
    type BoundRef<'a> = Vec<(&'a TypeRefStore, TypeRefId)>;

    /// A declaration's parameters split into parallel name and bound-ref lists,
    /// keeping every `&`-separated conjunct.
    fn split_declared<'a>(
        params: &'a [GenericParamData],
        store: &'a TypeRefStore,
    ) -> (Vec<Name>, Vec<BoundRef<'a>>) {
        params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    param.bounds.iter().map(|&id| (store, id)).collect(),
                )
            })
            .unzip()
    }

    let file = func_loc.file(db);
    let func = function_data(db, func_loc);
    // The arena holding this function's own signature type refs (params, return).
    let func_store = &func.type_refs;

    let param_names: Vec<String> = func.params.iter().map(|p| p.name.to_string()).collect();
    let param_has_default: Vec<bool> = parameter_defaults
        .params
        .iter()
        .map(Option::is_some)
        .collect();

    let pkg_info = file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package);
    let _pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    // The item this method belongs to, via the firewall (mirrors MIR's enclosing
    // lookups; replaces the removed `method_owners`/`implements_for` flat fields).
    // Each `*_data` result carries its own `TypeRefStore` for the refs read below.
    let owner = method_owner(db, func_loc);
    let enclosing_free_impl = match owner {
        Some(MethodOwner::FreeImpl(impl_loc)) => Some(impl_block_data(db, impl_loc)),
        _ => None,
    };
    let enclosing_class = match owner {
        Some(MethodOwner::Class(class_loc)) => Some(class_data(db, class_loc)),
        _ => None,
    };
    let enclosing_interface_loc = match owner {
        Some(MethodOwner::Interface(iface_loc)) => Some(iface_loc),
        _ => None,
    };
    let enclosing_interface = enclosing_interface_loc.map(|loc| interface_data(db, loc));

    // For methods on generic classes/interfaces/impls, the enclosing generic
    // params are in scope inside the method signature. Mirror
    // `MirLowerer::enclosing_generic_params`: enclosing params come first, then
    // function-level params.
    let (scoped_generic_param_names, scoped_generic_bound_refs): (Vec<Name>, Vec<BoundRef>) = {
        let (mut names, mut bounds) = if let Some(block) = enclosing_free_impl {
            match &block.subject {
                ImplSubjectData::Free { generics, .. } => {
                    split_declared(generics, &block.type_refs)
                }
                ImplSubjectData::InClass { .. } => (Vec::new(), Vec::new()),
            }
        } else if let Some(iface) = enclosing_interface {
            split_declared(&iface.generic_params, &iface.type_refs)
        } else {
            enclosing_class
                .map(|c| split_declared(&c.generic_params, &c.type_refs))
                .unwrap_or_default()
        };
        let (own_names, own_bounds) = split_declared(&func.generic_params, func_store);
        names.extend(own_names);
        bounds.extend(own_bounds);
        (names, bounds)
    };
    let enclosing_generics = baml_compiler2_hir_ty::lower::function_generic_frame(db, func_loc);

    // Every type variable in scope for this signature, with its interface
    // bounds - the one shared param env (`function_generic_bounds` covers
    // the enclosing class/interface/free-impl plus the function's own
    // params; the interface arm carries `Self`'s own bound and the
    // associated slots). Threaded into every lowering below so a
    // `T.member` projection resolves through `T`'s declared bound DURING
    // lowering.
    let scope_bounds = baml_compiler2_hir_ty::lower::function_generic_bounds(db, func_loc);

    // A method declared inside an interface resolves its associated types
    // (`Item`/`Error`) and `Self` against the rigid `Self` type variable, the same
    // way the method body does. Binding each associated-type name to the projection
    // `Self.<name>` (and `Self` to its rigid type variable) keeps a bare `Item` in a
    // signature as a faithful `Self.Item` projection rather than an unresolved type.
    // Empty for non-interface methods (the `receiver_ty` path below handles
    // class/impl receivers). Only the interface's own associated types are bound;
    // names inherited through `requires` are not (that ambiguity-aware resolution
    // lived in the removed `interface_self_projection_bindings`).
    let self_param = enclosing_generics
        .iter()
        .find(|param| param.as_str() == "Self")
        .cloned();
    let self_var = || {
        Ty::TypeVar(
            self_param
                .clone()
                .expect("interface method environment contains Self"),
            baml_type::TyAttr::default(),
        )
    };
    // The declaring interface as a plain constraint (its qualified name at
    // its own rigid params) - the qualifier each `Self.<assoc>` projection
    // below is built against.
    let self_declaring_interface: Option<baml_type::Interface> = enclosing_interface_loc
        .zip(enclosing_interface)
        .map(|(loc, iface)| {
            let qtn = baml_compiler2_hir_ty::lower::qualify_def(
                db,
                baml_compiler2_hir::contributions::Definition::Interface(loc),
                &iface.name,
            );
            let args = iface
                .generic_params
                .iter()
                .map(|declared| {
                    let param = enclosing_generics
                        .iter()
                        .find(|param| param.name() == &declared.name)
                        .expect("interface generic parameter is in the function environment");
                    Ty::TypeVar(param.clone(), baml_type::TyAttr::default())
                })
                .collect();
            baml_type::Interface::new(qtn, args, Box::new([]))
        });
    let interface_signature_bindings: rustc_hash::FxHashMap<ParamTy, Ty> = match enclosing_interface
    {
        Some(iface) => {
            let mut bindings: rustc_hash::FxHashMap<ParamTy, Ty> = enclosing_generics
                .iter()
                .map(|p| {
                    (
                        p.clone(),
                        Ty::TypeVar(p.clone(), baml_type::TyAttr::default()),
                    )
                })
                .collect();
            bindings.insert(
                self_param
                    .clone()
                    .expect("interface method environment contains Self"),
                self_var(),
            );
            for assoc in &iface.associated_types {
                let assoc_param = enclosing_generics
                    .iter()
                    .find(|param| param.name() == &assoc.name)
                    .expect("associated type is in the function environment");
                bindings.insert(
                    assoc_param.clone(),
                    Ty::AssociatedTypeProjection {
                        base: Box::new(self_var()),
                        // The declaring interface resolved above; we are in the
                        // `Some(iface)` arm, so it is present.
                        interface: Box::new(self_declaring_interface.clone().unwrap_or_else(
                            || unreachable!("interface method has a declaring interface"),
                        )),
                        member: assoc.name.clone(),
                        attr: baml_type::TyAttr::default(),
                    },
                );
            }
            bindings
        }
        None => rustc_hash::FxHashMap::default(),
    };
    // The interface branch's generic-param scope: every name it binds (`Self`, the
    // interface's params, its associated types), so a bare `Item` lowers to
    // `TypeVar(Item)` before substitution.
    let interface_binding_params = enclosing_generics.clone();

    // The concrete receiver (`ClassName<T,…>` for a class method, or a free-impl
    // `for` target), lowered once. A non-interface method's `Self` / `Self.Assoc`
    // then root at it through the `self_ty` channel — the same projection path the
    // interface branch drives with its rigid `Self` type variable. `None` for a
    // free function (no receiver in scope) and for interface methods (which bind
    // `Self` via `interface_signature_bindings`). The class/interface receiver is a
    // synthetic `Name<params>` path (no item-tree read); the free-impl receiver is
    // its `for_target` ref, lowered from the impl block's arena.
    let receiver_ty: Option<Ty> = if enclosing_interface.is_some() {
        None
    } else {
        // `owner_self_ty` resolves both the class receiver (with the
        // builtin-container sugar) and a free impl's `for` target.
        baml_compiler2_hir_ty::lower::owner_self_ty(db, func_loc, &enclosing_generics)
    };

    // Lower a signature type ref (in `store`) against this method's scope. For an
    // interface method the associated-type / `Self` bindings are applied by lowering
    // with their names in scope (so `Item` lowers to `TypeVar(Item)`) and then
    // substituting; for every other method `Self` is bound to the concrete
    // `receiver_ty` via the `self_ty` channel. In both cases `scope_bounds` drives
    // projection resolution. Namespace-relative resolution (e.g. `MyLorem` in a
    // signature under `ns_lorem/`) uses the file's namespace so a non-root-ns class
    // does not erase to `unknown`.
    let scoped_ctx = || {
        let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file)
            .with_bounds(scope_bounds.clone());
        if enclosing_interface.is_some() {
            ctx.with_frame(interface_binding_params.clone())
        } else {
            ctx.with_frame(enclosing_generics.clone())
                .with_self_ty(receiver_ty.clone())
                .with_impl_target(baml_compiler2_hir_ty::lower::owner_impl_target(
                    db,
                    func_loc,
                    &enclosing_generics,
                ))
        }
    };
    let lower_scoped =
        |store: &TypeRefStore, id: TypeRefId, _diags: &mut Vec<TirTypeError>| -> Ty {
            let lowered =
                baml_compiler2_hir_ty::lower::reject_holes(&scoped_ctx().lower_type_ref(store, id));
            let realized = if enclosing_interface.is_some() {
                substitute_ty(&lowered, &interface_signature_bindings)
            } else {
                lowered
            };
            // Post-substitution normalization: a projection over a ground
            // base (`(UserRepository as Repository<Record = UserRecord>)
            // .Record`) reduces to what it IS; a rigid-var base stays
            // symbolic (`(T as BoxLike).Item`).
            baml_compiler2_hir_ty::package_interface::reduce_ground_projections(db, &realized, 8)
        };

    // Each scoped generic parameter's bound as a displayable `Ty`, kept only when it
    // lowers cleanly (a bound that fails to resolve is dropped rather than shown as
    // `unknown`). A bound is a constraint head — it pins only the associated
    // members it writes, so lowering it existentially would mint completeness
    // diagnostics for members the bound legitimately leaves free (and drop the
    // bound from display). Rendered into `display_type_params` below.
    let generic_param_bounds: HashMap<Name, Vec<Ty>> = scoped_generic_param_names
        .iter()
        .enumerate()
        .filter_map(|(idx, name)| {
            let refs = scoped_generic_bound_refs.get(idx)?;
            let frame = if enclosing_interface.is_some() {
                &interface_binding_params
            } else {
                &enclosing_generics
            };
            // Each conjunct lowers independently; one that fails to lower is
            // dropped (its declaration reported the error) without hiding the
            // rest of the conjunction.
            let bound_tys: Vec<Ty> = refs
                .iter()
                .filter_map(|&(store, id)| {
                    let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file)
                        .with_bounds(scope_bounds.clone())
                        .with_frame(frame.clone());
                    let (lowered, diagnostics) = ctx.lower_type_ref_at_with_diagnostics(
                        store,
                        id,
                        baml_compiler2_hir_ty::lower::TypePosition::ConstraintHead,
                    );
                    let lowered = baml_compiler2_hir_ty::lower::reject_holes(&lowered);
                    let clean = diagnostics.is_empty();
                    let bound_ty = if enclosing_interface.is_some() {
                        substitute_ty(&lowered, &interface_signature_bindings)
                    } else {
                        lowered
                    };
                    clean.then_some(bound_ty)
                })
                .collect();
            (!bound_tys.is_empty()).then(|| (name.clone(), bound_tys))
        })
        .collect();

    // Projection resolution is folded into `lower_scoped` (via `scope_bounds`), so
    // no separate resolve pass is needed. Diagnostics are discarded — the display
    // fields keep whatever resolved.
    let resolve_display_tir = |store: &TypeRefStore, id: TypeRefId| -> Ty {
        let mut diags = Vec::new();
        lower_scoped(store, id, &mut diags)
    };

    let display_type_params: Vec<String> = scoped_generic_param_names
        .iter()
        .map(|name| match generic_param_bounds.get(name) {
            Some(bounds) => {
                let rendered = bounds
                    .iter()
                    .map(Ty::render_user_facing)
                    .collect::<Vec<_>>()
                    .join(" & ");
                format!("{} extends {rendered}", name.as_str())
            }
            None => name.to_string(),
        })
        .collect();

    // The receiver type used for a `self` parameter with no written annotation. For
    // an interface method it is the interface at its own params, resolved through
    // the same binding path (a synthetic `Name<params>` path — no item-tree read);
    // for every other method it is the already-lowered `receiver_ty`. The interface
    // view is `Self`'s *bound*, so it lowers as a constraint head: associated
    // members stay unpinned (they realize per-receiver — a default is not a pin),
    // rather than demanding the existential's completeness.
    let self_param_ty = || -> Option<Ty> {
        match enclosing_interface {
            Some(iface) => {
                let te =
                    type_expr_for_name_with_generic_args(iface.name.clone(), &iface.generic_params);
                let mut builder = baml_compiler2_hir::type_ref::TypeRefBuilder::new();
                let id = builder.lower(&te);
                let (store, _spans) = builder.finish();
                let lowered = baml_compiler2_hir_ty::lower::reject_holes(
                    &baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file)
                        .with_bounds(scope_bounds.clone())
                        .with_frame(interface_binding_params.clone())
                        .lower_type_ref_at(
                            &store,
                            id,
                            baml_compiler2_hir_ty::lower::TypePosition::ConstraintHead,
                        ),
                );
                Some(substitute_ty(&lowered, &interface_signature_bindings))
            }
            None => receiver_ty.clone(),
        }
    };

    // The signature is templated over this function's own callee frame, so a
    // *value* of it reconstructs precisely by substituting the realized args it
    // carries. `frame_generic_params` is the same layout the body's `TypeArgRef`s
    // and the frames callers seed use — templating against any other list would
    // silently name the wrong types.
    let frame_params = baml_compiler2_hir_ty::lower::function_generic_frame(db, func_loc);
    let to_template = |tir_ty: &Ty| {
        // Metadata shows what the type IS: a projection over a ground base
        // (`(UserRepository as Repository<Record = UserRecord>).Record`)
        // reduces through the oracle; a rigid-var base stays symbolic
        // (`(T as BoxLike).Item`), rendered as written.
        let tir_ty =
            &baml_compiler2_hir_ty::package_interface::reduce_ground_projections(db, tir_ty, 8);
        baml_compiler2_mir::tir2_to_template(tir_ty, cache, &frame_params)
    };
    let runtime_generic_param_bounds = frame_params
        .iter()
        .map(|param| {
            scope_bounds
                .get(param)
                .into_iter()
                .flatten()
                .map(|bound| baml_compiler2_mir::RuntimeInterfaceBound {
                    interface: bound.name.clone(),
                    args: bound.generics.iter().map(to_template).collect(),
                    assoc: bound
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (name.clone(), to_template(ty)))
                        .collect(),
                })
                .collect()
        })
        .collect();
    let null_template = || baml_type::TyTemplate::Null {
        attr: baml_type::TyAttr::default(),
    };

    let mut param_types = Vec::with_capacity(func.params.len());
    let mut display_param_types = Vec::with_capacity(func.params.len());
    for param in &func.params {
        let resolved = if let Some(id) = param.type_ref {
            Some(resolve_display_tir(func_store, id))
        } else if param.name.as_str() == "self" {
            self_param_ty()
        } else {
            None
        };
        if let Some(tir_ty) = resolved {
            display_param_types.push(tir_ty.render_user_facing());
            param_types.push(to_template(&tir_ty));
        } else {
            display_param_types.push("null".to_string());
            param_types.push(null_template());
        }
    }

    let (return_type, display_return_type) = if let Some(id) = func.return_type {
        let tir_ty = resolve_display_tir(func_store, id);
        (to_template(&tir_ty), tir_ty.render_user_facing())
    } else {
        (null_template(), "null".to_string())
    };

    baml_compiler2_mir::RuntimeSignature {
        param_names,
        param_types,
        param_has_default,
        return_type,
        // TIR's inferred transitive throw set — richer than the declared
        // clause (a declared clause is a firewall the inference respects).
        throws_type: compute_throws_type(db, file, &func.name, cache, &frame_params),
        docstring: func.docstring.clone(),
        name: Some(func.name.to_string()),
        display_type_params,
        generic_param_bounds: runtime_generic_param_bounds,
        display_param_types,
        display_return_type,
    }
}

/// Root-relative display path for `file` (our `-trimpath`).
///
/// `Function::source_file` is display/metadata-only (backtraces, event
/// metadata, reflection locations) — never opened from disk — so stripping
/// the root keeps serialized `Program`s location-independent (a cached or
/// packed blob is byte-identical wherever the project lives) and backtraces
/// machine-independent. Only `Workspace` roots are stripped: `Stdlib` and
/// `Dependency` roots keep their paths verbatim, because their
/// `<builtin>/…` unit paths are a wire contract
/// (`bex_vm_types/src/link.rs` string-matches the prefix).
fn relative_source_path(db: &dyn baml_compiler2_mir::Db, file: baml_base::SourceFile) -> String {
    let path = file.path(db);
    let root = file.source_root(db);
    match root.kind(db) {
        baml_base::SourceRootKind::Workspace => path
            .strip_prefix(root.path(db))
            .unwrap_or(&path)
            .display()
            .to_string(),
        baml_base::SourceRootKind::Stdlib
        | baml_base::SourceRootKind::Dependency
        | baml_base::SourceRootKind::Dynamic => path.display().to_string(),
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
    db: &dyn baml_compiler2_ppir::Db,
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

    // Kahn's algorithm cannot order the members of a dependency cycle: they
    // never reach in-degree zero, so they would silently VANISH from the
    // result — and with it from `package_init_order`, leaving their globals
    // uninitialized. The package dependency graph is acyclic by construction;
    // enforce it so a future edge cannot rot into that silent failure.
    assert_eq!(
        result.len(),
        pkg_names.len(),
        "package dependency cycle: {:?} cannot be topologically ordered",
        pkg_names
            .iter()
            .filter(|name| !result.contains(name))
            .collect::<Vec<_>>()
    );

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
    RuntimeTy::Unknown {
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

/// Pass 4, serial (the reference implementation, used on 1-thread pools):
/// lower and compile every function body,
/// file by file, straight into the program pool.
#[allow(clippy::too_many_arguments)]
fn emit_functions_serial(
    db: &dyn baml_compiler2_mir::Db,
    files: &[baml_base::SourceFile],
    skip_clean: Option<&HashSet<String>>,
    globals: &HashMap<String, usize>,
    classes: &HashMap<String, HashMap<String, usize>>,
    class_object_indices: &HashMap<String, usize>,
    enum_object_indices: &HashMap<String, usize>,
    enum_variants: &HashMap<String, HashMap<String, usize>>,
    class_fields: &ClassFieldSnapshot,
    alias_caches: &HashMap<Name, ResolvedAliases>,
    program: &mut Program,
    opt: OptLevel,
) {
    for file in files {
        let rel_path = relative_source_path(db, *file);
        // A clean file (incremental compile) is not emitted here at all — its
        // compiled unit is taken verbatim from the cached image by the caller. We
        // never lower a clean file's bodies (the core B-693 Stage 6 invariant).
        if let Some(clean) = skip_clean {
            if clean.contains(&rel_path) {
                continue;
            }
        }
        // This file's bodies are about to be MIR/bytecode-lowered — record it for
        // the Stage 6 "only dirty files are lowered" evidence counter.
        record_lowered_file(&rel_path);
        let line_starts = build_line_starts(file.text(db));
        let pkg_info_pass4 = file_package(db, *file);
        let is_builtin_file = file.path(db).to_string_lossy().starts_with("<builtin>/");
        let cache_pass4 = &alias_caches[&pkg_info_pass4.package];
        for &func_loc in file_functions(db, *file) {
            // Required interface methods are signature-only items: nothing
            // to compile or index (mirrors their pre-item invisibility here).
            if baml_compiler2_ppir::item_data::is_required_interface_method(db, func_loc) {
                continue;
            }
            let mir = lower_function(db, func_loc, opt);
            let fq_name = mir.item_ref.to_string();

            let mut compiled_fn = match &mir.kind {
                MirFunctionKind::Bytecode(body) => {
                    // Compile lambda children first, collecting their ObjectPool indices.
                    let source_file = relative_source_path(db, *file);
                    let empty_capture_types = Vec::new();
                    let empty_spawn_capture_indices = HashSet::new();
                    let lambda_info = compile_lambdas_flat(
                        &mir.lambdas,
                        Some(body),
                        &empty_capture_types,
                        &empty_spawn_capture_indices,
                        &line_starts,
                        &source_file,
                        globals,
                        classes,
                        class_object_indices,
                        enum_object_indices,
                        enum_variants,
                        class_fields,
                        &mut program.objects,
                        0,
                        opt,
                    );
                    let lambda_obj_indices: Vec<usize> =
                        lambda_info.iter().map(|(idx, _)| *idx).collect();
                    let lambda_names_vec: Vec<String> =
                        lambda_info.iter().map(|(_, name)| name.clone()).collect();
                    let ctx = MirCodegenContext {
                        globals,
                        classes,
                        class_object_indices,
                        enum_object_indices,
                        enum_variants,
                        class_fields,
                        objects: &mut program.objects,
                        objects_base: 0,
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
                MirFunctionKind::Builtin(kind) => {
                    match builtin_emit_function(*kind, &fq_name, mir.arity) {
                        Some(f) => f,
                        // Intrinsics and `__await_any` have no callable body —
                        // call sites lower to `StatementKind::Intrinsic` /
                        // `Terminator::AwaitAny` directly. Skip entirely.
                        None => continue,
                    }
                }
            };

            attach_function_metadata(
                db,
                func_loc,
                cache_pass4,
                is_builtin_file,
                &fq_name,
                &mut compiled_fn,
            );
            register_compiled_function(
                program,
                globals,
                skip_clean.is_some(),
                fq_name,
                compiled_fn,
            );
        }
    }
}

/// One function's Pass-4 state carried from the lowering stage to the
/// parallel codegen stage and the serial merge stage.
struct FnWorkItem {
    file: baml_base::SourceFile,
    local_id: baml_compiler2_hir::ids::LocalItemId<baml_compiler2_hir::ids::FunctionMarker>,
    mir: baml_compiler2_mir::MirFunction,
    fq_name: String,
    /// Project-relative source path (`relative_source_path`).
    source_file: String,
    /// Line index of the item's file, shared by every function in the file.
    line_starts: std::sync::Arc<[u32]>,
    is_builtin_file: bool,
}

/// Identity of one Pass-4 function before MIR lowering: the [`FnWorkItem`]
/// fields known from enumeration alone (Stage A's input).
struct FnSeed {
    file: baml_base::SourceFile,
    local_id: baml_compiler2_hir::ids::LocalItemId<baml_compiler2_hir::ids::FunctionMarker>,
    /// Project-relative source path (`relative_source_path`).
    source_file: String,
    /// Line index of the item's file, shared by every function in the file.
    line_starts: std::sync::Arc<[u32]>,
    is_builtin_file: bool,
}

/// Lower one seed's function to MIR on the given database handle.
///
/// `FunctionLoc` is minted on the SAME handle the lowering reads from — it is
/// a `'db`-interned key, and interning is shared storage, so every handle
/// mints the identical id.
fn lower_seed(
    db: &dyn baml_compiler2_mir::Db,
    seed: &FnSeed,
    opt: OptLevel,
) -> baml_compiler2_mir::MirFunction {
    lower_function(db, FunctionLoc::new(db, seed.file, seed.local_id), opt)
}

/// Stage A driver: lower every seed's function to MIR, returned in seed order.
///
/// `lower_function` reads salsa (PPIR bodies, scope type inference) and
/// returns an owned [`baml_compiler2_mir::MirFunction`] that is a pure
/// function of the frozen inputs, and every call is independent — so when the
/// database mints worker handles ([`Db::parallel_db_handle`]) the seeds are
/// lowered across rayon workers and the reassembled output is exactly the
/// serial loop's.
///
/// The database type is `Send` but deliberately not `Sync` (each salsa handle
/// carries thread-confined query-stack state), so — exactly like
/// `baml_project`'s parallel check — one handle per chunk is cloned on THIS
/// thread and MOVED into its task; all clones share one memo table, so a
/// scope inferred by one worker is a cache hit for every other. The first
/// seed is lowered serially before fanning out to warm the file/package-level
/// memos every body read reaches. Tiny batches, single-threaded pools, and
/// databases without handles take the serial loop directly.
fn lower_seed_mirs(
    db: &dyn crate::Db,
    seeds: &[FnSeed],
    opt: OptLevel,
) -> Vec<baml_compiler2_mir::MirFunction> {
    // Small chunks keep rayon's work-stealing effective — bodies vary a lot
    // in inference cost — while amortizing the per-task handle clone.
    const CHUNK: usize = 4;
    // Fan-out pays for itself only past a handful of bodies (mirrors the
    // parallel-check threshold in `baml_project`).
    const MIN_PARALLEL: usize = 9;

    if seeds.len() < MIN_PARALLEL || rayon::current_num_threads() <= 1 {
        return seeds.iter().map(|seed| lower_seed(db, seed, opt)).collect();
    }
    let (first, rest) = seeds.split_first().expect("seeds checked non-empty above");

    // Handles are cloned OUTSIDE the rayon scope — a `!Sync` database cannot
    // be borrowed by the (Send) scope closure — and each chunk's handle is
    // MOVED into its task. A database that mints no handles keeps Stage A
    // serial.
    let chunks: Vec<&[FnSeed]> = rest.chunks(CHUNK).collect();
    let mut handles: Vec<Box<dyn baml_compiler2_mir::Db + Send>> = Vec::with_capacity(chunks.len());
    for _ in &chunks {
        match db.parallel_db_handle() {
            Some(handle) => handles.push(handle),
            None => return seeds.iter().map(|seed| lower_seed(db, seed, opt)).collect(),
        }
    }

    // Warm the shared file/package-level memos before fanning out, so cold
    // workers don't all block on the same shared memo slots.
    let first_mir = lower_seed(db, first, opt);

    let (tx, rx) = std::sync::mpsc::channel::<(usize, Vec<baml_compiler2_mir::MirFunction>)>();
    rayon::scope(move |s| {
        // Seed index of the current chunk's first element (`first` is 0).
        let mut next_start = 1usize;
        for (chunk, handle) in chunks.into_iter().zip(handles) {
            let tx = tx.clone();
            let chunk_start = next_start;
            next_start += chunk.len();
            s.spawn(move |_| {
                let db: &dyn baml_compiler2_mir::Db = &*handle;
                let out: Vec<baml_compiler2_mir::MirFunction> =
                    chunk.iter().map(|seed| lower_seed(db, seed, opt)).collect();
                // Receiver outlives the scope; a send only fails if it
                // dropped early, which would mean a panic elsewhere.
                let _ = tx.send((chunk_start, out));
            });
        }
    });

    let mut mirs: Vec<Option<baml_compiler2_mir::MirFunction>> = Vec::with_capacity(seeds.len());
    mirs.resize_with(seeds.len(), || None);
    mirs[0] = Some(first_mir);
    for (chunk_start, out) in rx {
        for (offset, mir) in out.into_iter().enumerate() {
            mirs[chunk_start + offset] = Some(mir);
        }
    }
    mirs.into_iter()
        .map(|mir| mir.expect("every chunk reports exactly its seeds"))
        .collect()
}

/// Pass 4, parallel (multi-threaded rayon pools): compile function bodies
/// across rayon workers, byte-identically to [`emit_functions_serial`].
///
/// Three stages:
///
/// - **Stage A (parallel, salsa)**: lower every function to MIR across
///   worker-owned database handles ([`lower_seed_mirs`]), collecting owned
///   work items in exactly the serial pass's iteration order. Falls back to
///   the serial loop when the database mints no handles.
/// - **Stage B (parallel, no salsa)**: pure codegen of every bytecode body.
///   Each worker mints into a fresh fragment pool based at the shared
///   watermark `W = program.objects.len()`, so pre-existing absolute indices
///   (< W: classes/enums/interfaces from Passes 1–3, resolved through frozen
///   maps and the [`ClassFieldSnapshot`]) stay valid and every worker mint is
///   fragment-relative (>= W). Workers never touch `program.globals` — every
///   `GlobalIndex` they embed is a Pass-1 slot read from the frozen `globals`
///   map — so global operands are absolute in every worker and are never
///   rewritten at merge time.
/// - **Stage C (serial merge, original order)**: splice each fragment into
///   the program pool (replaying cross-function `GenericFunction` interning —
///   see [`merge_function_fragment`]), then run the unchanged serial tail
///   (metadata attachment and registration).
///
/// Because the serial pool is append-only within Pass 4 and the only
/// cross-function coupling is the `GenericFunction` interning scan (replayed
/// exactly by the merge), concatenating fragments in original function order
/// reproduces the serial pool layout byte for byte.
#[allow(clippy::too_many_arguments)]
fn emit_functions_parallel(
    db: &dyn crate::Db,
    files: &[baml_base::SourceFile],
    skip_clean: Option<&HashSet<String>>,
    globals: &HashMap<String, usize>,
    classes: &HashMap<String, HashMap<String, usize>>,
    class_object_indices: &HashMap<String, usize>,
    enum_object_indices: &HashMap<String, usize>,
    enum_variants: &HashMap<String, HashMap<String, usize>>,
    class_fields: &ClassFieldSnapshot,
    alias_caches: &HashMap<Name, ResolvedAliases>,
    program: &mut Program,
    opt: OptLevel,
) {
    use rayon::prelude::*;

    // --- Stage A: lower every function to MIR (parallel: salsa queries on
    // pre-cloned handles) ---
    // Enumeration (clean-file skip, item trees, line indexes) is cheap and
    // stays serial on `db`; the lowering itself — the expensive half of
    // Pass 4 — fans out across worker-owned database handles (see
    // [`lower_seed_mirs`]), reassembled into this exact enumeration order.
    let mut seeds: Vec<FnSeed> = Vec::new();
    for file in files {
        let rel_path = relative_source_path(db, *file);
        // Clean files are never lowered (see the serial pass).
        if let Some(clean) = skip_clean {
            if clean.contains(&rel_path) {
                continue;
            }
        }
        record_lowered_file(&rel_path);
        let line_starts: std::sync::Arc<[u32]> = build_line_starts(file.text(db)).into();
        let is_builtin_file = file.path(db).to_string_lossy().starts_with("<builtin>/");
        for &func_loc in file_functions(db, *file) {
            // Required interface methods are signature-only items: nothing
            // to compile or index (mirrors their pre-item invisibility here).
            if baml_compiler2_ppir::item_data::is_required_interface_method(db, func_loc) {
                continue;
            }
            seeds.push(FnSeed {
                file: *file,
                local_id: func_loc.id(db),
                source_file: rel_path.clone(),
                line_starts: line_starts.clone(),
                is_builtin_file,
            });
        }
    }
    let mirs = lower_seed_mirs(db, &seeds, opt);
    let work: Vec<FnWorkItem> = seeds
        .into_iter()
        .zip(mirs)
        .map(|(seed, mir)| FnWorkItem {
            file: seed.file,
            local_id: seed.local_id,
            fq_name: mir.item_ref.to_string(),
            mir,
            source_file: seed.source_file,
            line_starts: seed.line_starts,
            is_builtin_file: seed.is_builtin_file,
        })
        .collect();

    // --- Stage B: compile bytecode bodies (parallel: pure codegen) ---
    let watermark = program.objects.len();
    let compiled: Vec<Option<(Function, ObjectPool)>> = work
        .par_iter()
        .map(|item| {
            let MirFunctionKind::Bytecode(body) = &item.mir.kind else {
                // Builtins mint nothing; Stage C constructs them serially.
                return None;
            };
            let mut fragment = ObjectPool::default();
            let empty_capture_types = Vec::new();
            let empty_spawn_capture_indices = HashSet::new();
            let lambda_info = compile_lambdas_flat(
                &item.mir.lambdas,
                Some(body),
                &empty_capture_types,
                &empty_spawn_capture_indices,
                &item.line_starts,
                &item.source_file,
                globals,
                classes,
                class_object_indices,
                enum_object_indices,
                enum_variants,
                class_fields,
                &mut fragment,
                watermark,
                opt,
            );
            let lambda_obj_indices: Vec<usize> = lambda_info.iter().map(|(idx, _)| *idx).collect();
            let lambda_names_vec: Vec<String> =
                lambda_info.iter().map(|(_, name)| name.clone()).collect();
            let ctx = MirCodegenContext {
                globals,
                classes,
                class_object_indices,
                enum_object_indices,
                enum_variants,
                class_fields,
                objects: &mut fragment,
                objects_base: watermark,
                lambda_object_indices: &lambda_obj_indices,
                lambda_names: &lambda_names_vec,
                capture_types: &empty_capture_types,
                spawn_capture_indices: &empty_spawn_capture_indices,
            };
            let mut f = compile_mir_function(
                body,
                item.mir.arity,
                item.mir.span,
                &item.line_starts,
                ctx,
                opt,
            );
            f.name.clone_from(&item.fq_name);
            f.source_file.clone_from(&item.source_file);
            Some((f, fragment))
        })
        .collect();

    // --- Stage C: merge fragments + serial tail (original function order) ---
    // Seed the GenericFunction interning table with anything already pooled
    // below the watermark: the serial scan's candidate set when compiling
    // function N is {pre-Pass-4 pool} ∪ {mints of functions 1..N-1} ∪ {own
    // earlier mints}, and the in-order merge replays exactly that set.
    let mut intern = GenericFunctionInterner::default();
    for (idx, obj) in program.objects.iter().enumerate() {
        if let Object::GenericFunction(gf) = obj {
            intern.insert_if_absent(gf, idx);
        }
    }
    for (item, slot) in work.into_iter().zip(compiled) {
        let mut compiled_fn = match slot {
            Some((function, fragment)) => {
                merge_function_fragment(program, watermark, fragment, function, &mut intern)
            }
            None => {
                let MirFunctionKind::Builtin(kind) = &item.mir.kind else {
                    unreachable!("Stage B compiles every bytecode function")
                };
                match builtin_emit_function(*kind, &item.fq_name, item.mir.arity) {
                    Some(f) => f,
                    // Intrinsics and `__await_any` never become callable
                    // objects (mirrors the serial pass).
                    None => continue,
                }
            }
        };

        let func_loc = FunctionLoc::new(db, item.file, item.local_id);
        let pkg_info = file_package(db, item.file);
        let cache = &alias_caches[&pkg_info.package];
        attach_function_metadata(
            db,
            func_loc,
            cache,
            item.is_builtin_file,
            &item.fq_name,
            &mut compiled_fn,
        );
        register_compiled_function(
            program,
            globals,
            skip_clean.is_some(),
            item.fq_name,
            compiled_fn,
        );
    }
}

/// Interning table for pooled `Object::GenericFunction`s, bucketed by target
/// global slot. Mirrors the serial `emit_constant` scan's equality exactly:
/// same `function` slot and `==` on the `type_args` slice, first pooled match
/// wins.
#[derive(Default)]
struct GenericFunctionInterner {
    by_function: HashMap<usize, Vec<InternedGenericFunction>>,
}

/// One interned instantiation: its type arguments and its final pool index.
type InternedGenericFunction = (Box<[bex_vm_types::RealizedTy]>, usize);

impl GenericFunctionInterner {
    fn get(&self, gf: &bex_vm_types::GenericFunction) -> Option<usize> {
        self.by_function
            .get(&gf.function.raw())?
            .iter()
            .find(|(args, _)| args.as_ref() == gf.type_args.as_ref())
            .map(|(_, idx)| *idx)
    }

    fn insert_if_absent(&mut self, gf: &bex_vm_types::GenericFunction, idx: usize) {
        let bucket = self.by_function.entry(gf.function.raw()).or_default();
        if !bucket
            .iter()
            .any(|(args, _)| args.as_ref() == gf.type_args.as_ref())
        {
            bucket.push((gf.type_args.clone(), idx));
        }
    }
}

/// Splice one worker's fragment pool into the program pool and rebase every
/// fragment-relative object operand (>= `watermark`) in both the fragment's
/// own objects and the compiled function.
///
/// A fragment `Object::GenericFunction` equal to one already pooled (below
/// the watermark or by an earlier-merged fragment) is dropped and its minted
/// index mapped to the existing object — exactly the reuse the serial
/// whole-pool interning scan would have produced. Everything else appends in
/// mint order, so the merged pool layout is byte-identical to the serial
/// pass's. `GlobalIndex` operands are Pass-1 slots, absolute in every worker,
/// and are never rewritten; a `GenericFunction`'s identity (global slot +
/// type args) is therefore stable across the rewrite.
fn merge_function_fragment(
    program: &mut Program,
    watermark: usize,
    fragment: ObjectPool,
    mut compiled_fn: Function,
    intern: &mut GenericFunctionInterner,
) -> Function {
    // Pass 1: append in mint order, building the minted-index → final-index
    // map. Completed before any operand rewrite so references to later mints
    // (e.g. a constant pooled after the lambda that uses it) resolve too.
    let mut index_map: Vec<usize> = Vec::with_capacity(fragment.len());
    let mut appended: Vec<usize> = Vec::with_capacity(fragment.len());
    for obj in fragment {
        let is_generic_function = match &obj {
            Object::GenericFunction(gf) => {
                if let Some(existing) = intern.get(gf) {
                    index_map.push(existing);
                    continue;
                }
                true
            }
            _ => false,
        };
        let idx = program.add_object(obj);
        if is_generic_function {
            if let Object::GenericFunction(gf) = &program.objects[ObjectIndex::from_raw(idx)] {
                intern.insert_if_absent(gf, idx);
            }
        }
        index_map.push(idx);
        appended.push(idx);
    }

    // Pass 2: rewrite fragment-relative operands to their final indices.
    // Object indices below the watermark (classes/enums/strings from Passes
    // 1–3) and all global slots are already absolute — left untouched.
    let rewrite = |operand: bex_vm_types::relink::IndexOperand<'_>| {
        if let bex_vm_types::relink::IndexOperand::Object(idx) = operand {
            let raw = idx.raw();
            if raw >= watermark {
                let final_idx = *index_map
                    .get(raw - watermark)
                    .expect("fragment operand must map to a merged pool index");
                *idx = ObjectIndex::from_raw(final_idx);
            }
        }
    };
    for &idx in &appended {
        bex_vm_types::relink::visit_object_operands(
            &mut program.objects[ObjectIndex::from_raw(idx)],
            rewrite,
        );
    }
    bex_vm_types::relink::visit_index_operands(&mut compiled_fn, rewrite);
    compiled_fn
}

/// Build the callable `Function` object for a builtin, or `None` for the
/// kinds that never become callable objects: intrinsics (call sites lower to
/// `StatementKind::Intrinsic`) and BEP-034 `__await_any` (call sites lower to
/// a `Terminator::AwaitAny` suspend point).
fn builtin_emit_function(kind: BuiltinKind, fq_name: &str, arity: usize) -> Option<Function> {
    let kind = match kind {
        BuiltinKind::Intrinsic | BuiltinKind::AwaitAny => return None,
        BuiltinKind::Io => {
            let sys_op = bex_vm_types::sys_op_for_path(fq_name)
                .unwrap_or_else(|| panic!("unknown sys_op path: {fq_name}"));
            FunctionKind::SysOp(sys_op)
        }
        BuiltinKind::Vm => FunctionKind::NativeUnresolved,
    };
    Some(Function {
        name: fq_name.to_string(),
        source_file: String::new(), // builtins have no source file
        docstring: None,
        declared_name: None,
        arity,
        real_local_count: 0,
        bytecode: Bytecode::default(),
        kind,
        local_names: Vec::new(),
        debug_locals: Vec::new(),
        span: Span::fake(),
        return_type: bex_vm_types::TyTemplate::Null {
            attr: baml_type::TyAttr::default(),
        },
        param_names: Vec::new(),
        param_types: Vec::new(),
        param_has_default: Vec::new(),
        display_type_params: Vec::new(),
        generic_param_bounds: Vec::new(),
        display_param_types: Vec::new(),
        display_return_type: "null".to_string(),
        throws_type: bex_vm_types::TyTemplate::Never {
            attr: baml_type::TyAttr::default(),
        },
        origin: FunctionOrigin::Builtin,
        body_meta: None,
        capture: FunctionCaptureProps::disabled(),
        function_id: 0, // assigned at engine init (interim provider)
        runtime_package: bex_vm_types::HeapPtr::null(),
    })
}

/// Fill a compiled function's signature, throws, origin, and LLM metadata
/// from the item tree — the Pass-4 tail shared by the serial and parallel
/// passes. Every lookup here is a salsa query, so this always runs on the
/// serial control thread.
#[allow(clippy::too_many_arguments)]
fn attach_function_metadata<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    cache: &ResolvedAliases,
    is_builtin_file: bool,
    fq_name: &str,
    compiled_fn: &mut Function,
) {
    let func = function_data(db, func_loc);
    // Set function metadata from signature
    let parameter_defaults = baml_compiler2_ppir::function_parameter_defaults(db, func_loc);
    let signature_metadata = compute_function_metadata(db, func_loc, &parameter_defaults, cache);
    apply_signature_metadata(compiled_fn, &signature_metadata);
    compiled_fn.origin = emitted_function_origin(fq_name, is_builtin_file, func.metadata.origin);

    // Set LLM-specific body_meta if this is an LLM function with a client.
    //
    // NOTE (canary merge): canary removed the runtime `Function.stream_return_type`
    // field and its plumbing (the pre-existing streaming infra from PRs #3362/#3755).
    // The stream return type is now carried by the synthesized `$stream` companion's
    // own `return_type` (see ppir's `companion_stream_return_type`), so the old
    // emit-side pre-computation block was dropped. BEP-049 M5e stream-path rendering
    // of `ctx.output_format()` should be re-verified against canary's streaming.
    if let Some(llm_meta) = function_llm_meta(db, func_loc)
        && let Some(client) = &llm_meta.client_name
    {
        compiled_fn.body_meta = Some(FunctionMeta::Llm {
            client: client.to_string(),
        });
        compiled_fn.capture = FunctionCaptureProps::disabled()
            .with_auto(CaptureCategory::Input)
            .with_auto(CaptureCategory::Output)
            .with_auto(CaptureCategory::Error);
    }
}

/// Pool the compiled function object and register its global slot — the
/// final Pass-4 tail shared by the serial and parallel passes.
fn register_compiled_function(
    program: &mut Program,
    globals: &HashMap<String, usize>,
    dirty_only: bool,
    fq_name: String,
    compiled_fn: Function,
) {
    let fn_obj_idx = program.add_object(Object::Function(Box::new(compiled_fn)));
    program.function_indices.insert(fq_name.clone(), fn_obj_idx);
    let val = ConstValue::Object(ObjectIndex::from_raw(fn_obj_idx));
    if dirty_only {
        // Dirty-only emit: write the function value at its whole-project
        // (Pass-1) slot rather than appending, so clean-file slot holes are
        // preserved and every operand slot reverses to the right name.
        let slot = globals[&fq_name];
        program.function_global_indices.insert(fq_name, slot);
        program.globals[slot] = val;
    } else {
        let slot = program.globals.len();
        debug_assert_eq!(
            globals.get(&fq_name).copied(),
            Some(slot),
            "Pass-4 append slot must match the Pass-1 assignment for {fq_name}",
        );
        program.function_global_indices.insert(fq_name, slot);
        program.add_global(val);
    }
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
    class_fields: &ClassFieldSnapshot,
    objects: &mut ObjectPool,
    objects_base: usize,
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
                    class_fields,
                    objects,
                    objects_base,
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
                    class_fields,
                    objects,
                    objects_base,
                    lambda_object_indices: &nested_obj_indices,
                    lambda_names: &nested_names,
                    capture_types: &capture_info.capture_types,
                    spawn_capture_indices: &capture_info.spawn_capture_indices,
                };
                let mut f =
                    compile_mir_function(body, lambda.arity, lambda.span, line_starts, ctx, opt);
                f.name.clone_from(&lambda_name);
                f.source_file = source_file.to_string();
                // Stamp the runtime signature `lower_lambda` recorded — the
                // same struct and writer as a top-level declaration (lambdas
                // have no TIR `func_data` to read from). Closure values
                // otherwise carry no signature, which BEP-062's
                // `reflect.signature` / `reflect.call_any` consume.
                if let Some(sig) = &lambda.signature {
                    apply_signature_metadata(&mut f, sig);
                }
                let idx = objects_base + objects.len();
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
    class_fields: &ClassFieldSnapshot,
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
                let source_file = relative_source_path(db, *file);
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
                    class_fields,
                    &mut program.objects,
                    0,
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
                    class_fields,
                    objects: &mut program.objects,
                    objects_base: 0,
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
                    docstring: None,
                    declared_name: None,
                    arity: 0,
                    real_local_count: 0,
                    bytecode,
                    kind: FunctionKind::Bytecode,
                    local_names: Vec::new(),
                    debug_locals: Vec::new(),
                    span: baml_base::Span::fake(),
                    return_type: bex_vm_types::TyTemplate::Null {
                        attr: baml_type::TyAttr::default(),
                    },
                    param_names: Vec::new(),
                    param_types: Vec::new(),
                    param_has_default: Vec::new(),
                    display_type_params: Vec::new(),
                    generic_param_bounds: Vec::new(),
                    display_param_types: Vec::new(),
                    display_return_type: "null".to_string(),
                    throws_type: bex_vm_types::TyTemplate::Never {
                        attr: baml_type::TyAttr::default(),
                    },
                    origin: FunctionOrigin::Internal,
                    body_meta: None,
                    capture: FunctionCaptureProps::disabled(),
                    function_id: 0, // assigned at engine init (interim provider)
                    runtime_package: bex_vm_types::HeapPtr::null(),
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
        docstring: None,
        declared_name: None,
        arity: 0,
        real_local_count: 0,
        bytecode,
        kind: FunctionKind::Bytecode,
        local_names: Vec::new(),
        debug_locals: Vec::new(),
        span: baml_base::Span::fake(),
        return_type: bex_vm_types::TyTemplate::Null {
            attr: baml_type::TyAttr::default(),
        },
        param_names: Vec::new(),
        param_types: Vec::new(),
        param_has_default: Vec::new(),
        display_type_params: Vec::new(),
        generic_param_bounds: Vec::new(),
        display_param_types: Vec::new(),
        display_return_type: "null".to_string(),
        throws_type: bex_vm_types::TyTemplate::Never {
            attr: baml_type::TyAttr::default(),
        },
        origin: FunctionOrigin::Internal,
        body_meta: None,
        capture: FunctionCaptureProps::disabled(),
        function_id: 0, // assigned at engine init (interim provider)
        runtime_package: bex_vm_types::HeapPtr::null(),
    })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
        sync::atomic::{AtomicU32, Ordering},
    };

    use baml_base::{FileId, Name, SourceFile, SourceRoot, SourceRootKind, SourceRootTable};
    use baml_compiler2_hir::item_tree::{Attribute, AttributeArg};
    use salsa::Setter;

    use super::*;

    #[salsa::db]
    struct TestDb {
        storage: salsa::Storage<TestDb>,
        next_file_id: AtomicU32,
        /// Present from construction (`Default` fills them in immediately).
        roots: Option<SourceRootTable>,
        workspace: Option<SourceRoot>,
    }

    impl Default for TestDb {
        fn default() -> Self {
            let mut db = Self {
                storage: salsa::Storage::default(),
                next_file_id: AtomicU32::new(0),
                roots: None,
                workspace: None,
            };
            let workspace = SourceRoot::new(
                &db,
                PathBuf::from("."),
                Name::new(baml_type::RESERVED_USER_PACKAGE),
                SourceRootKind::Workspace,
                Vec::new(),
            );
            db.roots = Some(SourceRootTable::new(&db, vec![workspace]));
            db.workspace = Some(workspace);
            db
        }
    }

    impl Clone for TestDb {
        fn clone(&self) -> Self {
            Self {
                storage: self.storage.clone(),
                next_file_id: AtomicU32::new(self.next_file_id.load(Ordering::SeqCst)),
                roots: self.roots,
                workspace: self.workspace,
            }
        }
    }

    impl TestDb {
        /// Add a workspace file (registered on the workspace root).
        fn add_file(&mut self, path: impl Into<PathBuf>, content: &str) -> SourceFile {
            let file_id = FileId::new(self.next_file_id.fetch_add(1, Ordering::SeqCst));
            let root = self
                .workspace
                .expect("workspace root present from construction");
            let file =
                SourceFile::new(self, content.to_string(), path.into(), file_id, false, root);
            let mut files = root.files(self).clone();
            files.push(file);
            root.set_files(self).to(files);
            file
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl baml_compiler2_hir::Db for TestDb {
        fn source_roots(&self) -> SourceRootTable {
            self.roots.expect("root table present from construction")
        }
    }

    #[salsa::db]
    impl baml_compiler2_ppir::Db for TestDb {}

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
    fn removed_hash_string_returns_none() {
        assert_eq!(parse_string_attr_value("#\"raw text\"#"), None);
        assert_eq!(parse_string_attr_value("##\"raw text\"##"), None);
        assert_eq!(parse_string_attr_value("#\"\"#"), None);
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
    }

    #[test]
    fn function_metadata_reports_defaulted_params() {
        // A real parsed function, not a fabricated item tree: the firewall
        // queries this flows through (`function_in_scope_generic_param_bounds`
        // → `function_data`) are total over Locs minted from a real tree, and
        // panic on ids that were never allocated.
        let mut db = TestDb::default();
        let file = db.add_file(
            "test.baml",
            "function f(required: int, with_default: int = 1, also_required: int) -> int { 1 }",
        );

        let func_loc = baml_compiler2_ppir::item_data::file_functions(&db, file)
            .iter()
            .copied()
            .find(|&loc| {
                baml_compiler2_ppir::item_data::function_data(&db, loc)
                    .name
                    .as_str()
                    == "f"
            })
            .expect("test file declares `f`");
        let parameter_defaults =
            baml_compiler2_hir::signature::function_parameter_defaults(&db, func_loc);
        let cache = ResolvedAliases {
            aliases: HashMap::new(),
            recursive: HashSet::new(),
        };

        let metadata = compute_function_metadata(&db, func_loc, &parameter_defaults, &cache);

        assert_eq!(metadata.param_has_default, vec![false, true, false]);
    }

    // ── build_interface_def ─────────────────────────────────────────────

    /// Build the runtime signature for the single interface declared in `source`.
    fn interface_def_for(source: &str, name: &str) -> bex_vm_types::types::InterfaceDef {
        let mut db = TestDb::default();
        let file = db.add_file("test.baml", source);

        let iface_loc = baml_compiler2_ppir::item_data::file_interfaces(&db, file)
            .iter()
            .copied()
            .find(|&loc| {
                baml_compiler2_ppir::item_data::interface_data(&db, loc)
                    .name
                    .as_str()
                    == name
            })
            .expect("test file declares the interface");
        let cache = ResolvedAliases {
            aliases: HashMap::new(),
            recursive: HashSet::new(),
        };
        build_interface_def(
            &db,
            iface_loc,
            baml_type::TypeName::local(baml_base::Name::new(name)),
            baml_type::typetag::TypeTag::of_head(name),
            &cache,
        )
    }

    /// A declaration is described symbolically — `Self.Item` stays an associated
    /// projection rather than being dropped for want of a receiver to substitute.
    #[test]
    fn interface_def_keeps_self_projections() {
        let def = interface_def_for(
            concat!(
                "interface Src {\n",
                "  type Item\n",
                "  function next(self) -> Self.Item throws never\n",
                "}\n",
            ),
            "Src",
        );
        let next = def
            .methods
            .iter()
            .find(|m| m.name.as_str() == "next")
            .expect("`next` is declared");
        assert!(
            matches!(
                next.returns,
                baml_type::RuntimeTy::AssociatedTypeProjection { .. }
            ),
            "expected a projection return, got {:?}",
            next.returns
        );
    }

    /// Every declared parameter occupies its position: a signature's positional
    /// layout is only meaningful if no parameter can silently vanish from it.
    #[test]
    fn interface_def_keeps_every_parameter_position() {
        let def = interface_def_for(
            concat!(
                "interface Sink<T> {\n",
                "  type Item\n",
                "  function put(self, first: Self.Item, second: T, third: int) -> int throws never\n",
                "}\n",
            ),
            "Sink",
        );
        let put = def
            .methods
            .iter()
            .find(|m| m.name.as_str() == "put")
            .expect("`put` is declared");
        assert_eq!(
            put.args.len(),
            3,
            "the `self` receiver drops, the other three stay: {:?}",
            put.args
        );
        assert!(matches!(put.args[2], baml_type::RuntimeTy::Int { .. }));
    }

    /// A `requires` clause is recorded even when it projects through `Self`.
    #[test]
    fn interface_def_keeps_self_projecting_requires() {
        let def = interface_def_for(
            concat!(
                "interface Base {\n",
                "  type Item\n",
                "  function b(self) -> int throws never\n",
                "}\n",
                "interface Derived requires Base<Item = Self.Item> {\n",
                "  type Item\n",
                "  function d(self) -> int throws never\n",
                "}\n",
            ),
            "Derived",
        );
        assert_eq!(
            def.requires.len(),
            1,
            "the `requires` clause must survive lowering: {:?}",
            def.requires
        );
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
        let meta = extract_schema_attrs(&attrs, Some("docs"));
        assert_eq!(meta.description, Some("A field".to_string()));
        assert_eq!(meta.alias, Some("myField".to_string()));
        assert_eq!(meta.docstring, Some("docs".to_string()));
        assert!(!meta.skip);
    }

    #[test]
    fn extract_skip() {
        let attrs = vec![mk_attr("skip", &[])];
        let meta = extract_schema_attrs(&attrs, None);
        assert_eq!(meta.description, None);
        assert_eq!(meta.alias, None);
        assert!(meta.skip);
    }

    #[test]
    fn extract_custom_attrs_into_other() {
        let attrs = vec![
            mk_attr("stream.done", &["true"]),
            mk_attr("internal.opaque", &[]),
            mk_attr("description", &[r#""kept""#]),
        ];
        let meta = extract_schema_attrs(&attrs, None);
        assert_eq!(meta.description, Some("kept".to_string()));
        assert_eq!(meta.other["stream.done"], "true");
        assert_eq!(meta.other["internal.opaque"], "true");
    }

    #[test]
    fn extract_non_string_arg_ignored() {
        let attrs = vec![mk_attr("description", &["42"])];
        let meta = extract_schema_attrs(&attrs, None);
        assert_eq!(meta.description, None);
    }

    #[test]
    fn extract_wrong_arg_count_ignored() {
        let attrs = vec![mk_attr("description", &[])]; // 0 args
        let meta = extract_schema_attrs(&attrs, None);
        assert_eq!(meta.description, None);
    }

    #[test]
    fn extract_duplicate_last_wins() {
        let attrs = vec![
            mk_attr("description", &[r#""first""#]),
            mk_attr("description", &[r#""second""#]),
        ];
        let meta = extract_schema_attrs(&attrs, None);
        assert_eq!(meta.description, Some("second".to_string()));
    }

    #[test]
    fn removed_hash_string_attr_is_ignored() {
        let attrs = vec![mk_attr("description", &["#\"raw desc\"#"])];
        let meta = extract_schema_attrs(&attrs, None);
        assert_eq!(meta.description, None);
    }

    #[test]
    fn extract_regular_string_attr_decodes_escapes() {
        let attrs = vec![mk_attr("description", &[r#""a\nb\tc\\d\"e""#])];
        let meta = extract_schema_attrs(&attrs, None);
        assert_eq!(meta.description, Some("a\nb\tc\\d\"e".to_string()));
    }

    #[test]
    fn extract_no_attrs() {
        let meta = extract_schema_attrs(&[], None);
        assert_eq!(meta, SchemaAttrs::default());
    }
}
