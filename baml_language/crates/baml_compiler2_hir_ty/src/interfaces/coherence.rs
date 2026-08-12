//! Per-package interface coherence: no two `implements` blocks of the same interface
//! may overlap ([`package_coherence_diagnostics`]). Overlap between two impl subjects
//! is decided by the shared equality-unification engine ([`baml_type::unify`]): the two
//! impls' generic params are renamed to disjoint unification variables, the for-types
//! and interface args must unify position-wise, and a bound a pinned param provably
//! violates makes the pair disjoint.

use baml_base::{Name, Span, TyAttr};
use baml_compiler2_hir::package::PackageId;
use baml_type::unify::{
    EnumVariants, Overlap, TypeBindings, chase_var, contains_bound_typevar, expand_alias_head, nf,
    substitute_ty, unify_into, var_under_union,
};
use baml_type::{ParamTy, Ty, TypeName};

use super::{
    ImplData, enum_variant_names, impl_data, impl_data_source_map, interface_loc_qtn,
    normalized_alias_map, package_impl_locs,
};

/// Map an overlap result to a reportable violation: `None` = disjoint (no
/// diagnostic); `Some(indeterminate)` = report it, where `indeterminate` is
/// `true` for the conservative "couldn't prove disjoint" case.
fn overlap_violation(overlap: Overlap) -> Option<bool> {
    match overlap {
        Overlap::No => None,
        Overlap::Yes => Some(false),
        Overlap::Unknown => Some(true),
    }
}

/// A coherence violation: two implementations of the same interface that overlap,
/// or that could not be proven disjoint. With no specialization, either is a hard
/// error; `indeterminate` lets the caller word the diagnostic correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoherenceViolation {
    /// The offending impl. Always owned by the package being checked, so the
    /// diagnostic lands on a file the user can edit.
    pub primary: Span,
    /// The impl it overlaps with. May live in a dependency package.
    pub secondary: Span,
    /// `true` when overlap could be neither proven nor disproven (conservatively
    /// rejected) rather than a definite overlap.
    pub indeterminate: bool,
}

/// Per-package interface coherence check.
///
/// Reports overlapping implementations of the same interface across the whole
/// package *and its dependency closure* — the BAML analog of rustc's per-crate
/// coherence plus knowability. Only pairs with at least one impl owned by
/// `pkg_id` are reported; dependency-internal conflicts are attributed to the
/// dependency when *its* coherence is checked, so nothing is double-reported.
#[salsa::tracked(returns(ref))]
pub fn package_coherence_diagnostics<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
) -> Vec<CoherenceViolation> {
    let mut own = package_impls_with_spans(db, pkg_id);
    // Sort by source position so the overlap attribution tracks a stable textual
    // order. Key on `(file_id, start, end)`: a package spans multiple files.
    own.sort_by_key(|(_, s)| {
        (
            s.file_id.as_u32(),
            u32::from(s.range.start()),
            u32::from(s.range.end()),
        )
    });
    let deps = baml_compiler2_hir::package::package_dependency_closure(db, pkg_id);

    // Type aliases (own package plus dependency exports), pre-normalized so
    // alias-referencing for-types and interface args are compared by the same
    // union laws as their spelled-out forms.
    let aliases = normalized_alias_map(db, pkg_id);

    let dep_impls: Vec<(&ImplData, Span)> = deps
        .iter()
        .flat_map(|dep| package_impls_with_spans(db, *dep))
        .collect();

    let mut violations = Vec::new();
    for (i, &(own_data, own_span)) in own.iter().enumerate() {
        // own × own — each unordered pair once; the later impl carries the error.
        for &(other_data, other_span) in &own[i + 1..] {
            if let Some(indeterminate) =
                overlap_violation(impls_conflict(db, pkg_id, own_data, other_data, aliases))
            {
                violations.push(CoherenceViolation {
                    primary: other_span,
                    secondary: own_span,
                    indeterminate,
                });
            }
        }
        // own × dependency — the owning package's impl carries the error.
        for &(dep_data, dep_span) in &dep_impls {
            if let Some(indeterminate) =
                overlap_violation(impls_conflict(db, pkg_id, own_data, dep_data, aliases))
            {
                violations.push(CoherenceViolation {
                    primary: own_span,
                    secondary: dep_span,
                    indeterminate,
                });
            }
        }
    }
    violations
}

/// The impls of `pkg` the overlap check compares: each resolved [`ImplData`] paired
/// with its source span, drawn from the canonical `impl_data` substrate.
fn package_impls_with_spans<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
) -> Vec<(&'db ImplData<'db>, Span)> {
    package_impl_locs(db, pkg_id)
        .iter()
        .filter_map(|&loc| {
            let data = impl_data(db, loc).as_ref().ok()?;
            let span = impl_data_source_map(db, loc).impl_span;
            Some((data, span))
        })
        .collect()
}

/// True iff two impls of the *same* interface conflict (overlap with no
/// specialization to rescue them). Distinct interfaces never conflict.
pub fn impls_conflict<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
    a: &ImplData<'db>,
    b: &ImplData<'db>,
    aliases: &std::collections::HashMap<TypeName, Ty>,
) -> Overlap {
    let (Some(a_qtn), Some(b_qtn)) = (
        interface_loc_qtn(db, a.interface),
        interface_loc_qtn(db, b.interface),
    ) else {
        return Overlap::No;
    };
    if a_qtn != b_qtn {
        return Overlap::No;
    }
    // An impl whose for-target is not a valid implementor (rejected by the E0138
    // concreteness gate) must not contribute a coherence overlap, or it would stack
    // a spurious E0132 on top of that rejection. The gate is applied to the
    // *alias-expanded* for-type so `impl I for C` + `impl I for AliasC` cannot slip
    // past both gates.
    if !expand_alias_head(&a.for_ty_pattern, aliases).is_valid_impl_subject()
        || !expand_alias_head(&b.for_ty_pattern, aliases).is_valid_impl_subject()
    {
        return Overlap::No;
    }
    // NOTE: same-class in-body duplicates are NOT excluded here — a duplicate is a
    // degenerate overlap, matching Rust's conflicting-implementations error.
    impls_overlap(db, pkg_id, a, b, aliases)
}

/// Conservative symmetric overlap test over two impls of the same interface.
///
/// Two impls overlap iff their *subjects* — the for-type plus the interface
/// type-args — have a **common instance**, decided by first-order unification
/// with *both* impls' generic params as fresh unification variables. Associated
/// bindings are interface *outputs*, so only the args participate. Bounds a
/// pinned ground witness provably violates make the pair disjoint.
fn impls_overlap<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
    a: &ImplData<'db>,
    b: &ImplData<'db>,
    aliases: &std::collections::HashMap<TypeName, Ty>,
) -> Overlap {
    let enum_variants = |qtn: &TypeName| enum_variant_names(db, qtn);
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
    // Unify the for-type and each interface arg. A provably-disjoint part
    // short-circuits the whole pair to disjoint; an undecidable part downgrades a
    // would-be overlap to `Unknown`.
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

    // The subjects share a common instance; the impls are still disjoint if either
    // carries a bound the common instance provably violates.
    let a_subject: Vec<&Ty> = std::iter::once(&a_for).chain(a_args.iter()).collect();
    let b_subject: Vec<&Ty> = std::iter::once(&b_for).chain(b_args.iter()).collect();
    if !bounds_hold_at_common_instance(db, pkg_id, a, 'a', &vars, &bindings, &a_subject, aliases)
        || !bounds_hold_at_common_instance(
            db, pkg_id, b, 'b', &vars, &bindings, &b_subject, aliases,
        )
    {
        return Overlap::No;
    }
    result
}

/// Whether every bound of `rule` could hold at the common instance the unifier
/// produced. Returns `false` as soon as a bound whose subject the unifier pinned
/// to a *forced* ground type is provably unsatisfiable. A bound whose subject is
/// still a variable is undecidable in an open world and is assumed satisfiable.
#[expect(
    clippy::too_many_arguments,
    reason = "one cohesive overlap-check context: query (db, pkg), this impl (rule, prefix, \
              subject), and the unifier state (vars, bindings, aliases)"
)]
fn bounds_hold_at_common_instance<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
    rule: &ImplData<'db>,
    prefix: char,
    vars: &[ParamTy],
    bindings: &TypeBindings,
    subject: &[&Ty],
    aliases: &std::collections::HashMap<TypeName, Ty>,
) -> bool {
    // Each of this impl's params, resolved to the ground type it takes at the
    // common instance (following the unifier's binding chains).
    let param_witnesses: TypeBindings = rule
        .generic_params
        .iter()
        .enumerate()
        .map(|(j, (name, _))| {
            (
                name.clone(),
                chase_var(
                    &Ty::TypeVar(renamed_var(prefix, j), TyAttr::default()),
                    vars,
                    bindings,
                ),
            )
        })
        .collect();

    for (i, (param_name, bounds)) in rule.generic_params.iter().enumerate() {
        if bounds.is_empty() {
            continue;
        }
        let var_i = renamed_var(prefix, i);
        // Non-principal (union-cover) binding ⇒ the witness is arbitrary; don't disprove.
        if subject.iter().any(|t| var_under_union(&var_i, t)) {
            continue;
        }
        let witness = &param_witnesses[param_name];
        // An unpinned witness (still a coherence var, or otherwise not fully realized)
        // is undecidable in an open world.
        if contains_bound_typevar(witness, vars)
            || baml_type::RealizedTy::try_from(witness).is_err()
        {
            continue;
        }
        // A single provably-unsatisfied bound makes the two impls disjoint at this
        // instance. We may conclude "disjoint" only when the instantiated bound is
        // itself fully realized.
        for bound in bounds {
            let bound = bound.map_tys(|t| substitute_ty(t, &param_witnesses));
            if bound
                .tys()
                .all(|t| baml_type::RealizedTy::try_from(t).is_ok())
                && super::get_implements_block(db, pkg_id, witness, &bound, aliases).is_none()
            {
                return false;
            }
        }
    }
    true
}

/// Fresh unification parameter for the `idx`-th generic param of the impl on
/// side `prefix`.
fn renamed_var(prefix: char, idx: usize) -> ParamTy {
    ParamTy::new(
        u32::try_from(idx).expect("coherence variable index fits in u32"),
        Name::new(format!("__coherence_{prefix}_{idx}")),
    )
}

/// The impl's subject — for-type and interface args — normalized (CNF) with its generic
/// params renamed to side-`prefix` unification variables. Associated bindings are
/// dropped (interface outputs, not part of overlap).
fn renamed_subject(
    rule: &ImplData<'_>,
    prefix: char,
    enum_variants: EnumVariants,
) -> (Ty, Vec<Ty>) {
    let rename: TypeBindings = rule
        .generic_params
        .iter()
        .enumerate()
        .map(|(i, (name, _bounds))| {
            (
                name.clone(),
                Ty::TypeVar(renamed_var(prefix, i), TyAttr::default()),
            )
        })
        .collect();
    let for_ty = nf(&substitute_ty(&rule.for_ty_pattern, &rename), enum_variants);
    let args = rule
        .interface_args
        .iter()
        .map(|arg| nf(&substitute_ty(arg, &rename), enum_variants))
        .collect();
    (for_ty, args)
}
