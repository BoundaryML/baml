//! Package interface types and resolution context.
//!
//! `PackageInterface` is a fully-resolved typed summary of everything a package
//! exports — classes, enums, type aliases, functions, and throw sets.
//! Dependent packages consume this instead of reaching into raw `ItemTree` /
//! `TypeExpr` data.
//!
//! `PackageResolutionContext` bundles a package's own `PackageItems` with its
//! dependencies' `PackageInterface`s, providing unified lookup methods.

use std::collections::{BTreeMap, BTreeSet};

use baml_base::{Name, SourceFile};
use baml_compiler2_ast::BuiltinKind;
use baml_compiler2_hir::{
    contributions::Definition,
    file_package,
    loc::{ClassLoc, EnumLoc, FunctionLoc, TypeAliasLoc},
    package::{PackageId, PackageItems, package_dependencies},
};
use baml_type::{FunctionParamMode, FunctionParamTy, ParamTy, QualifiedTypeName, Ty, TyAttr};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::lower::qualify_def;

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
        generic_params: Vec<ParamTy>,
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
    pub generic_params: Vec<ParamTy>,
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
    pub generic_params: Vec<ParamTy>,
    pub builtin_kind: Option<BuiltinKind>,
}

/// Common output for resolved method lookups (includes class context).
pub struct ResolvedMethod {
    pub function: ResolvedFunction,
    pub class_name: Name,
    pub class_generic_params: Vec<ParamTy>,
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

/// Reduce GROUND associated-type projections for an exported surface:
/// `(IntDecoder as Decoder<..>).Output` IS `int` once the impl is known,
/// and the export (describe, codegen schemas) should say so. Symbolic
/// bases (rigid vars) stay - a signature over `T` keeps `T.Item`.
pub fn reduce_ground_projections(db: &dyn baml_compiler2_ppir::Db, ty: &Ty, fuel: u32) -> Ty {
    use baml_type::normalize::{ProjectionStep, TypeContext as _};
    let recurse = |t: &Ty| reduce_ground_projections(db, t, fuel);
    match ty {
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => {
            let base_reduced = recurse(base);
            if fuel > 0 && !baml_type_runtime::contains_typevar(&base_reduced) {
                let facts = crate::facts::Facts::new(db);
                if let ProjectionStep::Reduced(reduced) =
                    facts.project(&base_reduced, interface, member, fuel)
                {
                    return reduce_ground_projections(db, &reduced, fuel - 1);
                }
            }
            Ty::AssociatedTypeProjection {
                base: Box::new(base_reduced),
                interface: Box::new(interface.map_tys(|t| recurse(t))),
                member: member.clone(),
                attr: attr.clone(),
            }
        }
        Ty::List(inner, attr) => Ty::List(Box::new(recurse(inner)), attr.clone()),
        Ty::EvolvingList(inner, attr) => Ty::EvolvingList(Box::new(recurse(inner)), attr.clone()),
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(recurse(key)),
            value: Box::new(recurse(value)),
            attr: attr.clone(),
        },
        Ty::EvolvingMap(key, value, attr) => Ty::EvolvingMap(
            Box::new(recurse(key)),
            Box::new(recurse(value)),
            attr.clone(),
        ),
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(recurse(value)),
            Box::new(recurse(error)),
            attr.clone(),
        ),
        Ty::Union(members, attr) => Ty::Union(members.iter().map(recurse).collect(), attr.clone()),
        Ty::Class(name, args, attr) => Ty::Class(
            name.clone(),
            args.iter().map(recurse).collect(),
            attr.clone(),
        ),
        Ty::Interface(name, args, pins, attr) => Ty::Interface(
            name.clone(),
            args.iter().map(recurse).collect(),
            pins.iter()
                .map(|(pin, t)| (pin.clone(), recurse(t)))
                .collect(),
            attr.clone(),
        ),
        Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            params: params
                .iter()
                .map(|param| FunctionParamTy {
                    name: param.name.clone(),
                    ty: recurse(&param.ty),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(recurse(ret)),
            throws: Box::new(recurse(throws)),
            attr: attr.clone(),
        },
        other => other.clone(),
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

/// Assemble an [`ExportedFunction`] from `function_signature` (the one
/// signature road; `Self`, projections, and bounds resolve exactly as the
/// type provider sees them) plus the effective-throws oracle
/// (`callable_throws`). The one place the two facts are paired. Exported
/// generics are the function's OWN params: the frame minus the enclosing
/// type's prefix (`enclosing_param_count`, 0 for a free function).
fn exported_function<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    name: &Name,
    enclosing_param_count: usize,
) -> ExportedFunction {
    let sig = crate::lower::function_signature(db, func_loc);
    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let params = sig
        .params
        .iter()
        .map(|param| {
            exported_function_param(
                param.name.clone(),
                reduce_ground_projections(db, &param.ty.to_plain(), 8),
                param.has_default,
            )
        })
        .collect();
    let declared_throws = sig
        .throws_declared
        .then(|| reduce_ground_projections(db, &sig.throws.to_plain(), 8));
    let callable_throws = crate::callable::callable_throws(db, func_loc).0;
    let builtin_kind = match body.as_ref() {
        baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
        _ => None,
    };
    ExportedFunction {
        name: name.clone(),
        params,
        return_type: reduce_ground_projections(db, &sig.ret.to_plain(), 8),
        declared_throws,
        callable_throws,
        generic_params: sig.generic_params[enclosing_param_count.min(sig.generic_params.len())..]
            .to_vec(),
        builtin_kind,
    }
}

// ── Per-item lowering helpers ──────────────────────────────────────────────
//
// Shared by the per-file fragments folded into `package_interface`.

/// Lower a class definition into its `ExportedType::Class`.
fn lower_class_export<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    _pkg_items: &PackageItems<'db>,
    class_loc: ClassLoc<'db>,
    name: &Name,
) -> ExportedType {
    let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
    let class_frame = crate::lower::class_generic_frame(db, class_loc);

    // Lower fields under the class frame and bounds so an associated-type
    // projection `T.member` in a field type resolves `T`'s declaring
    // interface.
    let field_ctx = crate::lower::lower_ctx_for_file(db, class_loc.file(db))
        .with_frame(class_frame.clone())
        .with_bounds(crate::lower::class_generic_bounds(db, class_loc));
    let mut fields = Vec::new();
    for field in &class_data.fields {
        let field_ty = field_ctx
            .lower_type_ref(&class_data.type_refs, field.type_ref)
            .to_plain();
        fields.push((field.name.clone(), field_ty));
    }

    // Lower methods
    let mut methods = Vec::new();
    for &method_loc in &class_data.methods {
        let method_data = baml_compiler2_ppir::item_data::function_data(db, method_loc);
        methods.push(exported_function(
            db,
            method_loc,
            &method_data.name,
            class_frame.len(),
        ));
    }

    let qtn = qualify_def(db, Definition::Class(class_loc), name);
    ExportedType::Class {
        qtn,
        fields,
        methods,
        generic_params: class_frame,
    }
}

/// Lower an enum definition into its `ExportedType::Enum`.
fn lower_enum_export<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
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
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_items: &PackageItems<'db>,
    ta_loc: TypeAliasLoc<'db>,
    name: &Name,
) -> ExportedType {
    let _ = pkg_items;
    let resolved = crate::lower::type_alias_value(db, ta_loc).to_plain();
    let qtn = qualify_def(db, Definition::TypeAlias(ta_loc), name);
    ExportedType::TypeAlias { qtn, resolved }
}

/// Lower a free-function definition into its `ExportedFunction`, read off
/// `function_signature` (the one signature road).
fn lower_function_export<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    func_loc: FunctionLoc<'db>,
    name: &Name,
) -> ExportedFunction {
    exported_function(db, func_loc, name, 0)
}

// ── file_interface_fragment Salsa query ────────────────────────────────────

#[salsa::tracked(returns(ref))]
pub fn file_callable_throws_fragment(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
) -> CallableThrowsFragment {
    let by_id = baml_compiler2_ppir::item_data::file_functions(db, file)
        .iter()
        // Required interface methods are signature-only items: their
        // throws is the declared clause read at the DECLARATION scope
        // (the interface driver), never a callable fixpoint entry.
        .filter(|&&func_loc| {
            !baml_compiler2_ppir::item_data::is_required_interface_method(db, func_loc)
        })
        .map(|&func_loc| {
            (
                func_loc.id(db).as_u32(),
                crate::callable::callable_throws(db, func_loc).0,
            )
        })
        .collect();
    CallableThrowsFragment { by_id }
}

#[salsa::tracked(returns(ref))]
pub fn file_interface_fragment(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
) -> FileInterfaceFragment {
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
            functions.insert(name.clone(), lower_function_export(db, func_loc, name));
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
pub fn package_interface<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
) -> PackageInterface {
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
fn fold_package_interface<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
) -> PackageInterface {
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

    let throw_sets = function_throw_sets(db, pkg_id).clone();
    PackageInterface {
        types,
        functions,
        throw_sets,
    }
}

// ── Throw sets (the runtime's per-function throw metadata) ─────────────────

/// One throw fact: a single (leaf) thrown type.
pub type ThrowFact = Ty;

/// Per-package throw sets, keyed by the dotted function key
/// (`throw_set_key`). Derived from `callable_throws` - the transitive
/// caller-facing surface - so `direct` and `transitive` coincide here
/// (TIR's two-tier solver is subsumed by the salsa fixpoint).
#[derive(Debug, Clone, Default, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct FunctionThrowSets {
    pub direct: BTreeMap<Name, BTreeSet<ThrowFact>>,
    pub transitive: BTreeMap<Name, BTreeSet<ThrowFact>>,
}

// Safety: comparison-based replacement for Salsa early cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for FunctionThrowSets {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: pointer is Salsa-owned and valid for replacement.
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
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

impl FunctionThrowSets {
    pub fn transitive_for(&self, key: &Name) -> Option<&BTreeSet<ThrowFact>> {
        self.transitive.get(key)
    }
}

/// Build the throw-set lookup key for a function given its namespace path
/// and short name: the short name alone at top level, dotted otherwise.
pub fn throw_set_key(namespace_path: &[Name], short_name: &Name) -> Name {
    if namespace_path.is_empty() {
        short_name.clone()
    } else {
        let mut parts: Vec<String> = namespace_path
            .iter()
            .map(|n| n.as_str().to_string())
            .collect();
        parts.push(short_name.as_str().to_string());
        Name::new(parts.join("."))
    }
}

/// Decompose a throws surface into its leaf facts (union members flattened,
/// `never` dropped - the empty set IS `never`).
pub fn flatten_ty_to_facts(ty: &Ty) -> BTreeSet<ThrowFact> {
    let mut out = BTreeSet::new();
    collect_leaf_types(ty, &mut out);
    out
}

fn collect_leaf_types(ty: &Ty, out: &mut BTreeSet<Ty>) {
    match ty {
        Ty::Union(members, _) => {
            for member in members {
                collect_leaf_types(member, out);
            }
        }
        Ty::Never { .. } => {}
        other => {
            out.insert(other.clone());
        }
    }
}

/// The package's per-function throw sets, each function's set flattened
/// from its `callable_throws` surface (already transitive: the salsa
/// fixpoint crosses call and package boundaries).
#[salsa::tracked(returns(ref))]
pub fn function_throw_sets<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    package_id: PackageId<'db>,
) -> FunctionThrowSets {
    let pkg_items = baml_compiler2_ppir::package_items(db, package_id);
    let mut sets = FunctionThrowSets::default();
    for (ns_path, ns_items) in &pkg_items.namespaces {
        for def in ns_items.values.values() {
            let Definition::Function(func_loc) = def else {
                continue;
            };
            if baml_compiler2_ppir::item_data::is_required_interface_method(db, *func_loc) {
                continue;
            }
            let data = baml_compiler2_ppir::item_data::function_data(db, *func_loc);
            let key = throw_set_key(ns_path, &data.name);
            // The runtime boundary's shape: literals widen to their
            // primitives, `void` drops, and error-recovery sentinels
            // degrade to the top type (RuntimeTy has no Error).
            let facts: BTreeSet<ThrowFact> = crate::throw_facts::flatten_declared_ty_to_facts(
                &crate::callable::callable_throws(db, *func_loc).0,
            )
            .into_iter()
            .map(|fact| {
                if baml_type_runtime::contains_error_recovery(&fact) {
                    Ty::BuiltinUnknown {
                        attr: TyAttr::default(),
                    }
                } else {
                    fact
                }
            })
            .collect();
            sets.direct.insert(key.clone(), facts.clone());
            sets.transitive.insert(key, facts);
        }
    }
    sets
}

// ── package_resolution_context Salsa query ─────────────────────────────────

#[salsa::tracked(returns(ref))]
pub fn package_resolution_context<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
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
        db: &'db dyn baml_compiler2_ppir::Db,
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
        db: &'db dyn baml_compiler2_ppir::Db,
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
        db: &'db dyn baml_compiler2_ppir::Db,
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
        db: &'db dyn baml_compiler2_ppir::Db,
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

    /// Look up a class method. Dual dispatch.
    pub fn lookup_class_method(
        &self,
        db: &'db dyn baml_compiler2_ppir::Db,
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

    fn lookup_own_class_method(
        &self,
        db: &'db dyn baml_compiler2_ppir::Db,
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

        for &method_loc in &class_data.methods {
            let method_data = baml_compiler2_ppir::item_data::function_data(db, method_loc);
            if &method_data.name != method_name {
                continue;
            }
            let exported = exported_function(
                db,
                method_loc,
                &method_data.name,
                crate::lower::class_generic_frame(db, class_loc).len(),
            );

            return Some(ResolvedMethod {
                function: ResolvedFunction {
                    name: exported.name,
                    params: exported.params,
                    return_type: exported.return_type,
                    declared_throws: exported.declared_throws,
                    callable_throws: exported.callable_throws,
                    generic_params: exported.generic_params,
                    builtin_kind: exported.builtin_kind,
                },
                class_name: class_name.name().clone(),
                class_generic_params: crate::lower::class_generic_frame(db, class_loc),
            });
        }
        None
    }
}

/// Convert a Definition to Ty (own-package path).
fn def_to_ty<'db>(db: &'db dyn baml_compiler2_ppir::Db, def: Definition<'db>) -> Ty {
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
            let args = crate::lower::class_generic_frame(db, loc)
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
