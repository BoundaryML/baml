//! Package interface types and resolution context.
//!
//! `PackageInterface` is a fully-resolved typed summary of everything a package
//! exports — classes, enums, type aliases, interfaces, functions, throw sets,
//! and the namespace set. Mounted, source-less dependencies consume this
//! instead of reaching into raw `ItemTree` / `TypeExpr` data; source-backed
//! dependencies retain their loc path where navigation/body access needs it.
//!
//! `PackageResolutionContext` bundles a package's own `PackageItems` with its
//! dependencies' `PackageInterface`s, providing unified lookup methods.

use std::collections::{BTreeMap, BTreeSet};

use baml_base::{Name, SourceFile};
use baml_compiler2_ast::BuiltinKind;
use baml_compiler2_hir::{
    contributions::Definition,
    file_package,
    loc::{ClassLoc, EnumLoc, FunctionLoc, InterfaceLoc, TypeAliasLoc},
    package::{PackageId, PackageItems, package_dependencies},
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    lower_type_expr::qualify_def,
    throw_inference::{FunctionThrowSets, function_throw_sets},
    ty::{FunctionParamTy, ParamTy, QualifiedTypeName, Ty, TyAttr},
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
/// `QualifiedTypeName`, `Name`, `FunctionParamTy`, `ParamTy`,
/// `baml_type::Interface`, `BuiltinKind`, `FunctionThrowSets`) is Borsh-ready;
/// the `FxHashMap` members serialize deterministically because Borsh sorts map
/// entries by key, and `BTreeSet` is ordered by construction.
#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PackageInterface {
    /// All exported types: namespace path -> name -> `ExportedType`
    pub types: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedType>>,
    /// All exported free functions: namespace path -> name -> `ExportedFunction`
    pub functions: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedFunction>>,
    /// Throw sets for all functions in this package (transitive, fully inferred).
    pub throw_sets: FunctionThrowSets,
    /// The package's *full* namespace-path set — every namespace `package_items`
    /// resolves, whether or not it exports a type or function (the root
    /// namespace is `[]`). A source-less consumer needs this to distinguish "a
    /// namespace with nothing visible" from "no such namespace" (BEP-066 slice
    /// 6a; mounted namespace traversal can consume it without source items).
    pub namespaces: BTreeSet<Vec<Name>>,
    /// Every `implements` block the package declares, exported loc-free (BEP-066
    /// slice 6a) — the blob-side twin of the `impl_data` substrate, so a
    /// source-less dependency still contributes its impls to matching,
    /// membership, coherence, and dispatch (closing the R2 "fails-open" hole).
    ///
    /// Sorted by each row's borsh encoding — a canonical total order over the
    /// row's full content, independent of file enumeration order (ties are
    /// byte-identical rows, so their relative order is unobservable).
    pub impls: Vec<ExportedImpl>,
}

/// A type exported from a package.
#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum ExportedType {
    Class {
        qtn: QualifiedTypeName,
        /// Fields in declaration order: name, resolved type, and the span-free
        /// schema attributes (`@alias`/`@description`).
        fields: Vec<(Name, Ty, ExportedFieldAttrs)>,
        methods: Vec<ExportedFunction>,
        generic_params: Vec<ParamTy>,
        /// Per-parameter interface-bound conjunctions, parallel to
        /// `generic_params` (`T extends A & B` yields two entries in `T`'s
        /// `Vec`; an unbounded parameter yields an empty one).
        generic_param_bounds: Vec<Vec<baml_type::Interface>>,
    },
    Enum {
        qtn: QualifiedTypeName,
        variants: Vec<Name>,
    },
    TypeAlias {
        qtn: QualifiedTypeName,
        resolved: Ty,
    },
    /// An interface declaration's full loc-free surface (BEP-066 slice 6a),
    /// consumed for mounted type/bound/member resolution and virtual dispatch.
    ///
    /// Every type below is lowered at the interface's *own declaration scope*:
    /// the declared generic parameters are rigid `TypeVar`s and `Self` is the
    /// symbolic rigid `Self` type variable (`self_param`), so `Self.X` mentions
    /// survive as symbolic `AssociatedTypeProjection`s a consumer realizes by
    /// substitution.
    Interface {
        qtn: QualifiedTypeName,
        /// The symbolic `Self` parameter of the interface's generic
        /// environment — the `TypeVar` that `Self` lowers to in every type
        /// below.
        self_param: ParamTy,
        /// The declared generic parameters (excluding `Self` and the
        /// associated-type parameters).
        generic_params: Vec<ParamTy>,
        /// Per-parameter interface-bound conjunctions, parallel to
        /// `generic_params`.
        param_bounds: Vec<Vec<baml_type::Interface>>,
        /// The transitive `requires` closure, pre-flattened and pre-realized
        /// with argument/associated-type substitutions (symbolic `Self`,
        /// identity generic arguments; defaults left unfilled) — see
        /// `crate::interfaces::interface_requires_closure_symbolic` (private),
        /// the single derivation shared with the checker's closure walk.
        requires: Vec<baml_type::Interface>,
        /// Associated types in declaration order.
        associated_types: Vec<ExportedAssociatedType>,
        /// Fields in declaration order, types resolved in the interface's own
        /// scope (the same data [`crate::interfaces::resolve_interface_fields`]
        /// carries), plus schema attributes.
        fields: Vec<(Name, Ty, ExportedFieldAttrs)>,
        /// Required methods in declaration order (no body — an implementor must
        /// provide each). Signatures keep `Self` symbolic; `callable_throws` is
        /// the declared clause (`unknown` when unwritten, matching
        /// `signature::lower_signature`'s `Missing` slot convention). Split
        /// from [`default_methods`](Self::Interface::default_methods) exactly
        /// as `InterfaceData` splits them — a source-less consumer needs the
        /// distinction for conformance (E0113 required-method coverage) and
        /// method-default fallback.
        required_methods: Vec<ExportedFunction>,
        /// Default methods in declaration order (the interface provides a
        /// body). `callable_throws` is the body-inferred contract from
        /// [`crate::callable::callable_throws`].
        default_methods: Vec<ExportedFunction>,
    },
}

/// Span-free field-level schema attributes exported with a class or interface
/// field. Mirrors emit's `extract_schema_attrs` reading of `@alias` /
/// `@description` (malformed attributes are diagnosed at HIR validation and
/// skipped here).
#[derive(Debug, Clone, Default, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ExportedFieldAttrs {
    pub alias: Option<String>,
    pub description: Option<String>,
}

/// An interface's associated type, exported loc-free.
#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ExportedAssociatedType {
    pub name: Name,
    /// The declared `extends` bound, realized at the interface's identity
    /// arguments with `Self` symbolic (`None` when unbounded or not an
    /// interface — the declaration checker owns that diagnostic).
    pub bound: Option<baml_type::Interface>,
    /// The declared default, lowered ONCE with symbolic `Self` — exactly what
    /// [`crate::interfaces::interface_associated_type_default`] produces — so a
    /// consumer realizes it by substituting its receiver and arguments.
    pub default: Option<Ty>,
}

/// A function exported from a package (free function or method).
#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ExportedFunction {
    pub name: Name,
    pub params: Vec<FunctionParamTy>,
    pub return_type: Ty,
    pub declared_throws: Option<Ty>,
    pub callable_throws: Ty,
    /// Function-level generic parameters: user-declared parameters followed by
    /// synthetic callback-effect parameters introduced by signature
    /// elaboration. Runtime layout erases the synthetic effects.
    pub generic_params: Vec<ParamTy>,
    /// Per-parameter interface-bound conjunctions, parallel to
    /// `generic_params` (when synthetic effect parameters are present they are
    /// unbounded, so their entries are empty).
    pub generic_param_bounds: Vec<Vec<baml_type::Interface>>,
    pub builtin_kind: Option<BuiltinKind>,
    /// For a class method written in an `implements I { … }` block: `I`'s
    /// qualified name, resolved through the constraint-head lowering
    /// (`resolve_ref_to_interface_identity`). `None` for free functions, plain
    /// methods, and interface methods (an interface method's owner is its
    /// enclosing `ExportedType::Interface`).
    pub interface_target: Option<QualifiedTypeName>,
    /// Stable, loc-free fully-qualified identifier: `pkg.ns….name`, methods
    /// qualified by their owner (`pkg.ns….Owner.name`) — the same dotted
    /// rendering as MIR's `ItemRef` `Display` for `Free`/`Method`. Two
    /// same-named methods (a plain method plus an `implements`-block one)
    /// share this string and are disambiguated by `interface_target`; MIR's
    /// implements-scoped symbol naming is reconstructed from the pair and its
    /// exported impl head by the consumer.
    pub callable_fqn: String,
}

/// One `implements` block exported loc-free (BEP-066 slice 6a) — the blob-side
/// mirror of [`crate::interfaces::ImplData`], carrying everything a source-less
/// consumer needs to run impl matching (`match_impl_head`-shaped unification),
/// bound discharge, membership, coherence, and dispatch without `ImplLoc`s.
///
/// Like `ImplData`, every impl — in-body or out-of-body — is normalized to the
/// same *free* shape: an in-body `implements I {…}` inside `class C<T>` exports
/// exactly as `implement<T> I for C<T>`. Patterns are `Ty` values over
/// [`generic_params`](Self::generic_params)' rigid `Ty::TypeVar`s (the same
/// currency `match_ty_patterns` keys on — deliberately NOT `TyTemplate`);
/// a consumer realizes a match by binding those params.
#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ExportedImpl {
    /// The implemented interface head at this impl's declared instantiation:
    /// qualified name, generic input args, and the impl's *realized*
    /// associated-type bindings (explicit pins plus filled declared defaults —
    /// the same rows [`associated_types`](Self::associated_types) carries,
    /// binding order canonicalized by `Interface::new`'s by-name sort). Args
    /// and bindings may mention `generic_params` as `Ty::TypeVar`s.
    pub interface: baml_type::Interface,
    /// The resolved implementor pattern (may carry `Ty::TypeVar`s over
    /// `generic_params` — a bare `TypeVar` is a blanket impl).
    pub for_ty_pattern: Ty,
    /// The impl's generic parameters (a class's own params for an in-body
    /// impl), in declaration order.
    pub generic_params: Vec<ParamTy>,
    /// Per-parameter interface-bound conjunctions, parallel to
    /// `generic_params` (`T extends A & B` yields two entries in `T`'s `Vec`;
    /// an unbounded parameter yields an empty one).
    pub param_bounds: Vec<Vec<baml_type::Interface>>,
    /// The impl's associated-type bindings in the interface's declaration
    /// order: explicit `type Item = …` pins plus declared defaults filled at
    /// this impl's receiver — exactly `ImplData::associated_types`.
    pub associated_types: Vec<(Name, Ty)>,
    /// Interface-field → class-field links declared in the block (in-body
    /// `implements` blocks only; same-named default coverage is not spelled
    /// out here, mirroring `ImplData::field_links`).
    pub field_links: Vec<(Name, Name)>,
    /// In-body vs out-of-body provenance. Diagnostic metadata ONLY — it MUST
    /// NOT drive resolution/dispatch/coherence (see
    /// [`crate::interfaces::InterfaceImplOrigin`], which this mirrors).
    pub origin: ExportedImplOrigin,
    /// The impl body's own method overrides, in declaration order. Inherited
    /// interface defaults are NOT merged in — the consumer merges them from
    /// the interface's own exported row, mirroring `ImplData::methods`.
    pub methods: Vec<ExportedImplMethod>,
}

/// Loc-free mirror of [`crate::interfaces::InterfaceImplOrigin`]: where an
/// impl was written. Diagnostic metadata ONLY — it MUST NOT drive
/// resolution/dispatch/coherence (a concrete class's out-of-body
/// `implement I for C` is merged onto `C` for resolution but keeps
/// `OutOfBody`, so out-of-body-only rules like E0126 still fire).
#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum ExportedImplOrigin {
    /// `implements I { … }` written in the class body.
    InBodyClass { class_qtn: QualifiedTypeName },
    /// `implement<…> I for <for_target>` — any out-of-body impl.
    OutOfBody,
}

/// One impl-body method override, exported with a *structural* identity: the
/// owner is the enclosing [`ExportedImpl`] itself (interface head + for-type),
/// and `name` picks the interface member it overrides. Deliberately NOT MIR's
/// `{iface-display}$for${target-display}` source-text naming — the consumer
/// reconstructs MIR's symbol from this structural pair. `sig.callable_fqn`
/// alone is NOT unique for a free-impl method (it renders owner-less,
/// `pkg.ns….name`); identity is `(enclosing impl, name)`.
#[derive(Debug, Clone, PartialEq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ExportedImplMethod {
    /// The overridden interface member's name (also `sig.name`).
    pub name: Name,
    /// The override's signature with `Self` realized to
    /// [`ExportedImpl::for_ty_pattern`]: lowered with `Self` as a rigid type
    /// variable bounded by the implemented interface (so `Self.member`
    /// projections resolve their declaring interface through that bound), then
    /// `Self → for_ty_pattern` substituted last — the exact
    /// `realize_with_symbolic_self` shape the conformance checker
    /// (`validate_impl_signatures`) lowers the override through.
    /// `interface_target` is always the implemented interface's qtn.
    pub sig: ExportedFunction,
}

/// Reconstruct the MIR owner segment for an out-of-body impl method from the
/// resolved impl head. This is the loc-free counterpart of the historical
/// source spelling `{interface-ref}$for${for-ref}`: qualified leaves are
/// rendered as they are addressable from the impl's package/namespace, while
/// all generic/container structure comes from [`Ty::render_with`]. Source and
/// mounted callers MUST use this same function so independently compiled
/// units agree on the B-693 symbol.
pub fn impl_method_symbol_owner(
    interface: &baml_type::Interface,
    for_ty: &Ty,
    package: &Name,
    namespace: &[Name],
) -> Name {
    struct ImplSymbolRender<'a> {
        package: &'a Name,
        namespace: &'a [Name],
    }

    impl baml_type::TyRenderStrategy for ImplSymbolRender<'_> {
        fn qtn(&self, qtn: &QualifiedTypeName) -> String {
            if qtn.package() != self.package {
                return qtn.render_dotted(false);
            }
            if qtn.namespace() == self.namespace {
                return qtn.name().to_string();
            }
            std::iter::once("root".to_string())
                .chain(qtn.namespace().iter().map(ToString::to_string))
                .chain(std::iter::once(qtn.name().to_string()))
                .collect::<Vec<_>>()
                .join(".")
        }

        fn type_var(&self, name: &Name) -> String {
            name.to_string()
        }
    }

    let renderer = ImplSymbolRender { package, namespace };
    // Associated-type values are consequences of the impl and may include
    // declaration defaults that were never written in the source-era symbol.
    // Coherence keys an impl on the interface head + for-type, so they are not
    // part of the B-693 owner segment.
    let interface_head = baml_type::Interface::new(
        interface.name.clone(),
        interface.generics.clone(),
        Vec::new(),
    );
    Name::new(format!(
        "{}$for${}",
        interface_head.to_ty().render_with(&renderer),
        for_ty.render_with(&renderer)
    ))
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

/// The mounted (source-less) package interface for `pkg_name`, or `None` when
/// `pkg_name` is not a mounted package (BEP-066 slice 6a). The single entry
/// point for foreign (blob-backed) lookups: callers holding a package-prefixed
/// path or a foreign `QualifiedTypeName` consult the blob's rows through this
/// instead of raw `package_items` (which is empty for a mounted package).
pub fn mounted_interface<'db>(
    db: &'db dyn crate::Db,
    pkg_name: &Name,
) -> Option<&'db PackageInterface> {
    if !baml_compiler2_hir::package::is_mounted_package(db, pkg_name) {
        return None;
    }
    Some(package_interface(db, PackageId::new(db, pkg_name.clone())))
}

/// Look up an exported type row in a mounted package by qualified name.
/// `None` when `qtn`'s package is not mounted or the row does not exist.
pub fn mounted_type_row<'db>(
    db: &'db dyn crate::Db,
    qtn: &QualifiedTypeName,
) -> Option<&'db ExportedType> {
    if let Some(row) = mounted_interface(db, qtn.package())
        .and_then(|iface| iface.lookup_type(qtn.namespace(), qtn.name()))
    {
        return Some(row);
    }
    // Runtime-minted names intentionally retain their internal `user.$dyn…`
    // identity even when a derived package view is mounted under an arbitrary
    // dependency alias. Search mounted blobs by that hidden identity rather
    // than rewriting the qtn (which would break runtime object identity).
    qtn.is_runtime_minted().then(|| {
        baml_compiler2_hir::package::mounted_package_names(db)
            .into_iter()
            .find_map(|package| {
                mounted_interface(db, &package)
                    .and_then(|iface| iface.lookup_type(qtn.namespace(), qtn.name()))
            })
    })?
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
            // Mirrors `def_to_ty`'s Interface arm (empty args/assoc — the
            // unspecialized declaration shape).
            ExportedType::Interface { qtn, .. } => {
                Ty::Interface(qtn.clone(), vec![], vec![], TyAttr::default())
            }
        }
    }
}

/// Assemble an [`ExportedFunction`] from the declaration-site signature query
/// plus the effective-throws oracle. The one place the two facts are paired.
fn exported_function<'db>(
    db: &'db dyn crate::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    name: &Name,
) -> ExportedFunction {
    let sig = crate::callable::function_signature_ty(db, func_loc);
    let bounds_map = crate::lower_type_expr::function_in_scope_generic_param_bounds(db, func_loc);
    ExportedFunction {
        name: name.clone(),
        params: sig.params.clone(),
        return_type: sig.return_type.clone(),
        declared_throws: sig.declared_throws.clone(),
        callable_throws: canonical_exported_throws(crate::callable::callable_throws(db, func_loc)),
        generic_param_bounds: sig
            .generic_params
            .iter()
            .map(|param| bounds_map.get(param).cloned().unwrap_or_default())
            .collect(),
        generic_params: sig.generic_params.clone(),
        builtin_kind: sig.builtin_kind,
        interface_target: method_implements_target_qtn(db, func_loc),
        callable_fqn: callable_fqn_of(db, func_loc),
    }
}

/// Remove the inference-only `Ty::Unknown` cycle sentinel from an exported
/// throws contract.
///
/// Throw inference computes a least fixpoint: when a recursive callable has no
/// other throw fact, its effect is `never`; inside a union, the recovery
/// sentinel contributes no fact. Salsa may expose that cycle seed through an
/// already-memoized inference query depending on which checking path demanded
/// the recursive method first. It must not cross the package-artifact boundary.
/// Source-written `throws unknown` is `Ty::BuiltinUnknown` and is deliberately
/// preserved here.
fn canonical_exported_throws(throws: &Ty) -> Ty {
    match throws {
        Ty::Unknown { attr } => Ty::Never { attr: attr.clone() },
        Ty::Union(members, attr) => {
            let mut members = members
                .iter()
                .filter(|member| !matches!(member, Ty::Unknown { .. }))
                .cloned()
                .collect::<Vec<_>>();
            match members.len() {
                0 => Ty::Never { attr: attr.clone() },
                1 => members.pop().expect("one throws union member"),
                _ => Ty::Union(members, attr.clone()),
            }
        }
        throws => throws.clone(),
    }
}

/// The interface a class method's `implements I { … }` block targets, resolved
/// to its qualified name through the constraint-head lowering (the arena twin
/// `impl_data` uses). `None` for anything but an implements-block method, or
/// when the target does not resolve to an interface (diagnosed on its own
/// path).
fn method_implements_target_qtn<'db>(
    db: &'db dyn crate::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> Option<QualifiedTypeName> {
    let target = baml_compiler2_ppir::item_data::method_interface_target(db, func_loc).as_ref()?;
    let pkg_info = file_package::file_package(db, func_loc.file(db));
    let pkg_items =
        baml_compiler2_ppir::package_items(db, PackageId::new(db, pkg_info.package.clone()));
    crate::interfaces::resolve_ref_to_interface_identity(
        db,
        &target.type_refs,
        target.target,
        pkg_items,
        &pkg_info.namespace_path,
    )
    .map(|resolved| resolved.qtn)
}

/// The stable loc-free fully-qualified name for a declared callable:
/// `pkg.ns….name`, with methods qualified by their owning class or interface
/// (`pkg.ns….Owner.name`) — the dotted rendering MIR's `ItemRef` `Display`
/// uses for `Free`/`Method`.
fn callable_fqn_of<'db>(
    db: &'db dyn crate::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> String {
    use baml_compiler2_ppir::item_data::{MethodOwner, method_owner};
    let pkg_info = file_package::file_package(db, func_loc.file(db));
    let name = baml_compiler2_ppir::item_data::function_data(db, func_loc)
        .name
        .clone();
    let owner = match method_owner(db, func_loc) {
        Some(MethodOwner::Class(class_loc)) => Some(
            baml_compiler2_ppir::item_data::class_data(db, class_loc)
                .name
                .clone(),
        ),
        Some(MethodOwner::Interface(iface_loc)) => Some(
            baml_compiler2_ppir::item_data::interface_data(db, iface_loc)
                .name
                .clone(),
        ),
        // A free-impl method renders owner-less (`pkg.ns….name`) — NOT unique
        // on its own. Its export identity is structural: the enclosing
        // `ExportedImpl` (interface head + for-type) plus the method name; the
        // consumer reconstructs MIR's `{iface}$for${target}`-scoped symbol
        // from that pair, never from this string.
        Some(MethodOwner::FreeImpl(_)) | None => None,
    };
    dotted_fqn(
        &pkg_info.package,
        &pkg_info.namespace_path,
        owner.as_ref(),
        &name,
    )
}

/// `pkg.ns….[owner.]name` — one join for every `callable_fqn` producer.
fn dotted_fqn(package: &Name, namespace: &[Name], owner: Option<&Name>, name: &Name) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(namespace.len() + 3);
    parts.push(package.as_str());
    parts.extend(namespace.iter().map(Name::as_str));
    if let Some(owner) = owner {
        parts.push(owner.as_str());
    }
    parts.push(name.as_str());
    parts.join(".")
}

/// Extract the span-free `@alias`/`@description` schema attributes from a
/// field's HIR attributes — the same reading as emit's `extract_schema_attrs`
/// (invalid usage is diagnosed at HIR validation; malformed entries are
/// skipped).
fn exported_field_attrs(attrs: &[baml_compiler2_hir::item_tree::Attribute]) -> ExportedFieldAttrs {
    let mut out = ExportedFieldAttrs::default();
    for attr in attrs {
        match attr.name.as_str() {
            "description" | "alias" if attr.args.len() == 1 => {
                let value =
                    baml_compiler2_ast::parse_string_attr_value(attr.args[0].value.as_str());
                if attr.name.as_str() == "description" {
                    out.description = value;
                } else {
                    out.alias = value;
                }
            }
            _ => {}
        }
    }
    out
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
    let class_generic_env = crate::generic_env::class_generic_env(db, class_loc);
    let class_ns = file_package::file_package(db, class_loc.file(db)).namespace_path;

    // Lower fields. The class's type-variable bounds let an associated-type
    // projection `T.member` in a field type resolve `T`'s declaring interface.
    let field_scope = crate::lower_type_expr::ScopeCtx {
        db,
        package_items: pkg_items,
        ns_context: &class_ns,
        generic_params: class_generic_env.source_params(),
        bounds: crate::lower_type_expr::class_generic_param_bounds(db, class_loc),
        self_ty: None,
    };
    let mut fields = Vec::new();
    let mut diags = Vec::new();
    for field in &class_data.fields {
        let field_ty = crate::lower_type_expr::lower_type_ref(
            &class_data.type_refs,
            field.type_ref,
            &field_scope,
            &mut diags,
        );
        fields.push((
            field.name.clone(),
            field_ty,
            exported_field_attrs(&field.attributes),
        ));
    }

    // Lower methods
    let mut methods = Vec::new();
    for &method_loc in &class_data.methods {
        let method_data = baml_compiler2_ppir::item_data::function_data(db, method_loc);
        methods.push(exported_function(db, method_loc, &method_data.name));
    }

    let generic_params = class_generic_env.params().to_vec();
    let bounds_map = crate::lower_type_expr::class_generic_param_bounds(db, class_loc);
    let generic_param_bounds = generic_params
        .iter()
        .map(|param| bounds_map.get(param).cloned().unwrap_or_default())
        .collect();

    let qtn = qualify_def(db, Definition::Class(class_loc), name);
    ExportedType::Class {
        qtn,
        fields,
        methods,
        generic_params,
        generic_param_bounds,
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

/// Lower an interface definition into its `ExportedType::Interface` (BEP-066
/// slice 6a). Reuses the declaration-scope substrate `interfaces.rs` already
/// maintains — nothing here lowers a `TypeRef` on its own:
///
/// - `requires`: [`crate::interfaces::interface_requires_closure_symbolic`]
///   (the closure walker at identity arguments, symbolic `Self`);
/// - associated types: [`crate::interfaces::interface_associated_type_default`]
///   for defaults, [`associated_type_declared_bound`] for bounds — both keep
///   `Self` symbolic;
/// - fields: [`crate::interfaces::resolve_interface_fields`];
/// - methods: [`crate::interfaces::resolve_interface_required_methods`] and
///   [`crate::interfaces::resolve_interface_default_methods`].
///
/// [`associated_type_declared_bound`]: crate::builder::associated_projection::associated_type_declared_bound
fn lower_interface_export<'db>(
    db: &'db dyn crate::Db,
    iface_loc: InterfaceLoc<'db>,
    name: &Name,
) -> ExportedType {
    let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    let qtn = qualify_def(db, Definition::Interface(iface_loc), name);

    let env = crate::generic_env::interface_generic_env(db, iface_loc);
    let (self_param, declared_params) = env.interface_param_parts();
    let self_param = self_param.clone();
    let generic_params = declared_params.to_vec();
    let identity_args: Vec<Ty> = generic_params
        .iter()
        .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
        .collect();

    let bounds_map = crate::lower_type_expr::interface_generic_param_bounds(db, iface_loc);
    let param_bounds: Vec<Vec<baml_type::Interface>> = generic_params
        .iter()
        .map(|param| bounds_map.get(param).cloned().unwrap_or_default())
        .collect();

    let requires = crate::interfaces::interface_requires_closure_symbolic(db, iface_loc);

    // The interface at its identity instantiation (no pins) — the qualifier
    // `associated_type_declared_bound` realizes each bound against.
    let identity_iface = baml_type::Interface::new(qtn.clone(), identity_args, Vec::new());
    let associated_types = iface
        .associated_types
        .iter()
        .map(|assoc| ExportedAssociatedType {
            name: assoc.name.clone(),
            bound: assoc.bound.and_then(|_| {
                crate::builder::associated_projection::associated_type_declared_bound(
                    db,
                    &identity_iface,
                    &assoc.name,
                )
                .into_iter()
                .next()
            }),
            default: crate::interfaces::interface_associated_type_default(
                db,
                iface_loc,
                assoc.name.clone(),
            )
            .map(|(ty, _diags)| ty),
        })
        .collect();

    // `resolve_interface_fields` lowers `iface.fields` in order, so the two
    // run parallel. Class-field AST lowering hoists `@alias`/`@description`
    // off the outer type expression into `FieldData::attributes`; the
    // interface-field lowering does not, leaving them on the field's outer
    // `TypeRef` — read both homes so the export is complete either way.
    let resolved_fields = crate::interfaces::resolve_interface_fields(db, iface_loc);
    debug_assert_eq!(
        resolved_fields.fields.len(),
        iface.fields.len(),
        "resolve_interface_fields lowers `iface.fields` in order, so the two lists run parallel"
    );
    let fields = resolved_fields
        .fields
        .iter()
        .zip(&iface.fields)
        .map(|((field_name, ty, attrs), field_data)| {
            let mut exported = exported_field_attrs(attrs);
            let type_attrs = exported_field_attrs(&iface.type_refs[field_data.type_ref].attrs);
            exported.alias = exported.alias.or(type_attrs.alias);
            exported.description = exported.description.or(type_attrs.description);
            (field_name.clone(), ty.clone(), exported)
        })
        .collect();

    // Required and default methods, each list in declaration order — exported
    // split (as `InterfaceData` splits them) so a source-less consumer can run
    // conformance (E0113) and default fallback.
    let mut required_methods = Vec::new();
    let resolved_required = crate::interfaces::resolve_interface_required_methods(db, iface_loc);
    for (sig, resolved) in iface.required_methods.iter().zip(resolved_required) {
        // A required method has no body: its effective throws contract is its
        // declared clause (`unknown` when unwritten — `lower_signature`'s
        // `Missing` slot convention, which `function_ty` already encodes).
        if let Some(exported) =
            exported_interface_method(&qtn, resolved, sig.throws.is_some(), None, None)
        {
            required_methods.push(exported);
        }
    }
    let mut default_methods = Vec::new();
    let resolved_default = crate::interfaces::resolve_interface_default_methods(db, iface_loc);
    for (&func_loc, resolved) in iface.default_methods.iter().zip(resolved_default) {
        let declared_throws_written =
            baml_compiler2_ppir::item_data::elaborated_function_data(db, func_loc)
                .throws
                .is_some();
        // A default method has a body: pair the symbolic signature with the
        // same effective-throws oracle every exported function carries.
        let callable_throws =
            canonical_exported_throws(crate::callable::callable_throws(db, func_loc));
        let builtin_kind = crate::callable::function_signature_ty(db, func_loc).builtin_kind;
        if let Some(exported) = exported_interface_method(
            &qtn,
            resolved,
            declared_throws_written,
            Some(callable_throws),
            builtin_kind,
        ) {
            default_methods.push(exported);
        }
    }

    ExportedType::Interface {
        qtn,
        self_param,
        generic_params,
        param_bounds,
        requires,
        associated_types,
        fields,
        required_methods,
        default_methods,
    }
}

/// Repackage a [`ResolvedInterfaceMethod`](crate::interfaces::ResolvedInterfaceMethod)
/// (symbolic-`Self` declaration surface) as an [`ExportedFunction`].
/// `callable_throws` is `None` for a required method (the declared clause *is*
/// the contract) and the body-inferred oracle value for a default method.
/// `None` only if the resolved surface is not a `Ty::Function` (an internal
/// invariant break — the row is dropped rather than fabricated).
fn exported_interface_method(
    iface_qtn: &QualifiedTypeName,
    resolved: &crate::interfaces::ResolvedInterfaceMethod,
    declared_throws_written: bool,
    callable_throws: Option<Ty>,
    builtin_kind: Option<BuiltinKind>,
) -> Option<ExportedFunction> {
    let Ty::Function {
        params,
        ret,
        throws,
        ..
    } = &resolved.function_ty
    else {
        return None;
    };
    Some(ExportedFunction {
        name: resolved.name.clone(),
        params: params.clone(),
        return_type: (**ret).clone(),
        declared_throws: declared_throws_written.then(|| (**throws).clone()),
        callable_throws: callable_throws.unwrap_or_else(|| (**throws).clone()),
        generic_params: resolved
            .generic_params
            .iter()
            .map(|(param, _)| param.clone())
            .collect(),
        generic_param_bounds: resolved
            .generic_params
            .iter()
            .map(|(_, bounds)| bounds.clone())
            .collect(),
        builtin_kind,
        interface_target: None,
        callable_fqn: dotted_fqn(
            iface_qtn.package(),
            iface_qtn.namespace(),
            Some(iface_qtn.name()),
            &resolved.name,
        ),
    })
}

/// Lower a free-function definition into its `ExportedFunction`.
fn lower_function_export<'db>(
    db: &'db dyn crate::Db,
    func_loc: FunctionLoc<'db>,
    name: &Name,
) -> ExportedFunction {
    exported_function(db, func_loc, name)
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
    // resolver's within-file choice exactly. A first contribution of any other
    // definition kind still *claims* the name — leaving no structural export —
    // matching the reference derivation's `_ => continue`.
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
            Definition::Interface(iface_loc) => lower_interface_export(db, iface_loc, name),
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

    // Mounted-package arm (BEP-066 slice 6a): a source-less dependency mounted
    // as a blob serves its captured interface verbatim — the same seed
    // mechanism generalized to any (non-reserved) alias. Distinct from the
    // stdlib seed above: the stdlib map is an *optimization* over packages
    // whose source is present, while a mounted package has NO files, so this
    // arm is its only interface. `is_mounted_package` filters reserved names,
    // so a blob can never shadow the stdlib or the user package. A corrupt
    // blob falls through to honest derivation, which is empty for a file-less
    // package — every reference then fails with the ordinary unresolved
    // diagnostics rather than a panic.
    if baml_compiler2_hir::package::is_mounted_package(db, &pkg_name) {
        if let Some(mounted) = db.mounted_packages() {
            if let Some(bytes) = mounted.by_package(db).get(pkg_name.as_str()) {
                if let Ok(iface) = borsh::from_slice::<PackageInterface>(bytes) {
                    return iface;
                }
            }
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

/// Lower every resolvable `implements` block of `pkg_id` into its
/// [`ExportedImpl`] row (BEP-066 slice 6a).
///
/// Enumeration rides [`crate::interfaces::package_impl_locs`] (the canonical
/// substrate); per-impl facts come from [`crate::interfaces::impl_data`]
/// verbatim. A malformed impl (`Err(ImplDataError)`) is skipped exactly as
/// every resolution consumer skips it (`impls_for_type`,
/// `get_implements_block`, coherence all filter with `let Ok(data) = …`);
/// its diagnostics stay owned by `impl_data` and surfaced by check.rs, so
/// skipping here loses no signal — an unresolvable impl has no resolved facts
/// to export.
///
/// Rows sort by their borsh encoding: a canonical total order over the full
/// row content, independent of `compiler2_all_files` enumeration order (the
/// two-database determinism requirement); ties are byte-identical rows.
fn exported_impls<'db>(db: &'db dyn crate::Db, pkg_id: PackageId<'db>) -> Vec<ExportedImpl> {
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let mut rows: Vec<ExportedImpl> = Vec::new();
    for &impl_loc in crate::interfaces::package_impl_locs(db, pkg_id) {
        let Ok(data) = crate::interfaces::impl_data(db, impl_loc).as_ref() else {
            continue;
        };
        let Some(iface_qtn) = data.interface_qtn(db) else {
            continue;
        };
        // The impl's own file namespace — the scope its block lowered in
        // (matches `impl_data`'s `ns`); `package_impl_locs` already filtered
        // the file to this package.
        let ns = file_package::file_package(db, impl_loc.file(db)).namespace_path;

        let (generic_params, param_bounds): (Vec<ParamTy>, Vec<Vec<baml_type::Interface>>) =
            data.generic_params.iter().cloned().unzip();
        let methods = data
            .methods
            .iter()
            .filter_map(|&method_loc| {
                exported_impl_method(db, pkg_items, &ns, data, &iface_qtn, method_loc)
            })
            .collect();

        rows.push(ExportedImpl {
            interface: baml_type::Interface::new(
                iface_qtn,
                data.interface_args.clone(),
                data.associated_types.clone(),
            ),
            for_ty_pattern: data.for_ty_pattern.clone(),
            generic_params,
            param_bounds,
            associated_types: data.associated_types.clone(),
            field_links: data.field_links.clone(),
            origin: match &data.origin {
                crate::interfaces::InterfaceImplOrigin::InBodyClass { class_qtn } => {
                    ExportedImplOrigin::InBodyClass {
                        class_qtn: class_qtn.clone(),
                    }
                }
                crate::interfaces::InterfaceImplOrigin::OutOfBody => ExportedImplOrigin::OutOfBody,
            },
            methods,
        });
    }
    rows.sort_by_cached_key(|row| borsh::to_vec(row).expect("ExportedImpl serializes with borsh"));
    rows
}

/// Lower one impl-body method override into its [`ExportedImplMethod`].
///
/// The signature is lowered exactly as the conformance checker lowers the
/// override's side (`validate_impl_signatures`): the impl's generics plus the
/// method's own join the scope, the method's own bounds lower through
/// [`lower_generic_param_interface_bounds`] (joining the bounds map so a
/// `T.member` projection in the signature resolves through `T`'s bound), and
/// `Self` is realized through
/// [`realize_with_symbolic_self`] — a rigid type
/// variable bounded by the implemented interface at the impl's args, with
/// `Self → for_ty_pattern` substituted last. This deliberately does NOT reuse
/// [`crate::callable::function_signature_ty`], which lowers a free-impl method
/// *without* a `Self` binding (its `Self` mentions become `Ty::Error` — see
/// that query's doc); the throws contract still pairs with the same
/// [`crate::callable::callable_throws`] oracle every exported function
/// carries. `None` only if the lowered surface is not a `Ty::Function` (an
/// internal invariant break — the row is dropped rather than fabricated).
///
/// [`lower_generic_param_interface_bounds`]: crate::interfaces::lower_generic_param_interface_bounds
/// [`realize_with_symbolic_self`]: crate::interfaces::realize_with_symbolic_self
fn exported_impl_method<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    ns: &[Name],
    data: &crate::interfaces::ImplData<'db>,
    iface_qtn: &QualifiedTypeName,
    method_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> Option<ExportedImplMethod> {
    use crate::builder::interface_resolution::InterfaceMethodSpec;

    let name = baml_compiler2_ppir::item_data::function_data(db, method_loc)
        .name
        .clone();
    let spec = InterfaceMethodSpec::from_default(db, method_loc);

    // The impl's generics plus the method's own, with the method's bounds
    // joining the map before the signature lowers (the
    // `resolve_interface_method_spec` shape).
    let impl_param_names: Vec<ParamTy> = data
        .generic_params
        .iter()
        .map(|(param, _)| param.clone())
        .collect();
    // Unlike `InterfaceMethodSpec`, whose generic list is the user-written
    // declaration, the function environment also contains signature
    // elaboration's synthetic callback-effect params. They must cross the blob
    // boundary too: source and mounted call inference consult the same free
    // type variables even though runtime layout later erases them.
    let impl_param_count = impl_param_names.len();
    let scope_generics = crate::function_generic_params(db, method_loc);
    let impl_params = scope_generics.get(..impl_param_count)?;
    debug_assert_eq!(
        impl_params,
        impl_param_names.as_slice(),
        "an impl method's function environment must begin with the impl parameters"
    );
    let own_params = scope_generics.get(impl_param_count..)?;
    let mut bounds: crate::lower_type_expr::TypeVarBoundsMap =
        data.generic_params.iter().cloned().collect();
    // Lowering diagnostics are dropped, matching every export path: the
    // checked diagnostics for an impl are owned by `impl_data` +
    // `validate_impl_signatures`.
    let mut diags = Vec::new();
    let mut generic_params = Vec::new();
    let mut generic_param_bounds = Vec::new();
    let declared_count = spec.generic_bounds().len();
    let declared_params = own_params.get(..declared_count)?;
    for (param, declared) in declared_params.iter().zip(spec.generic_bounds()) {
        let ifaces = crate::interfaces::lower_generic_param_interface_bounds(
            db,
            spec.bound_store(),
            &declared.bounds,
            pkg_items,
            ns,
            &scope_generics,
            &mut diags,
        );
        bounds.insert(param.clone(), ifaces.clone());
        generic_params.push(param.clone());
        generic_param_bounds.push(ifaces);
    }
    let synthetic_params = own_params.get(declared_count..)?;
    for param in synthetic_params {
        bounds.insert(param.clone(), Vec::new());
        generic_params.push(param.clone());
        generic_param_bounds.push(Vec::new());
    }

    // The interface's symbolic `Self` parameter: from its generic env when the
    // target is source-backed, from the mounted row otherwise (a user impl of
    // a mounted interface still exports — its overrides are source-backed).
    let iface_self_param: ParamTy = match &data.interface {
        crate::interfaces::ImplInterfaceTarget::Source(loc) => {
            crate::generic_env::interface_generic_env(db, *loc)
                .interface_param_parts()
                .0
                .clone()
        }
        crate::interfaces::ImplInterfaceTarget::Mounted(qtn) => {
            match crate::package_interface::mounted_type_row(db, qtn) {
                Some(ExportedType::Interface { self_param, .. }) => self_param.clone(),
                _ => return None,
            }
        }
    };
    let self_bound =
        baml_type::Interface::new(iface_qtn.clone(), data.interface_args.clone(), Vec::new());
    let realized = crate::interfaces::realize_with_symbolic_self(
        db,
        pkg_items,
        ns,
        &scope_generics,
        &iface_self_param,
        &bounds,
        &self_bound,
        &data.for_ty_pattern,
        &crate::unify::TypeBindings::default(),
        |scope| spec.to_function_ty(scope, &mut diags),
    );
    let Ty::Function {
        params,
        ret,
        throws,
        ..
    } = realized
    else {
        return None;
    };

    let declared_throws_written =
        baml_compiler2_ppir::item_data::elaborated_function_data(db, method_loc)
            .throws
            .is_some();
    let builtin_kind = match baml_compiler2_ppir::function_body(db, method_loc).as_ref() {
        baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
        _ => None,
    };
    Some(ExportedImplMethod {
        name: name.clone(),
        sig: ExportedFunction {
            name,
            params,
            return_type: (*ret).clone(),
            // The lowered throws slot is the declared clause (`unknown` when
            // unwritten — `lower_signature`'s `Missing` convention); the
            // effective contract is the body-inferred oracle, as everywhere.
            declared_throws: declared_throws_written.then(|| (*throws).clone()),
            callable_throws: canonical_exported_throws(crate::callable::callable_throws(
                db, method_loc,
            )),
            generic_params,
            generic_param_bounds,
            builtin_kind,
            interface_target: Some(iface_qtn.clone()),
            callable_fqn: callable_fqn_of(db, method_loc),
        },
    })
}

/// Fold each file's `file_interface_fragment` into the whole-package interface.
///
/// Winner selection is driven by the resolved `pkg_items.namespaces` (the
/// deterministic `contribs[0]` pick); per-item *lowering* lives in
/// `file_interface_fragment`. The impls table is package-level (impls have no
/// export name, so the name-keyed fragment fold doesn't fit them) and derives
/// from the `package_impl_locs` substrate directly.
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

    // The FULL namespace set — every namespace `package_items` resolves,
    // including ones with no exported type or function. `BTreeSet` for a
    // deterministic (and borsh-stable) order regardless of the map's hash
    // iteration.
    let namespaces: BTreeSet<Vec<Name>> = pkg_items.namespaces.keys().cloned().collect();

    let throw_sets = function_throw_sets(db, pkg_id);
    PackageInterface {
        types,
        functions,
        throw_sets: throw_sets.clone(),
        namespaces,
        impls: exported_impls(db, pkg_id),
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
                        // Interface rows resolve only for MOUNTED (source-less)
                        // dependencies (BEP-066 slice 6a) — their rows are the
                        // sole representation. A source-backed dependency's
                        // interface rows stay invisible here, preserving the
                        // pre-export resolution (its interfaces resolve through
                        // source items on the loc path).
                        if !matches!(exported, ExportedType::Interface { .. })
                            || baml_compiler2_hir::package::is_mounted_package(db, dep_name)
                        {
                            return Some((ResolvedSource::Builtin, exported.to_ty()));
                        }
                    }
                }
            }
        }

        // BEP-066 keyword shorthand: a leading `reflect` resolves as
        // `baml.reflect`. Strictly after every user lookup above, so any
        // user-defined `reflect` shadows the shorthand.
        if path.len() >= 2 && path[0].as_str() == "reflect" && self.can_access_baml_package() {
            let namespace = &path[..path.len() - 1];
            if self.own_package_name.as_str() == "baml" {
                if let Some(def) = self.own_items.lookup_type(namespace, item) {
                    return Some((ResolvedSource::Builtin, def_to_ty(db, def)));
                }
            } else if let Some(exported) = self
                .baml_dependency_interface()
                .and_then(|interface| interface.lookup_type(namespace, item))
            {
                return Some((ResolvedSource::Builtin, exported.to_ty()));
            }
        }

        None
    }

    /// Whether the `baml` package's items are visible from this package (it is
    /// the own package or a declared dependency) — the precondition for the
    /// BEP-066 keyword shorthands (`reflect` ≡ `baml.reflect`, `type` ≡
    /// `baml.type`) to resolve.
    fn can_access_baml_package(&self) -> bool {
        self.own_package_name.as_str() == "baml" || self.baml_dependency_interface().is_some()
    }

    fn baml_dependency_interface(&self) -> Option<&PackageInterface> {
        self.dep_interfaces
            .iter()
            .find(|(name, _)| name.as_str() == "baml")
            .map(|(_, interface)| interface)
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
        for (dep_name, dep_iface) in &self.dep_interfaces {
            if let Some(exported) = dep_iface.lookup_type(namespace, item) {
                // See the interface-row gate in `resolve_type`: interface rows
                // resolve only for mounted dependencies.
                if !matches!(exported, ExportedType::Interface { .. })
                    || baml_compiler2_hir::package::is_mounted_package(db, dep_name)
                {
                    return Some((ResolvedSource::Builtin, exported.to_ty()));
                }
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

        // BEP-066 keyword shorthands: a leading `reflect` resolves as
        // `baml.reflect`, a leading `type` (the `type.of` / `type.of_value`
        // expression form, K-13) as `baml.type`. Strictly after every user
        // lookup above, so any user-defined name shadows the shorthand.
        if path.len() >= 2
            && matches!(path[0].as_str(), "reflect" | "type" | "json")
            && self.can_access_baml_package()
        {
            let namespace = &path[..path.len() - 1];
            if self.own_package_name.as_str() != "baml"
                && self
                    .baml_dependency_interface()
                    .and_then(|interface| interface.lookup_function(namespace, item))
                    .is_none()
            {
                return None;
            }
            // Value consumers still need the source-backed `Definition` loc.
            // The interface lookup above is the visibility gate; raw items are
            // consulted only to materialize a function already proven exported.
            let baml_items =
                baml_compiler2_ppir::package_items(db, PackageId::new(db, Name::new("baml")));
            if let Some(def) = baml_items.lookup_value(namespace, item) {
                return Some((ResolvedSource::Builtin, def));
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

        for &method_loc in &class_data.methods {
            let method_data = baml_compiler2_ppir::item_data::function_data(db, method_loc);
            if &method_data.name != method_name {
                continue;
            }
            let exported = exported_function(db, method_loc, &method_data.name);

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
                class_generic_params: crate::generic_env::class_generic_env(db, class_loc)
                    .params()
                    .to_vec(),
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
            let args = crate::generic_env::class_generic_env(db, loc)
                .params()
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
