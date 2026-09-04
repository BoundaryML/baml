//! Interface coherence and orphan checking over resolved implementation facts.
//!
//! The shared type unifier decides whether two implementation subjects have a
//! common instance. Bounds on ground witnesses are checked through the impl
//! registry. Reports retain item locations; diagnostics map them to spans.

use baml_compiler2_hir::{
    contributions::Definition,
    loc::ImplLoc,
    package::{PackageId, package_dependency_closure},
};
pub use baml_type::unify::{Overlap, TypeBindings};
use baml_type::{
    FunctionParamTy, Name, ParamTy, RealizedTy, Ty, TyAttr, TypeName,
    interned::{ClosedInterface, ClosedTy, InferInterface},
    normalize::TypeContext,
    unify::{EnumVariants, chase_var, contains_bound_typevar, nf, unify_into, var_under_union},
};
use rustc_hash::FxHashMap;

use crate::impls::{ImplFacts, impl_facts, package_impl_locs};

fn normalized_alias_map<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg: PackageId<'db>,
) -> FxHashMap<TypeName, Ty> {
    let mut aliases = FxHashMap::default();
    collect_package_aliases(db, pkg, &mut aliases);
    for dep in package_dependency_closure(db, pkg) {
        collect_package_aliases(db, *dep, &mut aliases);
    }
    let facts = crate::facts::Facts::new(db);
    let enum_variants = |qtn: &TypeName| facts.enum_variants(qtn);
    for body in aliases.values_mut() {
        *body = nf(body, &enum_variants);
    }
    aliases
}

fn collect_package_aliases<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg: PackageId<'db>,
    out: &mut FxHashMap<TypeName, Ty>,
) {
    let items = baml_compiler2_ppir::package_items(db, pkg);
    for (ns_path, ns_items) in &items.namespaces {
        for (name, def) in &ns_items.types {
            if let Definition::TypeAlias(loc) = def {
                let qualified = TypeName::new(items.package.clone(), ns_path.clone(), name.clone());
                out.entry(qualified)
                    .or_insert_with(|| crate::lower::type_alias_value(db, *loc));
            }
        }
    }
}

fn substitute_plain(ty: &Ty, bindings: &TypeBindings) -> Ty {
    match ty {
        Ty::TypeVar(param, _) => bindings.get(param).cloned().unwrap_or_else(|| ty.clone()),
        Ty::Union(members, attr) => Ty::Union(
            members
                .iter()
                .map(|m| substitute_plain(m, bindings))
                .collect(),
            attr.clone(),
        ),
        Ty::Class(name, args, attr) => Ty::Class(
            name.clone(),
            args.iter().map(|a| substitute_plain(a, bindings)).collect(),
            attr.clone(),
        ),
        Ty::Interface(name, args, assoc, attr) => Ty::Interface(
            name.clone(),
            args.iter().map(|a| substitute_plain(a, bindings)).collect(),
            assoc
                .iter()
                .map(|(n, t)| (n.clone(), substitute_plain(t, bindings)))
                .collect(),
            attr.clone(),
        ),
        Ty::List(inner, attr) => {
            Ty::List(Box::new(substitute_plain(inner, bindings)), attr.clone())
        }
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(substitute_plain(key, bindings)),
            value: Box::new(substitute_plain(value, bindings)),
            attr: attr.clone(),
        },
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(substitute_plain(value, bindings)),
            Box::new(substitute_plain(error, bindings)),
            attr.clone(),
        ),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => Ty::AssociatedTypeProjection {
            base: Box::new(substitute_plain(base, bindings)),
            interface: Box::new(interface.map_tys(|t| substitute_plain(t, bindings))),
            member: member.clone(),
            attr: attr.clone(),
        },
        Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            params: params
                .iter()
                .map(|p| FunctionParamTy {
                    name: p.name.clone(),
                    ty: substitute_plain(&p.ty, bindings),
                    mode: p.mode,
                })
                .collect(),
            ret: Box::new(substitute_plain(ret, bindings)),
            throws: Box::new(substitute_plain(throws, bindings)),
            attr: attr.clone(),
        },
        _ => ty.clone(),
    }
}

/// A coherence violation: two implementations of the same interface that
/// overlap, or could not be proven disjoint. With no specialization,
/// either is a hard error; `indeterminate` words the diagnostic.
/// Location-keyed (span-free): S17 maps to source ranges at render time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, salsa::Update)]
pub struct CoherenceViolation<'db> {
    /// The offending impl - always owned by the package being checked,
    /// so the diagnostic lands on a file the user can edit.
    pub primary: ImplLoc<'db>,
    /// The impl it overlaps with. May live in a dependency package.
    pub secondary: ImplLoc<'db>,
    /// `true` when overlap could be neither proven nor disproven
    /// (conservatively rejected) rather than definite.
    pub indeterminate: bool,
}

/// Wrapper for the manual `salsa::Update` impl (the `ImplFacts`
/// precedent).
#[derive(Debug, Clone, PartialEq, salsa::Update)]
pub struct CoherenceReport<'db>(pub Vec<CoherenceViolation<'db>>);

/// Per-package coherence: overlapping implementations across the package
/// and its dependency closure - rustc's per-crate coherence plus
/// knowability. The orphan rule guarantees any overlapping pair has one
/// side's package depending on the other's (or is intra-package), so
/// checking each package against its dependencies is complete without a
/// whole-program pass. Only pairs with at least one impl owned by `pkg`
/// are reported; dependency-internal conflicts are attributed to the
/// dependency when ITS coherence is checked.
#[salsa::tracked(returns(ref))]
pub fn package_coherence_violations<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg: PackageId<'db>,
) -> CoherenceReport<'db> {
    let mut own = package_impls(db, pkg);
    // Stable textual order for attribution (the later impl carries the
    // error): key on (file path, span start) - a package spans multiple
    // files, and impl ids are structural hashes, not source order.
    own.sort_by_key(|&(loc, _)| impl_sort_key(db, loc));

    let aliases = normalized_alias_map(db, pkg);
    let dep_impls: Vec<(ImplLoc<'db>, &'db ImplFacts<'db>)> = package_dependency_closure(db, pkg)
        .iter()
        .flat_map(|dep| package_impls(db, *dep))
        .collect();

    let mut violations = Vec::new();
    for (i, &(own_loc, own_facts)) in own.iter().enumerate() {
        // own x own - each unordered pair once; the later impl is primary.
        for &(other_loc, other_facts) in &own[i + 1..] {
            if let Some(indeterminate) =
                overlap_violation(impls_conflict(db, own_facts, other_facts, &aliases))
            {
                violations.push(CoherenceViolation {
                    primary: other_loc,
                    secondary: own_loc,
                    indeterminate,
                });
            }
        }
        // own x dependency - the owning package's impl is primary.
        for &(dep_loc, dep_facts) in &dep_impls {
            if let Some(indeterminate) =
                overlap_violation(impls_conflict(db, own_facts, dep_facts, &aliases))
            {
                violations.push(CoherenceViolation {
                    primary: own_loc,
                    secondary: dep_loc,
                    indeterminate,
                });
            }
        }
    }
    CoherenceReport(violations)
}

/// `None` = disjoint; `Some(indeterminate)` = report.
fn overlap_violation(overlap: Overlap) -> Option<bool> {
    match overlap {
        Overlap::No => None,
        Overlap::Yes => Some(false),
        Overlap::Unknown => Some(true),
    }
}

fn package_impls<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg: PackageId<'db>,
) -> Vec<(ImplLoc<'db>, &'db ImplFacts<'db>)> {
    package_impl_locs(db, pkg)
        .iter()
        .filter_map(|&loc| impl_facts(db, loc).resolved().map(|facts| (loc, facts)))
        .collect()
}

fn impl_sort_key(db: &dyn baml_compiler2_ppir::Db, loc: ImplLoc<'_>) -> (String, u32) {
    let span = baml_compiler2_ppir::item_data::impl_block_source_map(db, loc).span;
    (
        loc.file(db).path(db).display().to_string(),
        u32::from(span.start()),
    )
}

/// True iff two impls of the SAME interface conflict. Distinct interfaces
/// never conflict; a duplicate in-body block is a degenerate overlap
/// (rustc's conflicting-implementations error for exact duplicates).
///
/// There is deliberately NO concreteness gate here: an impl whose
/// for-target is not an implementor never produces
/// [`ImplFacts`] at all
/// ([`ImplHeaderResolution::NotImplementor`](crate::impls::ImplHeaderResolution)),
/// so it cannot reach this function. Re-deriving that judgment locally is
/// what opened the E0132 hole: this gate judged the RAW head while E0138
/// judged the normalized one, so a `true | false` subject was invalid here
/// and valid there, and the pair escaped both.
pub fn impls_conflict(
    db: &dyn baml_compiler2_ppir::Db,
    a: &ImplFacts<'_>,
    b: &ImplFacts<'_>,
    aliases: &FxHashMap<TypeName, Ty>,
) -> Overlap {
    if a.interface.name != b.interface.name {
        return Overlap::No;
    }
    impls_overlap(db, a, b, aliases)
}

/// Compare a source-owned impl with a span-less mounted export row.  This is
/// the declaration-diagnostic twin of the mounted candidate registry: the
/// overlap engine receives the same normalized facts, but the caller keeps the
/// mounted side structural because there is no legitimate `ImplLoc` to mint.
pub fn source_mounted_impl_conflict(
    db: &dyn baml_compiler2_ppir::Db,
    source_package: PackageId<'_>,
    source: &ImplFacts<'_>,
    mounted: &crate::package_interface::ExportedImpl,
) -> Overlap {
    let mounted_facts = ImplFacts {
        interface: ClosedInterface::from_constraint(&mounted.interface),
        for_ty_pattern: ClosedTy::from_plain(&mounted.for_ty_pattern),
        generic_params: mounted
            .generic_params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                (
                    param.clone(),
                    mounted
                        .param_bounds
                        .get(index)
                        .into_iter()
                        .flatten()
                        .map(ClosedInterface::from_constraint)
                        .collect(),
                )
            })
            .collect(),
        associated_types: mounted
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), ClosedTy::from_plain(ty)))
            .collect(),
        methods: Vec::new(),
    };
    impls_conflict(
        db,
        source,
        &mounted_facts,
        &normalized_alias_map(db, source_package),
    )
}

/// Conservative symmetric overlap over two impls of the same interface:
/// do the subjects (for-type + interface args) share a common instance?
/// Both impls' params become fresh disjoint unification variables, so
/// complementary pairs (`Pair<T, int>` vs `Pair<string, U>`) are found.
/// Associated bindings are interface OUTPUTS - only args participate.
/// A bound on a param the unifier pinned to a ground witness is checked
/// against the registry; provable violation makes the pair disjoint
/// (overriding even `Unknown`).
fn impls_overlap(
    db: &dyn baml_compiler2_ppir::Db,
    a: &ImplFacts<'_>,
    b: &ImplFacts<'_>,
    aliases: &FxHashMap<TypeName, Ty>,
) -> Overlap {
    let facts = crate::facts::Facts::new(db);
    let enum_variants = |qtn: &TypeName| facts.enum_variants(qtn);
    let (a_for, a_args) = renamed_subject(a, 'a', &enum_variants);
    let (b_for, b_args) = renamed_subject(b, 'b', &enum_variants);
    if a_args.len() != b_args.len() {
        return Overlap::No;
    }
    let mut vars: Vec<ParamTy> =
        Vec::with_capacity(a.generic_params.len() + b.generic_params.len());
    vars.extend((0..a.generic_params.len()).map(|i| renamed_var('a', i)));
    vars.extend((0..b.generic_params.len()).map(|i| renamed_var('b', i)));

    let mut bindings = TypeBindings::default();
    let mut result = unify_into(&a_for, &b_for, &vars, aliases, &mut bindings);
    if result == Overlap::No {
        return Overlap::No;
    }
    for (x, y) in a_args.iter().zip(b_args.iter()) {
        match unify_into(x, y, &vars, aliases, &mut bindings) {
            Overlap::No => return Overlap::No,
            Overlap::Unknown => result = Overlap::Unknown,
            Overlap::Yes => {}
        }
    }

    let a_subject: Vec<&Ty> = std::iter::once(&a_for).chain(a_args.iter()).collect();
    let b_subject: Vec<&Ty> = std::iter::once(&b_for).chain(b_args.iter()).collect();
    if !bounds_hold_at_common_instance(db, a, 'a', &vars, &bindings, &a_subject)
        || !bounds_hold_at_common_instance(db, b, 'b', &vars, &bindings, &b_subject)
    {
        return Overlap::No;
    }
    result
}

/// Whether every bound of this impl could hold at the common instance.
/// `false` only on a PROVABLE violation: the binding must be principal
/// (not a union-cover witness), the witness fully realized, and the
/// instantiated bound fully realized - then a `None` from the registry
/// means the witness genuinely does not implement it. Anything less
/// stays conservatively satisfiable (a wrong negative would admit an
/// overlapping pair).
fn bounds_hold_at_common_instance(
    db: &dyn baml_compiler2_ppir::Db,
    rule: &ImplFacts<'_>,
    prefix: char,
    vars: &[ParamTy],
    bindings: &TypeBindings,
    subject: &[&Ty],
) -> bool {
    let param_witnesses: TypeBindings = rule
        .generic_params
        .iter()
        .enumerate()
        .map(|(j, (param, _))| {
            (
                param.clone(),
                chase_var(
                    &Ty::TypeVar(renamed_var(prefix, j), TyAttr::default()),
                    vars,
                    bindings,
                ),
            )
        })
        .collect();

    for (i, (param, bounds)) in rule.generic_params.iter().enumerate() {
        if bounds.is_empty() {
            continue;
        }
        let var_i = renamed_var(prefix, i);
        if subject.iter().any(|t| var_under_union(&var_i, t)) {
            continue;
        }
        let witness = &param_witnesses[param];
        if contains_bound_typevar(witness, vars) || RealizedTy::try_from(witness).is_err() {
            continue;
        }
        for bound in bounds {
            let bound = bound
                .to_plain()
                .map_tys(|t| substitute_plain(t, &param_witnesses));
            if bound.tys().all(|t| RealizedTy::try_from(t).is_ok())
                && crate::impls::resolve_impl(
                    db,
                    &crate::impls::interned_ty(witness),
                    &InferInterface::from_constraint(&bound),
                )
                .is_none()
            {
                return false;
            }
        }
    }
    true
}

/// Fresh unification param for side `prefix`'s `idx`-th generic param.
fn renamed_var(prefix: char, idx: usize) -> ParamTy {
    ParamTy::new(
        u32::try_from(idx).expect("coherence variable index fits in u32"),
        Name::new(format!("__coherence_{prefix}_{idx}")),
    )
}

/// The impl's subject - for-type and interface args - normalized with its
/// generic params renamed to side-`prefix` unification variables.
/// Associated bindings are dropped (interface outputs, not overlap
/// inputs).
fn renamed_subject(
    rule: &ImplFacts<'_>,
    prefix: char,
    enum_variants: EnumVariants,
) -> (Ty, Vec<Ty>) {
    let rename: TypeBindings = rule
        .generic_params
        .iter()
        .enumerate()
        .map(|(i, (param, _bounds))| {
            (
                param.clone(),
                Ty::TypeVar(renamed_var(prefix, i), TyAttr::default()),
            )
        })
        .collect();
    let for_ty = nf(
        &substitute_plain(&rule.for_ty_pattern.to_plain(), &rename),
        enum_variants,
    );
    let args = rule
        .interface
        .to_plain()
        .generics
        .iter()
        .map(|arg| nf(&substitute_plain(arg, &rename), enum_variants))
        .collect();
    (for_ty, args)
}

// ── The orphan rule (E0139, RFC-2451 covered) ────────────────────────────

/// An orphan-rule violation on one impl: a foreign interface implemented
/// with no local type covering the impl (or an uncovered generic param
/// appearing before the first local type).
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct OrphanViolation<'db> {
    pub block: ImplLoc<'db>,
    pub interface: TypeName,
    /// `Some` = the RFC-2451 uncovered-param flavor; `None` = no local
    /// type anywhere in the impl's inputs.
    pub uncovered_param: Option<Name>,
}

/// Wrapper for the manual `salsa::Update` impl.
#[derive(Debug, Clone, PartialEq, salsa::Update)]
pub struct OrphanReport<'db>(pub Vec<OrphanViolation<'db>>);

/// Per-package orphan check: every impl must implement a local interface
/// or cover a local type (RFC-2451: the first local class/enum in the
/// impl's inputs - the for-type then the interface args, in order - with
/// any generic param BEFORE it uncovered and rejected).
#[salsa::tracked(returns(ref))]
pub fn package_orphan_violations<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg: PackageId<'db>,
) -> OrphanReport<'db> {
    let current_package = pkg.name(db);
    let mut violations = Vec::new();
    for (loc, facts) in package_impls(db, pkg) {
        let for_ty = facts.for_ty_pattern.to_plain();
        let args: Vec<Ty> = facts.interface.to_plain().generics.to_vec();
        match orphan_check(&current_package, &facts.interface.name, &for_ty, &args) {
            OrphanOutcome::Ok => {}
            OrphanOutcome::UncoveredParam(name) => violations.push(OrphanViolation {
                block: loc,
                interface: facts.interface.name.clone(),
                uncovered_param: Some(name),
            }),
            OrphanOutcome::NoLocalType => violations.push(OrphanViolation {
                block: loc,
                interface: facts.interface.name.clone(),
                uncovered_param: None,
            }),
        }
    }
    OrphanReport(violations)
}

enum OrphanOutcome {
    Ok,
    UncoveredParam(Name),
    NoLocalType,
}

fn orphan_check(
    current_package: &Name,
    interface: &TypeName,
    for_ty: &Ty,
    interface_args: &[Ty],
) -> OrphanOutcome {
    if interface.package() == current_package {
        return OrphanOutcome::Ok;
    }
    for input in std::iter::once(for_ty).chain(interface_args.iter()) {
        match input {
            Ty::Class(tn, ..) | Ty::Enum(tn, ..) if tn.package() == current_package => {
                return OrphanOutcome::Ok;
            }
            Ty::TypeVar(param, _) => {
                return OrphanOutcome::UncoveredParam(param.name().clone());
            }
            _ => {}
        }
    }
    OrphanOutcome::NoLocalType
}

#[cfg(test)]
mod tests {
    use super::*;

    fn foreign(name: &str) -> TypeName {
        TypeName::new(Name::new("dep"), Vec::new(), Name::new(name))
    }

    fn local_class(package: &str, name: &str) -> Ty {
        Ty::Class(
            TypeName::new(Name::new(package), Vec::new(), Name::new(name)),
            Box::new([]),
            TyAttr::default(),
        )
    }

    #[test]
    fn orphan_local_interface_is_ok() {
        let iface = TypeName::new(Name::new("me"), Vec::new(), Name::new("I"));
        assert!(matches!(
            orphan_check(&Name::new("me"), &iface, &local_class("dep", "C"), &[]),
            OrphanOutcome::Ok
        ));
    }

    #[test]
    fn orphan_foreign_interface_local_for_type_is_ok() {
        assert!(matches!(
            orphan_check(
                &Name::new("me"),
                &foreign("I"),
                &local_class("me", "C"),
                &[]
            ),
            OrphanOutcome::Ok
        ));
    }

    #[test]
    fn orphan_foreign_interface_no_local_type_is_violation() {
        assert!(matches!(
            orphan_check(
                &Name::new("me"),
                &foreign("I"),
                &local_class("dep", "C"),
                &[]
            ),
            OrphanOutcome::NoLocalType
        ));
    }

    #[test]
    fn orphan_uncovered_param_before_local_type_is_violation() {
        // `implement<T> dep.I<me.C> for T`: the bare param precedes the
        // first local type (RFC-2451's covered rule).
        assert!(matches!(
            orphan_check(
                &Name::new("me"),
                &foreign("I"),
                &Ty::type_var("T"),
                &[local_class("me", "C")]
            ),
            OrphanOutcome::UncoveredParam(_)
        ));
    }

    #[test]
    fn orphan_local_type_in_interface_args_is_ok() {
        // `implement dep.I<me.C> for int`: the local type may appear in the
        // interface args, not only the for-type.
        assert!(matches!(
            orphan_check(
                &Name::new("me"),
                &foreign("I"),
                &Ty::int(),
                &[local_class("me", "C")]
            ),
            OrphanOutcome::Ok
        ));
    }
}
