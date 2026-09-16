//! The External lane of [`DeclRef`]: identity for declarations a package
//! exports WITHOUT source.
//!
//! A package served from its compiler interface (a runtime mount, the
//! precompiled stdlib in a runtime compile, later a cached dependency) has
//! ROWS, not items: the `ExportedFunction` / `ExportedImpl` rows of its
//! [`PackageInterface`](crate::package_interface::PackageInterface). Before
//! this module a mounted callable had no identity — consumers carried a name
//! bundle ([`ExternalCallTarget`]) plus a copied descriptor, and every
//! consultation re-resolved the bundle by name with an `Option`. Here a row
//! gets an INTERNED identity, minted ONCE from the lookup key by the mint
//! fns at the bottom of this module; everything after compares ids and reads
//! the row through [`extern_function_row`], which is TOTAL — a minted loc
//! came from a row in this revision's interface input, so a miss is an
//! internal error, never a silent "not a member". Every fact of the row is
//! read off that one query — there are no per-field accessors to drift.
//!
//! The lane is keyed on the ROOT, never on a root kind: trust facts (a row's
//! `linkability`, `builtin_kind`) stay row data read by the existing gates,
//! so a cached external dependency served from its interface rides the same
//! lane as a runtime mount. Nothing here consults `is_served_from_interface`
//! either — that gate belongs to the resolution LADDER that decides which
//! lane to ask first, and an interface derived from source is addressable
//! too (the incomplete-impl recovery road reads a source interface's
//! bodyless default through this lane).

use std::borrow::Cow;

use baml_base::{Name, SourceRoot};
use baml_compiler2_hir::loc::{ClassLoc, DeclRef, EnumLoc, FunctionLoc, ImplLoc, InterfaceLoc};
use baml_type::{
    DeclName, ParamTy,
    interned::{ClosedInterface, ClosedTy},
};
use rustc_hash::FxHashMap;

use crate::{
    callable::{ExternalCallTarget, FunctionSignatureTy},
    impls::{MountedImplFacts, ResolvedImplFacts},
    package_interface::{
        ExportedAssociatedType, ExportedFieldAttrs, ExportedFunction, ExportedImpl, ExportedType,
        package_interface,
    },
    render::{Spell, Viewpoint},
};

/// A function by provenance: a source item or an exported row.
pub type FunctionRef<'db> = DeclRef<FunctionLoc<'db>, ExternFunctionLoc<'db>>;
/// A class, wherever it is declared.
pub type ClassRef<'db> = DeclRef<ClassLoc<'db>, ExternClassLoc<'db>>;
/// An enum, wherever it is declared.
pub type EnumRef<'db> = DeclRef<EnumLoc<'db>, ExternEnumLoc<'db>>;
/// An interface, wherever it is declared.
pub type InterfaceRef<'db> = DeclRef<InterfaceLoc<'db>, ExternInterfaceLoc<'db>>;
/// An `implements` block, wherever it is declared.
pub type ImplRef<'db> = DeclRef<ImplLoc<'db>, ExternImplLoc<'db>>;

// ── Type rows ────────────────────────────────────────────────────────────────
//
// A type row is addressed by its head — the one lane-uniform identity types
// already have — but as an INTERNED loc minted only from a row that exists,
// so that a `DeclName` fabricated anywhere (`DeclName::in_root(..)`) cannot
// pose as a declaration. One total read per kind hands the whole row back.

/// The identity of an exported class row.
#[salsa::interned]
pub struct ExternClassLoc<'db> {
    #[returns(ref)]
    pub head: DeclName,
}

/// The identity of an exported enum row.
#[salsa::interned]
pub struct ExternEnumLoc<'db> {
    #[returns(ref)]
    pub head: DeclName,
}

/// The identity of an exported interface row.
#[salsa::interned]
pub struct ExternInterfaceLoc<'db> {
    #[returns(ref)]
    pub head: DeclName,
}

impl std::fmt::Debug for ExternClassLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExternClassLoc(..)")
    }
}

impl std::fmt::Debug for ExternEnumLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExternEnumLoc(..)")
    }
}

impl std::fmt::Debug for ExternInterfaceLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExternInterfaceLoc(..)")
    }
}

/// An exported class row, borrowed whole from the package interface. A row
/// is data only: its identity is the loc it was read through, never a field
/// the blob controls.
#[derive(Clone, Copy)]
pub struct ClassRow<'db> {
    pub fields: &'db [(Name, baml_type::Ty, ExportedFieldAttrs)],
    pub methods: &'db [ExportedFunction],
    pub generic_params: &'db [ParamTy],
    pub generic_param_bounds: &'db [Vec<baml_type::Interface>],
}

/// An exported enum row, borrowed whole from the package interface. Data
/// only, as [`ClassRow`].
#[derive(Clone, Copy)]
pub struct EnumRow<'db> {
    pub variants: &'db [Name],
}

/// An exported interface row, borrowed whole from the package interface.
/// Data only, as [`ClassRow`].
#[derive(Clone, Copy)]
pub struct InterfaceRow<'db> {
    pub self_param: &'db ParamTy,
    pub generic_params: &'db [ParamTy],
    pub param_bounds: &'db [Vec<baml_type::Interface>],
    pub requires: &'db [baml_type::Interface],
    pub associated_types: &'db [ExportedAssociatedType],
    pub fields: &'db [(Name, baml_type::Ty, ExportedFieldAttrs)],
    pub required_methods: &'db [ExportedFunction],
    pub default_methods: &'db [ExportedFunction],
}

fn no_type_row(db: &dyn baml_compiler2_ppir::Db, kind: &str, head: &DeclName) -> ! {
    panic!(
        "internal error: no exported {kind} row for `{}`; an extern type loc is minted only from          a row in the current revision's package interface",
        head.spell(&Viewpoint::canonical(db))
    )
}

/// The row `class` names. TOTAL, as [`extern_function_row`].
pub fn extern_class_row<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: ExternClassLoc<'db>,
) -> ClassRow<'db> {
    let head = class.head(db);
    match type_row_at(db, head) {
        Some(ExportedType::Class {
            qtn: _,
            fields,
            methods,
            generic_params,
            generic_param_bounds,
        }) => ClassRow {
            fields,
            methods,
            generic_params,
            generic_param_bounds,
        },
        _ => no_type_row(db, "class", head),
    }
}

/// The row `enum_loc` names. TOTAL, as [`extern_function_row`].
pub fn extern_enum_row<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    enum_loc: ExternEnumLoc<'db>,
) -> EnumRow<'db> {
    let head = enum_loc.head(db);
    match type_row_at(db, head) {
        Some(ExportedType::Enum { qtn: _, variants }) => EnumRow { variants },
        _ => no_type_row(db, "enum", head),
    }
}

/// The row `interface` names. TOTAL, as [`extern_function_row`].
pub fn extern_interface_row<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: ExternInterfaceLoc<'db>,
) -> InterfaceRow<'db> {
    let head = interface.head(db);
    match type_row_at(db, head) {
        Some(ExportedType::Interface {
            qtn: _,
            self_param,
            generic_params,
            param_bounds,
            requires,
            associated_types,
            fields,
            required_methods,
            default_methods,
        }) => InterfaceRow {
            self_param,
            generic_params,
            param_bounds,
            requires,
            associated_types,
            fields,
            required_methods,
            default_methods,
        },
        _ => no_type_row(db, "interface", head),
    }
}

/// The class `head` names, if its package exports one.
pub fn extern_class_loc<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    head: &DeclName,
) -> Option<ExternClassLoc<'db>> {
    matches!(type_row_at(db, head)?, ExportedType::Class { .. })
        .then(|| ExternClassLoc::new(db, head.clone()))
}

/// The enum `head` names, if its package exports one.
pub fn extern_enum_loc<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    head: &DeclName,
) -> Option<ExternEnumLoc<'db>> {
    matches!(type_row_at(db, head)?, ExportedType::Enum { .. })
        .then(|| ExternEnumLoc::new(db, head.clone()))
}

/// The interface `head` names, if its package exports one.
pub fn extern_interface_loc<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    head: &DeclName,
) -> Option<ExternInterfaceLoc<'db>> {
    matches!(type_row_at(db, head)?, ExportedType::Interface { .. })
        .then(|| ExternInterfaceLoc::new(db, head.clone()))
}

/// [`extern_class_loc`] for a package served from its interface.
pub fn mounted_class_loc<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    head: &DeclName,
) -> Option<ExternClassLoc<'db>> {
    served(db, head.root()).then(|| extern_class_loc(db, head))?
}

/// [`extern_enum_loc`] for a package served from its interface.
pub fn mounted_enum_loc<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    head: &DeclName,
) -> Option<ExternEnumLoc<'db>> {
    served(db, head.root()).then(|| extern_enum_loc(db, head))?
}

/// [`extern_interface_loc`] for a package served from its interface.
pub fn mounted_interface_loc<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    head: &DeclName,
) -> Option<ExternInterfaceLoc<'db>> {
    served(db, head.root()).then(|| extern_interface_loc(db, head))?
}

// ── Impl identity ────────────────────────────────────────────────────────────

/// An implementation block's identity: every input coherence discriminates
/// on, nothing else — the structural twin of emit's `ImplCoherenceKey`,
/// spelled in `hir_ty`'s own vocabulary so a source block and its exported
/// row canonicalize to ONE value ([`impl_identity`] is the one
/// canonicalizer).
///
/// Parameter NAMES are canonicalized away: `ParamTy`'s equality includes
/// the name, and a written rename must not fork the identity, so every
/// `TypeVar` is respelled by its frame index. Each parameter's bound
/// conjunction is sorted by `ClosedInterface`'s `Ord` (a written reorder of
/// `A + B` cannot fork it either). The TARGET's own associated pins are
/// EXCLUDED — they are outputs of a match, not inputs to admissibility —
/// while bound-side pins ride inside each bound, as inputs.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub struct ImplIdentity {
    /// The implemented interface's head.
    pub interface: DeclName,
    /// The interface's arguments, canonicalized.
    pub interface_args: Vec<ClosedTy>,
    /// The `for` target pattern, canonicalized.
    pub for_ty_pattern: ClosedTy,
    /// Per impl-frame parameter in frame order, that parameter's bounds,
    /// canonically sorted.
    pub bounds: Vec<Vec<ClosedInterface>>,
}

/// The identity of a resolved impl, from whichever lane its facts came.
pub fn impl_identity(facts: &ResolvedImplFacts<'_>) -> ImplIdentity {
    impl_identity_of(
        facts.interface(),
        facts.for_ty_pattern(),
        facts.generic_params(),
    )
}

/// The identity of an exported impl row: the same canonicalization as
/// [`impl_identity`], over the row's facts.
pub fn exported_impl_identity(row: &ExportedImpl) -> ImplIdentity {
    let facts = crate::impls::exported_impl_facts(row);
    impl_identity_of(
        &facts.interface,
        &facts.for_ty_pattern,
        &facts.generic_params,
    )
}

fn impl_identity_of(
    interface: &ClosedInterface,
    for_ty_pattern: &ClosedTy,
    generic_params: &[(ParamTy, Vec<ClosedInterface>)],
) -> ImplIdentity {
    ImplIdentity {
        interface: interface.name.clone(),
        interface_args: interface
            .generics
            .iter()
            .map(|arg| {
                let arg = ClosedTy::try_from(arg).unwrap_or_else(|_| {
                    unreachable!("ClosedInterface invariant: every carried type is closed")
                });
                canonical_closed_ty(&arg)
            })
            .collect(),
        for_ty_pattern: canonical_closed_ty(for_ty_pattern),
        bounds: generic_params
            .iter()
            .map(|(_, bounds)| {
                let mut bounds: Vec<ClosedInterface> =
                    bounds.iter().map(canonical_closed_interface).collect();
                bounds.sort();
                bounds
            })
            .collect(),
    }
}

/// A frame parameter spelled by its slot alone (`#N`, the display
/// convention for frame indices), so identity never sees the written name.
fn canonical_param(param: &ParamTy) -> ParamTy {
    ParamTy::new(param.index(), Name::new(format!("#{}", param.index())))
}

fn canonical_plain(ty: &baml_type::Ty) -> baml_type::Ty {
    baml_type::unify::rewrite_ty(ty, &mut |ty| match ty {
        baml_type::Ty::TypeVar(param, attr) => {
            Some(baml_type::Ty::TypeVar(canonical_param(param), attr.clone()))
        }
        _ => None,
    })
}

fn canonical_closed_ty(ty: &ClosedTy) -> ClosedTy {
    ClosedTy::from_plain(&canonical_plain(&ty.to_plain()))
}

fn canonical_closed_interface(interface: &ClosedInterface) -> ClosedInterface {
    ClosedInterface::from_constraint(&interface.to_plain().map_tys(canonical_plain))
}

// ── Locs ─────────────────────────────────────────────────────────────────────

/// An implementation block a package exports, by identity — never by row
/// index: a row's position in the interface is a serialization accident,
/// while [`ImplIdentity`] is what coherence guarantees unique per package.
#[salsa::interned]
pub struct ExternImplLoc<'db> {
    pub package: SourceRoot,
    #[returns(ref)]
    pub identity: ImplIdentity,
}

/// WHERE an exported function row lives. Built ONLY by the mint fns below,
/// from the key the row was found under — never from the row's own
/// `target`, which is blob-controlled data.
///
/// Lifetime-free on purpose: salsa 0.26 stores interned fields as
/// `'static`, so an impl-provided row carries its block's KEY inline rather
/// than the block's interned handle ([`ExternFunctionLoc::impl_block`]
/// re-interns it, a hash lookup).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExternRowAddr {
    /// A row the package DECLARES: `Free { function }` addresses
    /// `functions[namespace][name]`, `Method { class, name }` a row of that
    /// class's `methods`, `Interface { interface, method }` a row of that
    /// interface's `required_methods ++ default_methods`. Each family holds
    /// at most one row per target, so the target is injective here — and
    /// NEVER reaches into `impls`, whose rows share the `Interface { I, m }`
    /// slot with the interface's own row.
    Declared(ExternalCallTarget),
    /// A row an `ExportedImpl` PROVIDES: its `target` is the interface's
    /// dispatch slot (shared with every other implementor), so it is
    /// addressed by its block's key instead.
    ImplProvided {
        package: SourceRoot,
        identity: ImplIdentity,
        method: Name,
    },
}

/// The identity of an exported function row.
#[salsa::interned]
pub struct ExternFunctionLoc<'db> {
    #[returns(ref)]
    pub addr: ExternRowAddr,
}

impl<'db> ExternFunctionLoc<'db> {
    /// The package the row lives in, from the ADDRESS — never from the
    /// row's own `target`, whose heads a blob spells for itself.
    pub fn package(self, db: &'db dyn baml_compiler2_ppir::Db) -> SourceRoot {
        match self.addr(db) {
            ExternRowAddr::Declared(ExternalCallTarget::Free { function }) => function.root(),
            ExternRowAddr::Declared(ExternalCallTarget::Method { class, .. }) => class.root(),
            ExternRowAddr::Declared(ExternalCallTarget::Interface { interface, .. }) => {
                interface.root()
            }
            ExternRowAddr::ImplProvided { package, .. } => *package,
        }
    }

    /// The dispatch slot this row occupies, from the ADDRESS: a declared
    /// row's own target; the implemented interface's `{ I, m }` slot for an
    /// impl-provided row. Equal to the row's `target` for every honest row
    /// (the substrate's oracles pin it), and the HONEST answer when a blob's
    /// `target` lies — which is why consumers link and trust through this,
    /// never through `extern_function_row(..).target`.
    pub fn slot(self, db: &'db dyn baml_compiler2_ppir::Db) -> ExternalCallTarget {
        match self.addr(db) {
            ExternRowAddr::Declared(target) => target.clone(),
            ExternRowAddr::ImplProvided {
                identity, method, ..
            } => ExternalCallTarget::Interface {
                interface: identity.interface.clone(),
                method: method.clone(),
            },
        }
    }

    /// The impl block that provides this row, for an impl-provided row.
    /// `None` IS the answer for a declared row.
    pub fn impl_block(self, db: &'db dyn baml_compiler2_ppir::Db) -> Option<ExternImplLoc<'db>> {
        match self.addr(db) {
            ExternRowAddr::Declared(_) => None,
            ExternRowAddr::ImplProvided {
                package, identity, ..
            } => Some(ExternImplLoc::new(db, *package, identity.clone())),
        }
    }
}

// Salsa interned types don't auto-derive Debug — the `hir::loc` convention.
impl std::fmt::Debug for ExternImplLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExternImplLoc(..)")
    }
}

impl std::fmt::Debug for ExternFunctionLoc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExternFunctionLoc(..)")
    }
}

// ── Row reads ────────────────────────────────────────────────────────────────

fn type_row_at<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    qtn: &DeclName,
) -> Option<&'db ExportedType> {
    package_interface(db, qtn.root()).lookup_type(qtn.namespace(), qtn.name())
}

/// The row at `addr`, if the package exports one: the ONE name-keyed row
/// read. `Option` is legitimate here and in the mint fns only — absence
/// means "no such member"; every read through a minted loc is total.
pub fn extern_row_at<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    addr: &ExternRowAddr,
) -> Option<&'db ExportedFunction> {
    match addr {
        ExternRowAddr::Declared(ExternalCallTarget::Free { function }) => {
            package_interface(db, function.root())
                .lookup_function(function.namespace(), function.name())
        }
        ExternRowAddr::Declared(ExternalCallTarget::Method { class, name }) => {
            let ExportedType::Class { methods, .. } = type_row_at(db, class)? else {
                return None;
            };
            methods.iter().find(|method| method.name == *name)
        }
        ExternRowAddr::Declared(ExternalCallTarget::Interface { interface, method }) => {
            let ExportedType::Interface {
                required_methods,
                default_methods,
                ..
            } = type_row_at(db, interface)?
            else {
                return None;
            };
            required_methods
                .iter()
                .chain(default_methods)
                .find(|row| row.name == *method)
        }
        ExternRowAddr::ImplProvided {
            package,
            identity,
            method,
        } => impl_row_at(db, *package, identity)?
            .methods
            .iter()
            .find(|row| row.name == *method),
    }
}

/// The row `function` names. TOTAL: the loc was minted from a row in the
/// current revision's interface input, so a miss is a compiler bug.
pub fn extern_function_row<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: ExternFunctionLoc<'db>,
) -> &'db ExportedFunction {
    let addr = function.addr(db);
    extern_row_at(db, addr).unwrap_or_else(|| {
        panic!(
            "internal error: no exported function row for {}; an `ExternFunctionLoc` is minted \
             only from a row in the current revision's package interface",
            spell_addr(db, addr)
        )
    })
}

fn spell_addr(db: &dyn baml_compiler2_ppir::Db, addr: &ExternRowAddr) -> String {
    let viewpoint = Viewpoint::canonical(db);
    match addr {
        ExternRowAddr::Declared(ExternalCallTarget::Free { function }) => {
            format!("function `{}`", function.spell(&viewpoint))
        }
        ExternRowAddr::Declared(ExternalCallTarget::Method { class, name }) => {
            format!("method `{}.{name}`", class.spell(&viewpoint))
        }
        ExternRowAddr::Declared(ExternalCallTarget::Interface { interface, method }) => {
            format!(
                "interface method `{}.{method}`",
                interface.spell(&viewpoint)
            )
        }
        ExternRowAddr::ImplProvided {
            package,
            identity,
            method,
        } => format!(
            "method `{method}` of {}",
            spell_impl(db, *package, identity)
        ),
    }
}

fn spell_impl(
    db: &dyn baml_compiler2_ppir::Db,
    package: SourceRoot,
    identity: &ImplIdentity,
) -> String {
    format!(
        "{} in package `{}`",
        spell_impl_identity(identity, &Viewpoint::canonical(db)),
        baml_compiler2_hir::package::spelling(db).of(package),
    )
}

/// `` `implement I for T` `` — an impl identity by its heads, for messages.
pub(crate) fn spell_impl_identity(identity: &ImplIdentity, viewpoint: &Viewpoint) -> String {
    format!(
        "`implement {} for {}`",
        identity.interface.spell(viewpoint),
        identity.for_ty_pattern.to_plain().spell(viewpoint),
    )
}

/// Every impl row the package exports, by identity.
///
/// Two rows with one identity overlap trivially, and coherence rejects that
/// at the source compile, so a faithful export never carries it;
/// [`crate::package_interface::import_interface`] refuses a blob that does
/// (`ImportError::DuplicateImpl`) before it can be mounted. Every package
/// this index is built for is a mounted one, so the collision is an
/// invariant here, not an outcome.
#[salsa::tracked(returns(ref))]
pub fn package_impl_index(
    db: &dyn baml_compiler2_ppir::Db,
    package: SourceRoot,
) -> FxHashMap<ImplIdentity, u32> {
    let mut index = FxHashMap::default();
    for (row, exported) in package_interface(db, package).impls.iter().enumerate() {
        let row = u32::try_from(row).expect("impl row count fits in u32");
        let identity = exported_impl_identity(exported);
        if let Some(earlier) = index.insert(identity.clone(), row) {
            unreachable!(
                "package `{}` was mounted with two impl rows of one identity ({}; rows {earlier} \
                 and {row}), which `import_interface` refuses",
                baml_compiler2_hir::package::spelling(db).of(package),
                spell_impl_identity(&identity, &Viewpoint::canonical(db)),
            );
        }
    }
    index
}

fn impl_row_at<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    package: SourceRoot,
    identity: &ImplIdentity,
) -> Option<&'db ExportedImpl> {
    let row = *package_impl_index(db, package).get(identity)?;
    Some(&package_interface(db, package).impls[usize::try_from(row).expect("u32 fits in usize")])
}

/// The row `block` names. TOTAL, as [`extern_function_row`].
pub fn extern_impl_row<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    block: ExternImplLoc<'db>,
) -> &'db ExportedImpl {
    let package = block.package(db);
    let identity = block.identity(db);
    impl_row_at(db, package, identity).unwrap_or_else(|| {
        panic!(
            "internal error: no exported impl row for {}; an `ExternImplLoc` is minted only from \
             a row in the current revision's package interface",
            spell_impl(db, package, identity)
        )
    })
}

/// The matching facts of an exported impl row, in the location-free shape
/// the impl registry matches: one road for runtime mounts and the
/// precompiled stdlib alike (the interface is a salsa input on the root, so
/// replacing a mount invalidates this like any other memo).
#[salsa::tracked(returns(ref))]
pub fn extern_impl_facts<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    block: ExternImplLoc<'db>,
) -> MountedImplFacts {
    crate::impls::exported_impl_facts(extern_impl_row(db, block))
}

/// The row's signature in the shape the source twin
/// ([`crate::callable::function_signature_ty`]) has — a projection of the
/// row, whose parameter modes the export already mapped from `has_default`.
#[salsa::tracked(returns(ref))]
pub fn extern_signature_ty<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: ExternFunctionLoc<'db>,
) -> FunctionSignatureTy {
    let row = extern_function_row(db, function);
    FunctionSignatureTy {
        params: row.params.clone(),
        return_type: row.return_type.clone(),
        generic_params: row.generic_params.clone(),
        builtin_kind: row.builtin_kind,
    }
}

// ── Owner frame ──────────────────────────────────────────────────────────────
//
// Every field of a row is read off [`extern_function_row`] directly — there
// are deliberately no per-field accessors, so a consumer holds ONE total read
// rather than several queries that could drift. The one derived read follows.

/// The generic frame the row's owner contributes ahead of the row's own
/// parameters, with each slot's declared bounds: the CLASS row's frame for
/// a class method, borrowed from the row; `[Self] ++ the interface's
/// generics` for an interface method, `Self` bounded by the interface at its
/// own parameters — the source frame (`lower::interface_scope_bounds`),
/// which an exported row's own generics index into, built here since no
/// row holds it whole; EMPTY for a free function and for an impl-provided
/// row, whose frame is the matched impl's, realized by the caller.
pub fn extern_owner_generics<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: ExternFunctionLoc<'db>,
) -> (Cow<'db, [ParamTy]>, Cow<'db, [Vec<baml_type::Interface>]>) {
    match function.addr(db) {
        ExternRowAddr::Declared(ExternalCallTarget::Method { class, .. }) => {
            match type_row_at(db, class) {
                Some(ExportedType::Class {
                    generic_params,
                    generic_param_bounds,
                    ..
                }) => (
                    Cow::Borrowed(generic_params.as_slice()),
                    Cow::Borrowed(generic_param_bounds.as_slice()),
                ),
                _ => panic!(
                    "internal error: the class row of {} is gone from the interface that minted \
                     the method row",
                    spell_addr(db, function.addr(db))
                ),
            }
        }
        ExternRowAddr::Declared(ExternalCallTarget::Interface { interface, .. }) => {
            match type_row_at(db, interface) {
                Some(ExportedType::Interface {
                    self_param,
                    generic_params,
                    param_bounds,
                    ..
                }) => {
                    let args: Box<[baml_type::Ty]> = generic_params
                        .iter()
                        .map(|param| {
                            baml_type::Ty::TypeVar(param.clone(), baml_type::TyAttr::default())
                        })
                        .collect();
                    let mut params = Vec::with_capacity(1 + generic_params.len());
                    params.push(self_param.clone());
                    params.extend(generic_params.iter().cloned());
                    let mut bounds = Vec::with_capacity(params.len());
                    // `Self`'s self-bound carries no pins, as in source: a
                    // `Self.Member` projection reduces through the resolver.
                    bounds.push(vec![baml_type::Interface::new(
                        interface.clone(),
                        args,
                        Box::new([]),
                    )]);
                    bounds.extend(param_bounds.iter().cloned());
                    (Cow::Owned(params), Cow::Owned(bounds))
                }
                _ => panic!(
                    "internal error: the interface row of {} is gone from the interface that \
                     minted the method row",
                    spell_addr(db, function.addr(db))
                ),
            }
        }
        ExternRowAddr::Declared(ExternalCallTarget::Free { .. })
        | ExternRowAddr::ImplProvided { .. } => (Cow::Borrowed(&[]), Cow::Borrowed(&[])),
    }
}

// ── Mint boundary: the ONLY constructors ─────────────────────────────────────
//
// Each mints from the key the row was found under, never from the row's
// `target` (blob-controlled data: a forged target must not be able to make
// a consumer link to another package's symbol). `Option` here means "no
// such member" and nowhere else.

fn mint_declared(
    db: &dyn baml_compiler2_ppir::Db,
    target: ExternalCallTarget,
) -> Option<ExternFunctionLoc<'_>> {
    let addr = ExternRowAddr::Declared(target);
    extern_row_at(db, &addr)?;
    Some(ExternFunctionLoc::new(db, addr))
}

// ── The ladder's lane gate ───────────────────────────────────────────────────
//
// A package's rows are minted for CONSUMERS only when the package is served
// from its interface; a source-backed package's declarations are its items,
// and its rows exist only for export. The `mounted_*` twins of the three
// declared mints apply that gate (the same one `mounted_type_row` applies to
// type rows), so a resolution ladder that consults them cannot answer a
// source item with an extern loc. The ungated mints below are for callers
// that hold a served-root head already, or that deliberately want the
// export (the incomplete-impl recovery in `lookup_impl_member`).

fn served(db: &dyn baml_compiler2_ppir::Db, root: SourceRoot) -> bool {
    baml_compiler2_hir::package::is_served_from_interface(db, root)
}

/// [`extern_function_named`] for a package served from its interface.
pub fn mounted_function_named<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    root: SourceRoot,
    namespace: &[Name],
    name: &Name,
) -> Option<ExternFunctionLoc<'db>> {
    served(db, root).then(|| extern_function_named(db, root, namespace, name))?
}

/// [`extern_class_method`] for a class of a package served from its interface.
pub fn mounted_class_method<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: &DeclName,
    name: &Name,
) -> Option<ExternFunctionLoc<'db>> {
    served(db, class.root()).then(|| extern_class_method(db, class, name))?
}

/// [`extern_interface_method`] for an interface of a package served from its
/// interface.
pub fn mounted_interface_method<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: &DeclName,
    name: &Name,
) -> Option<ExternFunctionLoc<'db>> {
    served(db, interface.root()).then(|| extern_interface_method(db, interface, name))?
}

/// The free function `namespace.name` of `root`, if exported.
pub fn extern_function_named<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    root: SourceRoot,
    namespace: &[Name],
    name: &Name,
) -> Option<ExternFunctionLoc<'db>> {
    mint_declared(
        db,
        ExternalCallTarget::Free {
            function: DeclName::in_root(root, namespace.to_vec(), name.clone()),
        },
    )
}

/// The inherent method `name` of the exported class `class`, if any.
pub fn extern_class_method<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    class: &DeclName,
    name: &Name,
) -> Option<ExternFunctionLoc<'db>> {
    mint_declared(
        db,
        ExternalCallTarget::Method {
            class: class.clone(),
            name: name.clone(),
        },
    )
}

/// The required or default method `method` the exported interface
/// `interface` DECLARES, if any — never a row an implementor provides.
pub fn extern_interface_method<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    interface: &DeclName,
    method: &Name,
) -> Option<ExternFunctionLoc<'db>> {
    mint_declared(
        db,
        ExternalCallTarget::Interface {
            interface: interface.clone(),
            method: method.clone(),
        },
    )
}

/// The method `method` the exported impl `block` provides, if any.
pub fn extern_impl_method<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    block: ExternImplLoc<'db>,
    method: &Name,
) -> Option<ExternFunctionLoc<'db>> {
    let addr = ExternRowAddr::ImplProvided {
        package: block.package(db),
        identity: block.identity(db).clone(),
        method: method.clone(),
    };
    extern_row_at(db, &addr)?;
    Some(ExternFunctionLoc::new(db, addr))
}

/// The impl row of `package` with `identity`, if it exports one.
pub fn extern_impl_block(
    db: &dyn baml_compiler2_ppir::Db,
    package: SourceRoot,
    identity: ImplIdentity,
) -> Option<ExternImplLoc<'_>> {
    package_impl_index(db, package)
        .contains_key(&identity)
        .then(|| ExternImplLoc::new(db, package, identity))
}
