//! Per-package interface coherence: no two `implements` blocks of the same interface
//! may overlap ([`package_coherence_diagnostics`]). Overlap between two impl subjects
//! is decided by the shared equality-unification engine ([`crate::unify`]): the two
//! impls' generic params are renamed to disjoint unification variables, the for-types
//! and interface args must unify position-wise, and a bound a pinned param provably
//! violates makes the pair disjoint.

use baml_base::{Name, Span, TyAttr};
use baml_compiler2_hir::package::PackageId;
use baml_type::{Ty, TypeName};

use crate::{
    interfaces::{ImplData, impl_data, impl_data_source_map, interface_loc_qtn, package_impl_locs},
    unify::{
        EnumVariants, Overlap, TypeBindings, chase_var, contains_bound_typevar, enum_variant_names,
        expand_alias_head, nf, normalized_alias_map, unify_into, var_under_union,
    },
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
/// coherence plus knowability. The orphan rule (E0139) guarantees every blanket
/// impl lives in its interface's package, and writing `implement I for …`
/// requires depending on `pkg(I)`, so any overlapping pair has one side's
/// package depending on the other's (or is intra-package). Checking each
/// package against its dependencies is therefore complete without a
/// whole-program pass, and stays sound under dynamic loading.
///
/// Only pairs with at least one impl owned by `pkg_id` are reported;
/// dependency-internal conflicts are attributed to the dependency when *its*
/// coherence is checked, so nothing is double-reported.
#[salsa::tracked(returns(ref))]
pub fn package_coherence_diagnostics<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
) -> Vec<CoherenceViolation> {
    let mut own = package_impls_with_spans(db, pkg_id);
    // Sort by source position so the overlap attribution tracks a stable textual order
    // rather than query iteration order, and stays stable when an unrelated item is
    // added. Key on `(file_id, start, end)`: a package spans multiple files, and keying
    // on the start offset alone makes two impls at the same offset in *different* files
    // tie and fall back to a nondeterministic order.
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
                overlap_violation(impls_conflict(db, pkg_id, own_data, other_data, &aliases))
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
                overlap_violation(impls_conflict(db, pkg_id, own_data, dep_data, &aliases))
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
/// with its source span, drawn from the canonical `impl_data` substrate. Span-less
/// (synthesized) impls and unresolved interface targets are dropped — neither can
/// carry a coherence diagnostic.
fn package_impls_with_spans<'db>(
    db: &'db dyn crate::Db,
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
/// specialization to rescue them). Distinct interfaces never conflict, and two
/// in-body blocks of the same class for the same interface are a duplicate
/// (reported separately), not an overlap.
pub fn impls_conflict<'db>(
    db: &'db dyn crate::Db,
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
    // concreteness gate — union, interface, literal, enum variant, or an error
    // type) must not contribute a coherence overlap, or it would stack a spurious
    // E0132 on top of that rejection. The gate is applied to the *alias-expanded*
    // for-type: a bare `type AliasC = C` for-type lowers to `Ty::TypeAlias` (not itself
    // a valid subject), but E0138 expands it and accepts it, so coherence must expand it
    // too — otherwise `impl I for C` + `impl I for AliasC` would slip past both gates,
    // leaving two impls for the same concrete type. (Aliases *under* a constructor are
    // resolved later by `is_same_normalized_type`; only the head matters here.)
    if !expand_alias_head(&a.for_ty_pattern, aliases).is_valid_impl_subject()
        || !expand_alias_head(&b.for_ty_pattern, aliases).is_valid_impl_subject()
    {
        return Overlap::No;
    }
    // NOTE: same-class in-body duplicates are NOT excluded here. Their exclusion
    // (deferring to a separate duplicate-block check) keyed that check on a Display
    // string, which disagreed with coherence's canonicalized equality on reordered /
    // dedupable unions (`Conv<int | string>` vs `Conv<string | int>` in one class) —
    // letting such duplicates escape both checks. Reporting them here as an overlap
    // (E0132) is correct: a duplicate is a degenerate overlap, matching Rust's
    // conflicting-implementations error for exact duplicates.
    impls_overlap(db, pkg_id, a, b, aliases)
}

/// Conservative symmetric overlap test over two impls of the same interface.
///
/// Two impls overlap iff their *subjects* — the for-type plus the interface
/// type-args — have a **common instance**: one concrete type + arg list that
/// both impls would apply to. We decide this by first-order unification with
/// *both* impls' generic params as fresh unification variables (renamed to
/// disjoint names so they can bind on either side). This finds complementary
/// pairs like `Pair<T, int>` vs `Pair<string, U>` (common instance
/// `Pair<string, int>`) that a one-directional matcher misses.
///
/// Associated bindings are interface *outputs*, so only the args participate.
///
/// Once the subjects unify, a bound on a param the unifier pinned to a *ground*
/// type is checked against the *specific* bound interface (args included) via the
/// canonical `get_implements_block` resolver: if that type does not implement the
/// instantiated bound, the bounded impl cannot apply to the common instance, so the
/// impls are disjoint. Bounds on params that remain variables — or whose own args
/// stay unresolved — are conservative: without negative impls we cannot prove two
/// bounded blankets disjoint, so they are assumed to overlap.
fn impls_overlap<'db>(
    db: &'db dyn crate::Db,
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
    let mut vars: Vec<Name> = Vec::with_capacity(a.generic_params.len() + b.generic_params.len());
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
    // carries a bound the common instance provably violates (a ground subject that
    // does not satisfy its bound). That conclusion holds even if `result` is
    // `Unknown`, so it can override to `No`.
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
/// to a *forced* ground type is provably unsatisfiable — which makes the two impls
/// disjoint. A bound whose subject is still a variable is undecidable in an open
/// world and is assumed satisfiable.
///
/// The disproof is only sound when the binding is *principal* (forced by structural
/// unification). A param that appears inside a union in this impl's `subject` (the
/// for-type and interface args) is instead bound by `cover_search` to *one* of several
/// possible witnesses, so disproving its bound against that single witness would
/// unsoundly reject an overlap that a different witness satisfies — those are skipped.
#[expect(
    clippy::too_many_arguments,
    reason = "one cohesive overlap-check context: query (db, pkg), this impl (rule, prefix, \
              subject), and the unifier state (vars, bindings, aliases)"
)]
fn bounds_hold_at_common_instance<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    rule: &ImplData<'db>,
    prefix: char,
    vars: &[Name],
    bindings: &TypeBindings,
    subject: &[&Ty],
    aliases: &std::collections::HashMap<TypeName, Ty>,
) -> bool {
    // Each of this impl's params, resolved to the ground type it takes at the common
    // instance (following the unifier's binding chains). Used both to read a param's own
    // witness and to instantiate a bound whose args mention sibling params.
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
        // An unpinned witness (still a coherence var, or otherwise not fully realized) is
        // undecidable in an open world — `get_implements_block` needs realized inputs.
        if contains_bound_typevar(witness, vars)
            || baml_type::RealizedTy::try_from(witness).is_err()
        {
            continue;
        }
        // The impl applies only if the witness satisfies *every* bound on the param, so a
        // single provably-unsatisfied bound makes the two impls disjoint at this instance.
        // Instantiate each bound at the common instance and check the *specific* interface
        // (args included) via the canonical resolver. We may conclude "disjoint" only when
        // the instantiated bound is itself fully realized: a `None` from `get_implements_block`
        // then means the witness genuinely does not implement it, whereas an unresolved bound
        // arg keeps the bound conservatively assumed-to-hold (a wrong negative would admit an
        // overlapping pair).
        for bound in bounds {
            let bound = bound.map_tys(|t| crate::generics::substitute_ty(t, &param_witnesses));
            if bound
                .tys()
                .all(|t| baml_type::RealizedTy::try_from(t).is_ok())
                && crate::interfaces::get_implements_block(db, pkg_id, witness, &bound, aliases)
                    .is_none()
            {
                return false;
            }
        }
    }
    true
}

/// Fresh unification-variable name for the `idx`-th generic param of the impl
/// on side `prefix`. Guillemets can't appear in user type-var names, so the two
/// impls' renamed params are guaranteed disjoint from each other and from any
/// real type.
fn renamed_var(prefix: char, idx: usize) -> Name {
    Name::new(format!("«{prefix}{idx}»"))
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
    let for_ty = nf(
        &crate::generics::substitute_ty(&rule.for_ty_pattern, &rename),
        enum_variants,
    );
    let args = rule
        .interface_args
        .iter()
        .map(|arg| nf(&crate::generics::substitute_ty(arg, &rename), enum_variants))
        .collect();
    (for_ty, args)
}
