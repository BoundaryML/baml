//! Package interface types and resolution context.
//!
//! `PackageInterface` is a fully-resolved typed summary of everything a package
//! exports — classes, enums, type aliases, functions, and throw sets.
//! Dependent packages consume this instead of reaching into raw `ItemTree` /
//! `TypeExpr` data.
//!
//! `PackageResolutionContext` bundles a package's own `PackageItems` with its
//! dependencies' `PackageInterface`s, providing unified lookup methods.

use std::collections::BTreeMap;

use baml_base::{Name, SourceFile};
use baml_compiler2_ast::BuiltinKind;
use baml_compiler2_hir::{
    contributions::Definition,
    file_package,
    loc::{ClassLoc, EnumLoc, FunctionLoc, TypeAliasLoc},
    package::{PackageId, PackageItems, package_dependencies},
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    infer_context::TirTypeError,
    lower_type_expr::qualify_def,
    throw_inference::{FunctionThrowSets, function_throw_sets},
    ty::{FunctionParamMode, FunctionParamTy, QualifiedTypeName, Ty, TyAttr},
};

/// Count of *honest* (non-seeded) `package_interface` derivations for stdlib
/// packages, since process start. A warm compile that seeds the cached stdlib
/// interface should leave this at zero; a cold compile bumps it up once per
/// stdlib package. Exposed for the `BAML_CACHE_DEBUG` warm-run counter and the
/// seeding tests — not part of any compile result.
static STDLIB_HONEST_DERIVATIONS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Number of stdlib `package_interface`s derived honestly (from source, not
/// from a seed) since process start. Zero on a warm run whose stdlib interface
/// seed served every stdlib package; one per stdlib package on a cold run.
pub fn stdlib_honest_derivations() -> usize {
    STDLIB_HONEST_DERIVATIONS.load(std::sync::atomic::Ordering::Relaxed)
}

// ── Data types ─────────────────────────────────────────────────────────────

/// Fully-resolved typed interface for a package.
/// Consumers never touch dependency `ItemTree` or raw `TypeExpr`.
///
/// Serializes with Borsh so the six stdlib packages' interfaces can be cached
/// once per compiler build (B-694 "export data") and seeded back into a fresh
/// database, skipping the cold re-derivation. Every leaf (`Ty`,
/// `QualifiedTypeName`, `Name`, `FunctionParamTy`, `BuiltinKind`,
/// `FunctionThrowSets`) is Borsh-ready; the `FxHashMap` members serialize
/// deterministically because Borsh sorts map entries by key.
#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PackageInterface {
    /// All exported types: namespace path -> name -> `ExportedType`
    pub types: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedType>>,
    /// All exported free functions: namespace path -> name -> `ExportedFunction`
    pub functions: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedFunction>>,
    /// Throw sets for all functions in this package (transitive, fully inferred).
    pub throw_sets: FunctionThrowSets,
}

/// A type exported from a package.
#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum ExportedType {
    Class {
        qtn: QualifiedTypeName,
        fields: Vec<(Name, Ty)>,
        methods: Vec<ExportedFunction>,
        generic_params: Vec<Name>,
    },
    Enum {
        qtn: QualifiedTypeName,
        variants: Vec<Name>,
    },
    TypeAlias {
        qtn: QualifiedTypeName,
        resolved: Ty,
    },
}

/// A function exported from a package (free function or method).
#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ExportedFunction {
    pub name: Name,
    pub params: Vec<FunctionParamTy>,
    pub return_type: Ty,
    pub declared_throws: Option<Ty>,
    pub callable_throws: Ty,
    /// Function-level generic parameters, including any synthetic callback
    /// effect parameters introduced by bounded signature elaboration.
    pub generic_params: Vec<Name>,
    pub builtin_kind: Option<BuiltinKind>,
}

/// The typed export surface a single file contributes to its package.
///
/// Structural entries are keyed by `Name` with keep-first semantics, exactly
/// mirroring `namespace_items`' `contribs[0]` winner selection *within a file*,
/// so folding these fragments (driven by the resolved `namespace_items`)
/// reproduces the whole-package derivation byte-for-byte.
#[derive(Debug, Clone, PartialEq)]
pub struct FileInterfaceFragment {
    /// The file's namespace path (`file_package(file).namespace_path`).
    pub ns_path: Vec<Name>,
    /// Types this file exports: name -> `ExportedType` (first contribution wins).
    pub types: FxHashMap<Name, ExportedType>,
    /// Free functions this file exports: name -> `ExportedFunction` (first wins).
    pub functions: FxHashMap<Name, ExportedFunction>,
}

/// The only per-file interface data consumed across CLI process boundaries.
/// Exported types and function signatures are derived through the normal Salsa
/// package-interface queries; persisting them in each bytecode unit duplicated
/// work without seeding those queries.
#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct CallableThrowsFragment {
    pub by_id: BTreeMap<u32, Ty>,
}

/// Distinguishes own-package results from dependency results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedSource {
    /// Found in the current package's own `PackageItems`.
    Item,
    /// Found in a dependency's `PackageInterface`.
    Builtin,
}

/// Common output for resolved function signatures.
pub struct ResolvedFunction {
    pub name: Name,
    pub params: Vec<FunctionParamTy>,
    pub return_type: Ty,
    pub declared_throws: Option<Ty>,
    pub callable_throws: Ty,
    pub generic_params: Vec<Name>,
    pub builtin_kind: Option<BuiltinKind>,
}

/// Common output for resolved method lookups (includes class context).
pub struct ResolvedMethod {
    pub function: ResolvedFunction,
    pub class_name: Name,
    pub class_generic_params: Vec<Name>,
}

/// Bundles a package's own items with its dependencies' pre-resolved interfaces.
/// All cross-package lookups go through this context's methods.
///
/// This query result intentionally owns its contents. Storing borrowed refs to
/// other `returns(ref)` queries here made incremental invalidation unsound
/// under rapid edits, because the cached context could outlive the referenced
/// query storage across revisions.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageResolutionContext<'db> {
    pub own_items: PackageItems<'db>,
    pub dep_interfaces: Vec<(Name, PackageInterface)>,
    pub own_package_name: Name,
}

// ── Salsa Update impls ─────────────────────────────────────────────────────

#[allow(unsafe_code)]
unsafe impl salsa::Update for PackageInterface {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        let old_ref = unsafe { &*old_pointer };
        if *old_ref == new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

#[allow(unsafe_code)]
unsafe impl salsa::Update for FileInterfaceFragment {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        let old_ref = unsafe { &*old_pointer };
        if *old_ref == new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

#[allow(unsafe_code)]
unsafe impl salsa::Update for PackageResolutionContext<'_> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        let old_ref = unsafe { &*old_pointer };
        if *old_ref == new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

// ── PackageInterface lookup helpers ────────────────────────────────────────

impl PackageInterface {
    /// Look up a type by explicit namespace and item name.
    ///
    /// Single hash lookup — no split-loop ambiguity.
    pub fn lookup_type(&self, namespace: &[Name], item: &Name) -> Option<&ExportedType> {
        self.types.get(namespace)?.get(item)
    }

    /// Look up a function by explicit namespace and item name.
    pub fn lookup_function(&self, namespace: &[Name], item: &Name) -> Option<&ExportedFunction> {
        self.functions.get(namespace)?.get(item)
    }
}

impl ExportedType {
    pub fn qtn(&self) -> &QualifiedTypeName {
        match self {
            ExportedType::Class { qtn, .. } => qtn,
            ExportedType::Enum { qtn, .. } => qtn,
            ExportedType::TypeAlias { qtn, .. } => qtn,
        }
    }

    /// Convert to a Ty (for type resolution results).
    pub fn to_ty(&self) -> Ty {
        match self {
            // Declared generics live on the type as `TypeVar` args (an
            // unspecialized generic class is `Foo<T>`), not on the name.
            ExportedType::Class {
                qtn,
                generic_params,
                ..
            } => Ty::Class(
                qtn.clone(),
                generic_params
                    .iter()
                    .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
                    .collect(),
                TyAttr::default(),
            ),
            ExportedType::Enum { qtn, .. } => Ty::Enum(qtn.clone(), TyAttr::default()),
            ExportedType::TypeAlias { qtn, .. } => Ty::TypeAlias(qtn.clone(), TyAttr::default()),
        }
    }
}

fn exported_function_param(name: Name, ty: Ty, has_default: bool) -> FunctionParamTy {
    FunctionParamTy {
        name: Some(name),
        ty,
        mode: if has_default {
            FunctionParamMode::Optional
        } else {
            FunctionParamMode::Required
        },
    }
}

struct LoweredClassMethodSignature {
    params: Vec<FunctionParamTy>,
    return_type: Ty,
    declared_throws: Option<Ty>,
    callable_throws: Ty,
    generic_params: Vec<Name>,
    builtin_kind: Option<BuiltinKind>,
}

fn lower_class_method_signature<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    class_data: &baml_compiler2_ppir::item_data::ClassData<'db>,
    method_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    ns_path: &[Name],
    diags: &mut Vec<TirTypeError>,
) -> LoweredClassMethodSignature {
    let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, method_loc);
    let body = baml_compiler2_ppir::function_body(db, method_loc);

    let mut all_generic_params = class_data.generic_params.clone();
    all_generic_params.extend(sig.user_generic_params.iter().cloned());
    all_generic_params.extend(sig.synthetic_effect_params.iter().cloned());

    // `Self` is the enclosing class's full receiver type (`Foo<T>`, or `Array<T>`→`List<T>`
    // for the builtin containers) — resolved through the lowering context, not erased to a
    // bare `Ty::Class` by a name-substitution pre-pass.
    let self_ty = crate::lower_type_expr::self_type_for_class_data(
        class_data,
        ns_path,
        file_package::file_package(db, method_loc.file(db)).package,
    );
    // The method's in-scope type-variable bounds (its own params plus the enclosing
    // class's) so an associated-type projection `T.member` in the signature can resolve
    // `T`'s declaring interface.
    let method_bounds =
        crate::lower_type_expr::function_in_scope_generic_param_bounds(db, method_loc);
    let ctx = crate::lower_type_expr::ScopeCtx {
        db,
        package_items: pkg_items,
        ns_context: ns_path,
        generic_params: &all_generic_params,
        bounds: method_bounds,
        self_ty: Some(self_ty.clone()),
    };
    let lower_with_self = |id: baml_compiler2_hir::type_ref::TypeRefId,
                           diags: &mut Vec<TirTypeError>| {
        crate::lower_type_expr::lower_type_ref(&sig.type_refs, id, &ctx, diags)
    };

    let mut params = Vec::new();
    for param in &sig.params {
        let param_ty = if param.name.as_str() == "self"
            && matches!(
                sig.type_refs[param.type_ref].kind,
                baml_compiler2_hir::type_ref::TypeRefKind::Unknown
            ) {
            self_ty.clone()
        } else {
            lower_with_self(param.type_ref, diags)
        };
        params.push(exported_function_param(
            param.name.clone(),
            param_ty,
            param.has_default,
        ));
    }

    let return_type = sig.return_type.map_or(
        Ty::Unknown {
            attr: TyAttr::default(),
        },
        |id| lower_with_self(id, diags),
    );

    let declared_throws = sig.throws.map(|id| lower_with_self(id, diags));
    let callable_throws = crate::callable::callable_throws(db, method_loc).clone();

    let builtin_kind = match body.as_ref() {
        baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
        _ => None,
    };

    LoweredClassMethodSignature {
        params,
        return_type,
        declared_throws,
        callable_throws,
        generic_params: sig
            .user_generic_params
            .iter()
            .chain(sig.synthetic_effect_params.iter())
            .cloned()
            .collect(),
        builtin_kind,
    }
}

// ── Per-item lowering helpers ──────────────────────────────────────────────
//
// Shared by the per-file fragments folded into `package_interface`.

/// Lower a class definition into its `ExportedType::Class`.
fn lower_class_export<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    class_loc: ClassLoc<'db>,
    name: &Name,
) -> ExportedType {
    let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
    let class_ns = file_package::file_package(db, class_loc.file(db)).namespace_path;

    // Lower fields. The class's type-variable bounds let an associated-type
    // projection `T.member` in a field type resolve `T`'s declaring interface.
    let field_scope = crate::lower_type_expr::ScopeCtx {
        db,
        package_items: pkg_items,
        ns_context: &class_ns,
        generic_params: &class_data.generic_params,
        bounds: crate::lower_type_expr::class_generic_param_bounds(db, class_loc),
        self_ty: None,
    };
    let mut fields = Vec::new();
    let mut diags = Vec::new();
    for field in &class_data.fields {
        if let Some(type_ref) = field.type_ref {
            let field_ty = crate::lower_type_expr::lower_type_ref(
                &class_data.type_refs,
                type_ref,
                &field_scope,
                &mut diags,
            );
            fields.push((field.name.clone(), field_ty));
        } else {
            fields.push((
                field.name.clone(),
                Ty::Unknown {
                    attr: TyAttr::default(),
                },
            ));
        }
    }

    // Lower methods
    let mut methods = Vec::new();
    for &method_loc in &class_data.methods {
        let method_data = baml_compiler2_ppir::item_data::function_data(db, method_loc);
        let lowered = lower_class_method_signature(
            db, pkg_items, class_data, method_loc, &class_ns, &mut diags,
        );

        methods.push(ExportedFunction {
            name: method_data.name.clone(),
            params: lowered.params,
            return_type: lowered.return_type,
            declared_throws: lowered.declared_throws,
            callable_throws: lowered.callable_throws,
            generic_params: lowered.generic_params,
            builtin_kind: lowered.builtin_kind,
        });
    }

    let qtn = qualify_def(db, Definition::Class(class_loc), name);
    ExportedType::Class {
        qtn,
        fields,
        methods,
        generic_params: class_data.generic_params.clone(),
    }
}

/// Lower an enum definition into its `ExportedType::Enum`.
fn lower_enum_export<'db>(
    db: &'db dyn crate::Db,
    enum_loc: EnumLoc<'db>,
    name: &Name,
) -> ExportedType {
    let enum_data = baml_compiler2_ppir::item_data::enum_data(db, enum_loc);
    let qtn = qualify_def(db, Definition::Enum(enum_loc), name);
    ExportedType::Enum {
        qtn,
        variants: enum_data.variants.iter().map(|v| v.name.clone()).collect(),
    }
}

/// Lower a type-alias definition into its `ExportedType::TypeAlias`.
fn lower_alias_export<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    ta_loc: TypeAliasLoc<'db>,
    name: &Name,
) -> ExportedType {
    let ta_data = baml_compiler2_ppir::item_data::type_alias_data(db, ta_loc);
    let ta_ns = file_package::file_package(db, ta_loc.file(db)).namespace_path;
    let mut diags = Vec::new();
    let resolved = ta_data
        .value
        .map(|id| {
            crate::lower_type_expr::lower_type_ref(
                &ta_data.type_refs,
                id,
                &crate::lower_type_expr::ScopeCtx {
                    db,
                    package_items: pkg_items,
                    ns_context: &ta_ns,
                    generic_params: &[],
                    bounds: &crate::lower_type_expr::TypeVarBoundsMap::default(),
                    self_ty: None,
                },
                &mut diags,
            )
        })
        .unwrap_or(Ty::Unknown {
            attr: TyAttr::default(),
        });
    let qtn = qualify_def(db, Definition::TypeAlias(ta_loc), name);
    ExportedType::TypeAlias { qtn, resolved }
}

/// Lower a free-function definition into its `ExportedFunction`.
fn lower_function_export<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    func_loc: FunctionLoc<'db>,
    name: &Name,
) -> ExportedFunction {
    let func_ns = file_package::file_package(db, func_loc.file(db)).namespace_path;
    let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, func_loc);
    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let mut diags = Vec::new();
    let function_generic_params: Vec<Name> = sig
        .user_generic_params
        .iter()
        .chain(sig.synthetic_effect_params.iter())
        .cloned()
        .collect();

    // One lowering scope for the whole signature. The function's in-scope
    // type-variable bounds let an associated-type projection `T.member`
    // resolve `T`'s declaring interface.
    let sig_scope = crate::lower_type_expr::ScopeCtx {
        db,
        package_items: pkg_items,
        ns_context: &func_ns,
        generic_params: &function_generic_params,
        bounds: crate::lower_type_expr::function_in_scope_generic_param_bounds(db, func_loc),
        self_ty: None,
    };

    let mut params = Vec::new();
    for param in &sig.params {
        let param_ty = crate::lower_type_expr::lower_type_ref(
            &sig.type_refs,
            param.type_ref,
            &sig_scope,
            &mut diags,
        );
        params.push(exported_function_param(
            param.name.clone(),
            param_ty,
            param.has_default,
        ));
    }

    let return_type = sig.return_type.map_or(
        Ty::Unknown {
            attr: TyAttr::default(),
        },
        |id| crate::lower_type_expr::lower_type_ref(&sig.type_refs, id, &sig_scope, &mut diags),
    );

    let declared_throws = sig.throws.map(|id| {
        crate::lower_type_expr::lower_type_ref(&sig.type_refs, id, &sig_scope, &mut diags)
    });
    let callable_throws = crate::callable::callable_throws(db, func_loc).clone();

    let builtin_kind = match body.as_ref() {
        baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
        _ => None,
    };

    ExportedFunction {
        name: name.clone(),
        params,
        return_type,
        declared_throws,
        callable_throws,
        generic_params: function_generic_params,
        builtin_kind,
    }
}

// ── file_interface_fragment Salsa query ────────────────────────────────────

#[salsa::tracked(returns(ref))]
pub fn file_callable_throws_fragment(
    db: &dyn crate::Db,
    file: SourceFile,
) -> CallableThrowsFragment {
    let by_id = baml_compiler2_ppir::item_data::file_functions(db, file)
        .iter()
        .map(|&func_loc| {
            (
                func_loc.id(db).as_u32(),
                crate::callable::callable_throws(db, func_loc).clone(),
            )
        })
        .collect();
    CallableThrowsFragment { by_id }
}

#[salsa::tracked(returns(ref))]
pub fn file_interface_fragment(db: &dyn crate::Db, file: SourceFile) -> FileInterfaceFragment {
    let pkg_info = file_package::file_package(db, file);
    let ns_path = pkg_info.namespace_path.clone();
    let pkg_id = PackageId::new(db, pkg_info.package);
    // Lower against the package's resolved items so a per-file fragment matches
    // the whole-package fold.
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let contributions = baml_compiler2_ppir::file_symbol_contributions(db, file);

    // Structural exports, keyed by `Name` with keep-first semantics. A file's
    // *first* contribution of a name is the one `namespace_items` would elect as
    // `contribs[0]` when this is the winning file, so this reproduces the
    // resolver's within-file choice exactly. A first contribution that is not a
    // Class/Enum/TypeAlias (e.g. an interface) still *claims* the name — leaving
    // no structural export — matching the reference derivation's `_ => continue`.
    let mut types: FxHashMap<Name, ExportedType> = FxHashMap::default();
    let mut claimed_types: FxHashSet<Name> = FxHashSet::default();
    for (name, contrib) in &contributions.types {
        if !claimed_types.insert(name.clone()) {
            continue;
        }
        let exported = match contrib.definition {
            Definition::Class(class_loc) => lower_class_export(db, pkg_items, class_loc, name),
            Definition::Enum(enum_loc) => lower_enum_export(db, enum_loc, name),
            Definition::TypeAlias(ta_loc) => lower_alias_export(db, pkg_items, ta_loc, name),
            _ => continue,
        };
        types.insert(name.clone(), exported);
    }

    let mut functions: FxHashMap<Name, ExportedFunction> = FxHashMap::default();
    let mut claimed_values: FxHashSet<Name> = FxHashSet::default();
    for (name, contrib) in &contributions.values {
        if !claimed_values.insert(name.clone()) {
            continue;
        }
        if let Definition::Function(func_loc) = contrib.definition {
            functions.insert(
                name.clone(),
                lower_function_export(db, pkg_items, func_loc, name),
            );
        }
    }

    FileInterfaceFragment {
        ns_path,
        types,
        functions,
    }
}

// ── package_interface Salsa query ──────────────────────────────────────────

#[salsa::tracked(returns(ref))]
pub fn package_interface<'db>(db: &'db dyn crate::Db, pkg_id: PackageId<'db>) -> PackageInterface {
    let pkg_name = pkg_id.name(db);

    // Seed short-circuit (B-694). `seeds.by_package(db)` is a *tracked* read of
    // the `SeededStdlibInterface` input: databases that seed (the CLI, the LSP)
    // hold the input from construction (empty until seeded), so this memo records
    // a dependency on the seed map and a later `set_seeded_stdlib_interface`
    // reliably invalidates it. Only stdlib package names appear in the map, so a
    // user package never hits the seed and derives normally. Because the entire
    // stdlib derivation cluster (signature lowering, `callable_throws` /
    // body inference, throw-set solving) is reachable only through this query,
    // short-circuiting here skips all of it. This stays ABOVE the fragment fold.
    if let Some(seeds) = db.seeded_stdlib_interface() {
        if let Some(bytes) = seeds.by_package(db).get(pkg_name.as_str()) {
            if let Ok(iface) = borsh::from_slice::<PackageInterface>(bytes) {
                return iface;
            }
            // corrupt/stale seed → fall through to honest derivation
        }
    }

    // Honest derivation. Count stdlib-package derivations so a warm run can
    // assert zero (the seed served every stdlib package). The authoritative set
    // of stdlib packages is the embedded builtin manifest — a package is stdlib
    // iff it contributes a `<builtin>/…` file — so this stays in lockstep with
    // the files that actually ship (no hand-maintained list to drift).
    if baml_builtins2::stdlib_package_names().contains(&pkg_name.as_str()) {
        STDLIB_HONEST_DERIVATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fold_package_interface(db, pkg_id)
}

/// Fold each file's `file_interface_fragment` into the whole-package interface.
///
/// Winner selection is driven by the resolved `pkg_items.namespaces` (the
/// deterministic `contribs[0]` pick); per-item *lowering* lives in
/// `file_interface_fragment`.
fn fold_package_interface<'db>(db: &'db dyn crate::Db, pkg_id: PackageId<'db>) -> PackageInterface {
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let mut types: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedType>> = FxHashMap::default();
    let mut functions: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedFunction>> =
        FxHashMap::default();

    for (ns_path, ns_items) in &pkg_items.namespaces {
        for (name, def) in &ns_items.types {
            let frag = file_interface_fragment(db, def.file(db));
            if let Some(exported) = frag.types.get(name) {
                types
                    .entry(ns_path.clone())
                    .or_default()
                    .insert(name.clone(), exported.clone());
            }
        }
        for (name, def) in &ns_items.values {
            let Definition::Function(func_loc) = def else {
                continue;
            };
            let frag = file_interface_fragment(db, func_loc.file(db));
            if let Some(exported) = frag.functions.get(name) {
                functions
                    .entry(ns_path.clone())
                    .or_default()
                    .insert(name.clone(), exported.clone());
            }
        }
    }

    let throw_sets = function_throw_sets(db, pkg_id);
    PackageInterface {
        types,
        functions,
        throw_sets: throw_sets.clone(),
    }
}

// ── package_resolution_context Salsa query ─────────────────────────────────

#[salsa::tracked(returns(ref))]
pub fn package_resolution_context<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
) -> PackageResolutionContext<'db> {
    let own_items = baml_compiler2_ppir::package_items(db, pkg_id).clone();
    let deps = package_dependencies(db, pkg_id);
    let dep_interfaces: Vec<(Name, PackageInterface)> = deps
        .iter()
        .map(|dep_id| {
            let name = dep_id.name(db);
            let iface = package_interface(db, *dep_id).clone();
            (name, iface)
        })
        .collect();
    PackageResolutionContext {
        own_items,
        dep_interfaces,
        own_package_name: pkg_id.name(db),
    }
}

// ── PackageResolutionContext lookup methods ─────────────────────────────────

impl<'db> PackageResolutionContext<'db> {
    /// Get `PackageItems` for an accessible package (own or declared dependency).
    ///
    /// Returns `Some` for the own package and any declared dependency,
    /// `None` for undeclared packages.
    pub fn items_for_package(
        &'db self,
        db: &'db dyn crate::Db,
        pkg_name: &Name,
    ) -> Option<&'db PackageItems<'db>> {
        if pkg_name.as_str() == self.own_package_name.as_str() {
            Some(&self.own_items)
        } else if self
            .dep_interfaces
            .iter()
            .any(|(n, _)| n.as_str() == pkg_name.as_str())
        {
            let pkg_id = PackageId::new(db, pkg_name.clone());
            Some(baml_compiler2_ppir::package_items(db, pkg_id))
        } else {
            None
        }
    }

    /// Resolve a type by path. Own-package via `PackageItems`, then deps.
    pub fn resolve_type(
        &self,
        db: &'db dyn crate::Db,
        path: &[Name],
        ns_context: &[Name],
    ) -> Option<(ResolvedSource, Ty)> {
        let item = path.last()?;
        // Try namespace-qualified path first
        if !ns_context.is_empty() {
            let ns: Vec<_> = ns_context
                .iter()
                .chain(path[..path.len() - 1].iter())
                .cloned()
                .collect();
            if let Some(result) = self.resolve_type_in_own_then_deps(db, &ns, item) {
                return Some(result);
            }
        }

        // When ns_context is empty, try unqualified path (same-namespace for root files)
        if ns_context.is_empty() {
            if let Some(result) =
                self.resolve_type_in_own_then_deps(db, &path[..path.len() - 1], item)
            {
                return Some(result);
            }
        }
        // No bare fallback from non-root namespaces: cross-namespace references
        // in the same package must start with `root`.

        // Try package-prefixed path (first segment is package name)
        if path.len() >= 2 {
            if path[0].as_str() == "root" {
                if let Some(def) = self.own_items.lookup_type(&path[1..path.len() - 1], item) {
                    let ty = def_to_ty(db, def);
                    return Some((ResolvedSource::Item, ty));
                }
            }
            for (dep_name, dep_iface) in &self.dep_interfaces {
                if &path[0] == dep_name {
                    if let Some(exported) = dep_iface.lookup_type(&path[1..path.len() - 1], item) {
                        return Some((ResolvedSource::Builtin, exported.to_ty()));
                    }
                }
            }
        }

        None
    }

    fn resolve_type_in_own_then_deps(
        &self,
        db: &'db dyn crate::Db,
        namespace: &[Name],
        item: &Name,
    ) -> Option<(ResolvedSource, Ty)> {
        if let Some(def) = self.own_items.lookup_type(namespace, item) {
            let ty = def_to_ty(db, def);
            return Some((ResolvedSource::Item, ty));
        }
        for (_dep_name, dep_iface) in &self.dep_interfaces {
            if let Some(exported) = dep_iface.lookup_type(namespace, item) {
                return Some((ResolvedSource::Builtin, exported.to_ty()));
            }
        }
        None
    }

    /// Resolve a function/value by path. Returns the Definition for own-package values.
    pub fn resolve_value(
        &self,
        db: &'db dyn crate::Db,
        path: &[Name],
        ns_context: &[Name],
    ) -> Option<(ResolvedSource, Definition<'db>)> {
        let item = path.last()?;
        if !ns_context.is_empty() {
            let ns: Vec<_> = ns_context
                .iter()
                .chain(path[..path.len() - 1].iter())
                .cloned()
                .collect();
            if let Some(result) = self.resolve_value_in_own(&ns, item) {
                return Some(result);
            }
        }
        // When ns_context is empty, try unqualified path (same-namespace for root files)
        if ns_context.is_empty() {
            if let Some(result) = self.resolve_value_in_own(&path[..path.len() - 1], item) {
                return Some(result);
            }
        }
        // No bare fallback from non-root namespaces — cross-namespace requires explicit qualification
        // root.* prefix handling (parity with resolve_type)
        if path.len() >= 2 {
            if path[0].as_str() == "root" {
                if let Some(def) = self.own_items.lookup_value(&path[1..path.len() - 1], item) {
                    return Some((ResolvedSource::Item, def));
                }
            }
            // dep-prefixed search (parity with resolve_type)
            for (dep_name, _dep_iface) in &self.dep_interfaces {
                if &path[0] == dep_name {
                    let dep_pkg_id = PackageId::new(db, dep_name.clone());
                    let dep_items = baml_compiler2_ppir::package_items(db, dep_pkg_id);
                    if let Some(def) = dep_items.lookup_value(&path[1..path.len() - 1], item) {
                        return Some((ResolvedSource::Builtin, def));
                    }
                }
            }
        }
        None
    }

    fn resolve_value_in_own(
        &self,
        namespace: &[Name],
        item: &Name,
    ) -> Option<(ResolvedSource, Definition<'db>)> {
        if let Some(def) = self.own_items.lookup_value(namespace, item) {
            return Some((ResolvedSource::Item, def));
        }
        None
    }

    /// Look up class fields. Dual dispatch:
    /// - Own-package: `ItemTree` -> lower fields
    /// - Dependency: `ExportedType::Class` { fields }
    pub fn lookup_class_fields(
        &self,
        db: &'db dyn crate::Db,
        class_name: &QualifiedTypeName,
    ) -> Vec<(Name, Ty)> {
        let class_pkg = class_name.package();
        if class_pkg.as_str() == self.own_package_name.as_str() {
            self.lookup_own_class_fields(db, class_name)
        } else {
            for (dep_name, dep_iface) in &self.dep_interfaces {
                if dep_name != class_pkg {
                    continue;
                }
                if let Some(ExportedType::Class { fields, .. }) =
                    dep_iface.lookup_type(class_name.namespace(), class_name.name())
                {
                    return fields.clone();
                }
            }
            Vec::new()
        }
    }

    /// Look up a class method. Dual dispatch.
    pub fn lookup_class_method(
        &self,
        db: &'db dyn crate::Db,
        class_name: &QualifiedTypeName,
        method_name: &Name,
    ) -> Option<ResolvedMethod> {
        let class_pkg = class_name.package();
        if class_pkg.as_str() == self.own_package_name.as_str() {
            self.lookup_own_class_method(db, class_name, method_name)
        } else {
            for (dep_name, dep_iface) in &self.dep_interfaces {
                if dep_name != class_pkg {
                    continue;
                }
                if let Some(ExportedType::Class {
                    methods,
                    generic_params,
                    ..
                }) = dep_iface.lookup_type(class_name.namespace(), class_name.name())
                {
                    if let Some(method) = methods.iter().find(|m| &m.name == method_name) {
                        return Some(ResolvedMethod {
                            function: ResolvedFunction {
                                name: method.name.clone(),
                                params: method.params.clone(),
                                return_type: method.return_type.clone(),
                                declared_throws: method.declared_throws.clone(),
                                callable_throws: method.callable_throws.clone(),
                                generic_params: method.generic_params.clone(),
                                builtin_kind: method.builtin_kind,
                            },
                            class_name: class_name.name().clone(),
                            class_generic_params: generic_params.clone(),
                        });
                    }
                }
            }
            None
        }
    }

    fn lookup_own_class_fields(
        &self,
        db: &'db dyn crate::Db,
        class_name: &QualifiedTypeName,
    ) -> Vec<(Name, Ty)> {
        let Some(def) = self
            .own_items
            .lookup_type(class_name.namespace(), class_name.name())
        else {
            return Vec::new();
        };
        let Definition::Class(class_loc) = def else {
            return Vec::new();
        };
        let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
        let ns = file_package::file_package(db, class_loc.file(db)).namespace_path;
        let field_scope = crate::lower_type_expr::ScopeCtx {
            db,
            package_items: &self.own_items,
            ns_context: &ns,
            generic_params: &class_data.generic_params,
            bounds: crate::lower_type_expr::class_generic_param_bounds(db, class_loc),
            self_ty: None,
        };
        let mut diags = Vec::new();
        let mut fields = Vec::new();
        for field in &class_data.fields {
            if let Some(type_ref) = field.type_ref {
                let field_ty = crate::lower_type_expr::lower_type_ref(
                    &class_data.type_refs,
                    type_ref,
                    &field_scope,
                    &mut diags,
                );
                fields.push((field.name.clone(), field_ty));
            } else {
                fields.push((
                    field.name.clone(),
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                ));
            }
        }
        fields
    }

    fn lookup_own_class_method(
        &self,
        db: &'db dyn crate::Db,
        class_name: &QualifiedTypeName,
        method_name: &Name,
    ) -> Option<ResolvedMethod> {
        let def = self
            .own_items
            .lookup_type(class_name.namespace(), class_name.name())?;
        let Definition::Class(class_loc) = def else {
            return None;
        };
        let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
        let ns = file_package::file_package(db, class_loc.file(db)).namespace_path;
        let mut diags = Vec::new();

        for &method_loc in &class_data.methods {
            let method_data = baml_compiler2_ppir::item_data::function_data(db, method_loc);
            if &method_data.name != method_name {
                continue;
            }
            let lowered = lower_class_method_signature(
                db,
                &self.own_items,
                class_data,
                method_loc,
                &ns,
                &mut diags,
            );

            return Some(ResolvedMethod {
                function: ResolvedFunction {
                    name: method_data.name.clone(),
                    params: lowered.params,
                    return_type: lowered.return_type,
                    declared_throws: lowered.declared_throws,
                    callable_throws: lowered.callable_throws,
                    generic_params: lowered.generic_params,
                    builtin_kind: lowered.builtin_kind,
                },
                class_name: class_name.name().clone(),
                class_generic_params: class_data.generic_params.clone(),
            });
        }
        None
    }
}

/// Convert a Definition to Ty (own-package path).
fn def_to_ty<'db>(db: &'db dyn crate::Db, def: Definition<'db>) -> Ty {
    let name = match def {
        Definition::Class(loc) => baml_compiler2_ppir::item_data::class_data(db, loc)
            .name
            .clone(),
        Definition::Enum(loc) => baml_compiler2_ppir::item_data::enum_data(db, loc)
            .name
            .clone(),
        Definition::Interface(loc) => baml_compiler2_ppir::item_data::interface_data(db, loc)
            .name
            .clone(),
        Definition::TypeAlias(loc) => baml_compiler2_ppir::item_data::type_alias_data(db, loc)
            .name
            .clone(),
        _ => {
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        }
    };
    match def {
        Definition::Class(loc) => {
            // Declared generics live on the type as `TypeVar` args, matching
            // `ExportedType::to_ty` so own-package and dependency resolution
            // produce the same `Ty::Class(qtn, [TypeVar…])` shape.
            let args = baml_compiler2_ppir::item_data::class_data(db, loc)
                .generic_params
                .iter()
                .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
                .collect();
            Ty::Class(qualify_def(db, def, &name), args, TyAttr::default())
        }
        Definition::Interface(_) => Ty::Interface(
            qualify_def(db, def, &name),
            vec![],
            vec![],
            TyAttr::default(),
        ),
        Definition::Enum(_) => Ty::Enum(qualify_def(db, def, &name), TyAttr::default()),
        Definition::TypeAlias(_) => Ty::TypeAlias(qualify_def(db, def, &name), TyAttr::default()),
        _ => Ty::Unknown {
            attr: TyAttr::default(),
        },
    }
}
