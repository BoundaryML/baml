use baml_base::{Literal, Name, Span, TyAttr};
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use baml_type::{FunctionParamTy, Ty, TypeName};

use crate::interfaces::{
    ImplData, TypeBindings, contains_bound_typevar, impl_data, impl_data_source_map,
    interface_loc_qtn, package_impl_locs,
};

/// Three-valued result of an overlap decision. Overlap is undecidable in general
/// (it's ACI-unification — NP-hard — once unions with variables are involved), so
/// the checker can report "couldn't tell" rather than guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlap {
    /// A common instance provably exists — the impls overlap.
    Yes,
    /// No common instance can exist — the impls are provably disjoint.
    No,
    /// Could not be decided within the search bounds (a too-large or
    /// variable-bearing union). Callers treat it as a possible overlap (the sound
    /// direction) but report it distinctly.
    Unknown,
}

impl Overlap {
    /// Kleene conjunction: a compound overlaps only if *every* part can —
    /// `No` dominates (one disjoint part ⇒ disjoint), then `Unknown`, then `Yes`.
    fn and(self, other: Overlap) -> Overlap {
        match (self, other) {
            (Overlap::No, _) | (_, Overlap::No) => Overlap::No,
            (Overlap::Unknown, _) | (_, Overlap::Unknown) => Overlap::Unknown,
            (Overlap::Yes, Overlap::Yes) => Overlap::Yes,
        }
    }
}

/// Budget on the number of `cover` trials the covering search may perform before it
/// gives up with `Overlap::Unknown` ("this type is too complex to decide — simplify
/// it"). ACI-unification is NP-hard, so the search is bounded rather than risking a
/// factorial blow-up. Easy cases (e.g. linear members) resolve in far fewer steps; the
/// budget is reached only by large, deeply-coupled variable-bearing unions.
const MAX_OVERLAP_SEARCH_STEPS: usize = 4096;

/// Recursion-depth cap for `unify_into` and the covering search it drives. Two
/// *distinct* recursive type aliases as impl subjects (`type R = Box<R>` /
/// `type S = Box<S>`, or a recursive union arg like `type U = int | Box<U>`)
/// expand head-first forever — `Box<R>` vs `Box<S>` is never `is_same_normalized_type`
/// (the Mu binders carry the distinct alias names), so the structural arm keeps
/// descending. Unlike a cycle that *repeats* a goal, this *grows* without repeating, so
/// a visited-set can't catch it; a fixed depth backstop (rustc's `recursion_limit`
/// approach, also used by the runtime resolver's `MAX_OBLIGATION_DEPTH`) does. Realistic
/// type nesting is shallow (single digits), so only pathological recursive aliases reach
/// this — at which point unification fails closed to `Overlap::Unknown` (→ "too complex
/// to prove disjoint; simplify"), which is sound: the pair is rejected rather than
/// crashing, and such aliases genuinely *do* overlap anyway.
const MAX_UNIFY_DEPTH: usize = 256;

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

    // Type aliases (own package plus dependency exports) so alias-referencing
    // for-types and interface args normalize before the overlap comparison.
    let mut aliases =
        crate::inference::collect_type_aliases(db, baml_compiler2_ppir::package_items(db, pkg_id));
    for dep in deps {
        for (qtn, ty) in
            crate::inference::collect_type_aliases(db, baml_compiler2_ppir::package_items(db, *dep))
        {
            aliases.entry(qtn).or_insert(ty);
        }
    }
    // Fold each alias body toward the union canonical form the overlap solver assumes
    // (`type TF = true | false` → `bool`, `type OI = 1 | int` → `int`), so an
    // alias-obscured union member is compared by the same union laws (ACI + completeness)
    // as its spelled-out form. Without this, `Bar<TF>` and `Bar<bool>` normalize to
    // structurally-different args and are wrongly judged disjoint — admitting two impls of
    // one interface for one type (a fails-open coherence hole).
    {
        let enum_variants = |qtn: &TypeName| enum_variant_names(db, qtn);
        for body in aliases.values_mut() {
            *body = nf(body, &enum_variants);
        }
    }

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
        .into_iter()
        .filter_map(|loc| {
            let data = impl_data(db, loc).as_ref().ok()?;
            let span = impl_data_source_map(db, loc)
                .as_ref()
                .map(|sm| sm.impl_span)?;
            Some((data, span))
        })
        .collect()
}

/// Resolve a chain of top-level type aliases to the underlying type via `aliases`,
/// bounded against alias cycles (those are a separate diagnostic). Only the *head* is
/// resolved — aliases nested under a constructor are handled by `is_same_normalized_type`.
/// Mirrors `expand_type_alias` in the diagnostics layer so the coherence valid-subject
/// gate sees through the same aliases the E0138 concreteness gate does.
fn expand_alias_head(ty: &Ty, aliases: &std::collections::HashMap<TypeName, Ty>) -> Ty {
    let mut current = ty.clone();
    for _ in 0..64 {
        let Ty::TypeAlias(qtn, _) = &current else {
            break;
        };
        match aliases.get(qtn) {
            Some(next) => current = next.clone(),
            None => break,
        }
    }
    current
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

/// Whether `name` occurs anywhere *inside a union* within `ty`. A bounded param that
/// does is bound by `cover_search` to one of several possible witnesses, so its
/// `chase_var` representative is non-principal — see `bounds_hold_at_common_instance`.
fn var_under_union(name: &Name, ty: &Ty) -> bool {
    fn occurs(name: &Name, ty: &Ty, in_union: bool) -> bool {
        match ty {
            Ty::TypeVar(n, _) => in_union && n == name,
            Ty::Union(members, _) => members.iter().any(|m| occurs(name, m, true)),
            Ty::Class(_, args, _) => args.iter().any(|a| occurs(name, a, in_union)),
            Ty::Interface(_, args, assoc, _) => {
                args.iter().any(|a| occurs(name, a, in_union))
                    || assoc.iter().any(|(_, t)| occurs(name, t, in_union))
            }
            Ty::List(inner, _) | Ty::EvolvingList(inner, _) => occurs(name, inner, in_union),
            Ty::Map {
                key: k, value: v, ..
            }
            | Ty::EvolvingMap(k, v, _)
            | Ty::Future(k, v, _) => occurs(name, k, in_union) || occurs(name, v, in_union),
            Ty::AssociatedTypeProjection {
                base, interface, ..
            } => occurs(name, base, in_union) || interface.tys().any(|t| occurs(name, t, in_union)),
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                params
                    .iter()
                    .any(|FunctionParamTy { ty, .. }| occurs(name, ty, in_union))
                    || occurs(name, ret, in_union)
                    || occurs(name, throws, in_union)
            }
            // Leaf types hold no nested type, so no variable occurs in them. Listed
            // explicitly (no total wildcard) so a new `Ty` variant must be classified.
            Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::Literal(..)
            | Ty::Enum(..)
            | Ty::EnumVariant(..)
            | Ty::RustType { .. }
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. }
            | Ty::Void { .. }
            | Ty::TypeAlias(..)
            | Ty::BuiltinUnknown { .. }
            | Ty::Never { .. }
            | Ty::Unknown { .. }
            | Ty::Error { .. }
            | Ty::Infer { .. } => false,
        }
    }
    occurs(name, ty, false)
}

/// Fresh unification-variable name for the `idx`-th generic param of the impl
/// on side `prefix`. Guillemets can't appear in user type-var names, so the two
/// impls' renamed params are guaranteed disjoint from each other and from any
/// real type.
fn renamed_var(prefix: char, idx: usize) -> Name {
    Name::new(format!("«{prefix}{idx}»"))
}

/// Looks up an enum's full set of variant names (`None` if it can't be resolved).
type EnumVariants<'a> = &'a dyn Fn(&TypeName) -> Option<Vec<Name>>;

/// Normalize an impl subject toward the union canonical form the covering solver assumes
/// (the db-aware part of CNF), recursing into every argument. For unions this flattens
/// nested unions, drops `never`, absorbs `unknown`, deduplicates, drops members subsumed
/// by a co-member (`1 | int → int`, `Color.Red | Color → Color`), and folds a *complete*
/// finite base back to its base (`true | false → bool`; all of an enum's variants → the
/// enum, via `enum_variants`). A var-bearing union opposite a finite base — which cannot
/// be folded away — is handled conservatively in `unify_into`.
fn nf(ty: &Ty, enum_variants: EnumVariants) -> Ty {
    match ty {
        Ty::Union(members, attr) => normalize_union(
            members.iter().map(|m| nf(m, enum_variants)).collect(),
            attr.clone(),
            enum_variants,
        ),
        Ty::List(inner, attr) => Ty::List(Box::new(nf(inner, enum_variants)), attr.clone()),
        Ty::EvolvingList(inner, attr) => {
            Ty::EvolvingList(Box::new(nf(inner, enum_variants)), attr.clone())
        }
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(nf(key, enum_variants)),
            value: Box::new(nf(value, enum_variants)),
            attr: attr.clone(),
        },
        Ty::EvolvingMap(k, v, attr) => Ty::EvolvingMap(
            Box::new(nf(k, enum_variants)),
            Box::new(nf(v, enum_variants)),
            attr.clone(),
        ),
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(nf(value, enum_variants)),
            Box::new(nf(error, enum_variants)),
            attr.clone(),
        ),
        Ty::Class(name, args, attr) => Ty::Class(
            name.clone(),
            args.iter().map(|a| nf(a, enum_variants)).collect(),
            attr.clone(),
        ),
        Ty::Interface(name, args, bindings, attr) => Ty::Interface(
            name.clone(),
            args.iter().map(|a| nf(a, enum_variants)).collect(),
            bindings
                .iter()
                .map(|(n, t)| (n.clone(), nf(t, enum_variants)))
                .collect(),
            attr.clone(),
        ),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => Ty::AssociatedTypeProjection {
            base: Box::new(nf(base, enum_variants)),
            interface: Box::new(interface.map_tys(|t| nf(t, enum_variants))),
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
                    ty: nf(&p.ty, enum_variants),
                    mode: p.mode,
                })
                .collect(),
            ret: Box::new(nf(ret, enum_variants)),
            throws: Box::new(nf(throws, enum_variants)),
            attr: attr.clone(),
        },
        _ => ty.clone(),
    }
}

/// The union laws of [`nf`]. `members` are already individually normalized.
fn normalize_union(members: Vec<Ty>, attr: TyAttr, enum_variants: EnumVariants) -> Ty {
    // Flatten, drop `never`, absorb `unknown`, deduplicate.
    let mut flat: Vec<Ty> = Vec::new();
    for member in members {
        match member {
            Ty::Never { .. } => {}
            Ty::BuiltinUnknown { .. } => return Ty::BuiltinUnknown { attr },
            Ty::Union(inner, _) => {
                for inner_member in inner {
                    match inner_member {
                        Ty::Never { .. } => {}
                        Ty::BuiltinUnknown { .. } => return Ty::BuiltinUnknown { attr },
                        other if !flat.contains(&other) => flat.push(other),
                        _ => {}
                    }
                }
            }
            other if !flat.contains(&other) => flat.push(other),
            _ => {}
        }
    }

    // Drop members subsumed by a co-member (`literal <: base`, `variant <: enum`).
    flat = (0..flat.len())
        .filter(|&i| !(0..flat.len()).any(|j| i != j && is_literal_subtype(&flat[i], &flat[j])))
        .map(|i| flat[i].clone())
        .collect();

    fold_finite_bases(&mut flat, enum_variants);

    // Canonical member order: overlap decisions are order-insensitive (they go through
    // `is_same_normalized_type` / set-covering), but a deterministic order keeps `nf`'s
    // output stable across runs, avoiding spurious Salsa-cache churn.
    flat.sort();

    match flat.len() {
        0 => Ty::Never { attr },
        1 => flat
            .pop()
            .unwrap_or_else(|| unreachable!("a length-1 vec has an element")),
        _ => Ty::Union(flat, attr),
    }
}

/// Fold complete finite bases in a flattened, deduplicated member list: `true | false`
/// becomes `bool`, and an enum all of whose variants are present becomes the enum.
fn fold_finite_bases(flat: &mut Vec<Ty>, enum_variants: EnumVariants) {
    let has_bool_literal = |value: bool| {
        flat.iter()
            .any(|m| matches!(m, Ty::Literal(Literal::Bool(v), _, _) if *v == value))
    };
    if has_bool_literal(true) && has_bool_literal(false) {
        flat.retain(|m| !matches!(m, Ty::Literal(Literal::Bool(_), _, _)));
        flat.push(Ty::Bool {
            attr: TyAttr::default(),
        });
    }

    // Each enum that has a variant member: fold if *all* its variants are present.
    let mut enums: Vec<TypeName> = Vec::new();
    for member in flat.iter() {
        if let Ty::EnumVariant(enum_name, _, _) = member
            && !enums.contains(enum_name)
        {
            enums.push(enum_name.clone());
        }
    }
    for enum_name in enums {
        let Some(all_variants) = enum_variants(&enum_name) else {
            continue;
        };
        if all_variants.is_empty() {
            continue;
        }
        let present: std::collections::HashSet<&Name> = flat
            .iter()
            .filter_map(|m| match m {
                Ty::EnumVariant(en, v, _) if *en == enum_name => Some(v),
                _ => None,
            })
            .collect();
        if all_variants.iter().all(|v| present.contains(v)) {
            flat.retain(|m| !matches!(m, Ty::EnumVariant(en, _, _) if *en == enum_name));
            flat.push(Ty::Enum(enum_name.clone(), TyAttr::default()));
        }
    }
}

/// Resolve an enum's full set of variant names, or `None` if it can't be resolved. Used
/// by `nf` to fold a complete variant union (`Cmp.Less | Cmp.Equal | Cmp.More`) back to
/// its enum (`Cmp`).
fn enum_variant_names(db: &dyn crate::Db, enum_qtn: &TypeName) -> Option<Vec<Name>> {
    let package_id = PackageId::new(db, enum_qtn.package().clone());
    let items = baml_compiler2_hir::package::package_items(db, package_id);
    let Some(Definition::Enum(enum_loc)) = items.lookup_type(enum_qtn.namespace(), enum_qtn.name())
    else {
        return None;
    };
    let file = enum_loc.file(db);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let enum_data = &item_tree[enum_loc.id(db)];
    Some(enum_data.variants.iter().map(|v| v.name.clone()).collect())
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

/// Symmetric first-order **equality** unification: is there a substitution of the
/// unification variables `vars` (the combined, renamed-disjoint var set of the two
/// impls; either side may bind) that makes `x` and `y` the *same* type? Returns the
/// tri-state `Overlap`, committing the binding on `Yes`. This is the structural engine
/// of the overlap check — invariant constructor args and the for-types must be *equal*
/// for the two impls to share a common instance.
///
/// Equality, not subtyping: `int` and `Literal(1)` are distinct types here (`No`), as
/// are `K<int>` and `K<1>`. The `literal <: base` / `variant <: enum` subtyping lives in
/// `cover` (the covering oracle, its only consumer). Variants with no structural arm
/// fall through to `No`: anything equal already unified above via the normalizer, so the
/// rest are disjoint — conservative, losing precision only for var-bearing
/// function-typed args (never impl subjects today), treated as disjoint.
/// Public entry: unify two types starting at depth 0. Delegates to the recursive
/// worker [`unify_into_at`]; this wrapper keeps the (many) call sites and tests
/// depth-agnostic.
fn unify_into(
    x: &Ty,
    y: &Ty,
    vars: &[Name],
    aliases: &std::collections::HashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    unify_into_at(x, y, vars, aliases, bindings, 0)
}

/// Recursive unification worker. `depth` accumulates across the whole mutually-recursive
/// cycle (structural args *and* the covering search), so a non-terminating recursive-alias
/// expansion is caught by the [`MAX_UNIFY_DEPTH`] backstop rather than overflowing the
/// stack.
fn unify_into_at(
    x: &Ty,
    y: &Ty,
    vars: &[Name],
    aliases: &std::collections::HashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
    depth: usize,
) -> Overlap {
    if depth >= MAX_UNIFY_DEPTH {
        return Overlap::Unknown;
    }
    let x = chase_var(x, vars, bindings);
    let y = chase_var(y, vars, bindings);
    // Resolve a type alias to its definition before matching so the structural arms and
    // variable binding see through it — e.g. blanket `Box<T>` vs `Box<int>` spelled via
    // `type BI = Box<int>` must unify `T = int`. (`is_same_normalized_type` below also
    // resolves aliases, but only for the exact-equality fast path; the var-binding
    // structural match needs the alias gone too, or it falls to the disjoint arm.)
    let x = expand_alias_head(&x, aliases);
    let y = expand_alias_head(&y, aliases);

    // The error sentinels never unify: an unresolved for-type or arg (`Unknown`)
    // or a type that already errored (`Error`) carries its own diagnostic, so
    // treating it as a common instance would stack a spurious overlap — and
    // `Error` is "compatible with anything" downstream, which would *admit* a
    // bogus overlap (the dangerous direction). The *inhabited* top type `unknown`
    // (`BuiltinUnknown`) is deliberately not bailed here: it binds an opposing
    // variable (below), and is otherwise a distinct atomic type compared by
    // equality — `Box<unknown>` is disjoint from `Box<int>`, exactly how the
    // runtime resolver matches it.
    if matches!(x, Ty::Unknown { .. } | Ty::Error { .. })
        || matches!(y, Ty::Unknown { .. } | Ty::Error { .. })
    {
        return Overlap::No;
    }

    if let Ty::TypeVar(n, _) = &x
        && vars.contains(n)
    {
        return bind_unify_var(n, &y, vars, aliases, bindings, depth + 1);
    }
    if let Ty::TypeVar(n, _) = &y
        && vars.contains(n)
    {
        return bind_unify_var(n, &x, vars, aliases, bindings, depth + 1);
    }

    // Structurally-equal (or alias-equal) subjects unify with no new bindings;
    // this also resolves ground unions order-insensitively via the normalizer.
    if crate::normalize::is_same_normalized_type(&x, &y, aliases) {
        return Overlap::Yes;
    }

    match (&x, &y) {
        (Ty::Class(xq, xa, _), Ty::Class(yq, ya, _)) if xq == yq && xa.len() == ya.len() => {
            unify_all(xa, ya, vars, aliases, bindings, depth + 1)
        }
        (Ty::Interface(xq, xa, xb, _), Ty::Interface(yq, ya, yb, _))
            if xq == yq && xa.len() == ya.len() =>
        {
            // Generic args *and* associated bindings are part of an interface-existential
            // type's identity: `I<Item=int>` and `I<Item=string>` are distinct (disjoint)
            // types, because coherence gives each concrete type a single `impl I`, hence
            // one `Item`. (Distinct from the *impl's own* interface, where the bindings
            // are outputs and dropped by `renamed_subject`.)
            unify_all(xa, ya, vars, aliases, bindings, depth + 1).and(unify_associated_bindings(
                xb,
                yb,
                vars,
                aliases,
                bindings,
                depth + 1,
            ))
        }
        (Ty::List(xi, _), Ty::List(yi, _)) | (Ty::EvolvingList(xi, _), Ty::EvolvingList(yi, _)) => {
            unify_into_at(xi, yi, vars, aliases, bindings, depth + 1)
        }
        (
            Ty::Map {
                key: xk, value: xv, ..
            },
            Ty::Map {
                key: yk, value: yv, ..
            },
        )
        | (Ty::EvolvingMap(xk, xv, _), Ty::EvolvingMap(yk, yv, _)) => unify_into_at(
            xk,
            yk,
            vars,
            aliases,
            bindings,
            depth + 1,
        )
        .and(unify_into_at(xv, yv, vars, aliases, bindings, depth + 1)),
        (Ty::Future(xv, xe, _), Ty::Future(yv, ye, _)) => unify_into_at(
            xv,
            yv,
            vars,
            aliases,
            bindings,
            depth + 1,
        )
        .and(unify_into_at(xe, ye, vars, aliases, bindings, depth + 1)),
        (
            Ty::Function {
                params: xp,
                ret: xr,
                throws: xt,
                ..
            },
            Ty::Function {
                params: yp,
                ret: yr,
                throws: yt,
                ..
            },
        ) if xp.len() == yp.len() && xp.iter().zip(yp.iter()).all(|(p, q)| p.mode == q.mode) => {
            // Function values are realized (no binders of their own), so unify the
            // param/ret/throws components directly. Coherence vars ride along in those
            // positions, so a var-bearing function arg (`(T) -> int` vs `(int) -> int`)
            // unifies at `T = int` instead of falling to the disjoint arm below and
            // wrongly admitting two impls. Mirrors the dispatch matcher's `Function` arm.
            let mut result = Overlap::Yes;
            for (p, q) in xp.iter().zip(yp.iter()) {
                result = result.and(unify_into_at(
                    &p.ty,
                    &q.ty,
                    vars,
                    aliases,
                    bindings,
                    depth + 1,
                ));
            }
            result
                .and(unify_into_at(xr, yr, vars, aliases, bindings, depth + 1))
                .and(unify_into_at(xt, yt, vars, aliases, bindings, depth + 1))
        }
        // Unions compare by covering on their member sets (ACI), so a non-union
        // operand is treated as the singleton union `{S}` and routed through the same
        // covering. That decides a variable- or literal-bearing union opposite a single
        // type precisely: `1 | T` vs `int` overlaps at `T = int` (`1 <: int` collapses),
        // `true | T` vs `bool` at `T = false`, and `C | T` vs `C` at `T = C`
        // (idempotency), while `D | T` vs `C` (with `C ≠ D`) stays disjoint. Routing
        // *every* union here — not only `Union` vs `Union` — is what stops a var-bearing
        // union opposite a single type from being wrongly judged disjoint by the
        // wildcard arm below. (The finite-base residual `true | T` / `Cmp.Less | T` is
        // covered precisely via `cover`'s `literal <: base` / `variant <: enum` oracle.)
        (Ty::Union(xm, _), Ty::Union(ym, _)) => {
            unify_union_members_at(xm, ym, vars, aliases, bindings, depth + 1)
        }
        (Ty::Union(xm, _), _) => unify_union_members_at(
            xm,
            std::slice::from_ref(&y),
            vars,
            aliases,
            bindings,
            depth + 1,
        ),
        (_, Ty::Union(ym, _)) => unify_union_members_at(
            std::slice::from_ref(&x),
            ym,
            vars,
            aliases,
            bindings,
            depth + 1,
        ),
        // An associated-type projection could stand for any concrete type, so under the
        // possible-worlds view there IS an instantiation where it coincides with the
        // opposing type — i.e. the impls *could* overlap. Answer `Yes` (conservatively
        // reject as overlapping), NOT `Unknown` (which is reserved for search-budget
        // exhaustion and renders a different "too complex — simplify" diagnostic). Today
        // projections in impl headers are `CyclicHeader`-rejected before reaching here,
        // so this is defense-in-depth against a future path that admits them.
        (Ty::AssociatedTypeProjection { .. }, _) | (_, Ty::AssociatedTypeProjection { .. }) => {
            Overlap::Yes
        }
        // Everything else is disjoint under equality: equal subjects already unified
        // above (the normalizer), unions are handled above, and any remaining pair is
        // distinct types. A literal and its base are *distinct* here — `unify_into`
        // decides equality, so `int ≢ 1`; the `literal <: base` subtyping is `cover`'s
        // job (the covering oracle), not equality's. A same-constructor pair whose guard
        // failed (different name/arity) or whose other side isn't that constructor lands
        // here too. Every variant is named (no total wildcard) so a new `Ty` must be
        // classified here rather than silently treated as disjoint.
        (
            Ty::Class(..)
            | Ty::Interface(..)
            | Ty::List(..)
            | Ty::EvolvingList(..)
            | Ty::Map { .. }
            | Ty::EvolvingMap(..)
            | Ty::Future(..)
            | Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::Literal(..)
            | Ty::Enum(..)
            | Ty::EnumVariant(..)
            | Ty::Function { .. }
            | Ty::RustType { .. }
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. }
            | Ty::Void { .. }
            | Ty::TypeAlias(..)
            | Ty::TypeVar(..)
            | Ty::BuiltinUnknown { .. }
            | Ty::Never { .. }
            | Ty::Unknown { .. }
            | Ty::Error { .. }
            | Ty::Infer { .. },
            _,
        ) => Overlap::No,
    }
}

/// Unify two equal-length type-argument lists position-wise (a conjunction): a
/// disjoint position short-circuits to `No`; an undecidable one downgrades the
/// result to `Unknown`.
fn unify_all(
    xs: &[Ty],
    ys: &[Ty],
    vars: &[Name],
    aliases: &std::collections::HashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
    depth: usize,
) -> Overlap {
    debug_assert_eq!(xs.len(), ys.len());
    let mut result = Overlap::Yes;
    for (x, y) in xs.iter().zip(ys.iter()) {
        match unify_into_at(x, y, vars, aliases, bindings, depth) {
            Overlap::No => return Overlap::No,
            Overlap::Unknown => result = Overlap::Unknown,
            Overlap::Yes => {}
        }
    }
    result
}

/// Unify two interface-existentials' associated bindings (`Item=…`) — a conjunction
/// over names common to both. A name on only one side does not constrain (conservative:
/// well-formed existentials specify the same associated types, so this is exact in
/// practice; a missing name only ever loosens toward `Yes`, never a wrong `No`).
fn unify_associated_bindings(
    xb: &[(Name, Ty)],
    yb: &[(Name, Ty)],
    vars: &[Name],
    aliases: &std::collections::HashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
    depth: usize,
) -> Overlap {
    let mut result = Overlap::Yes;
    for (name, xty) in xb {
        if let Some((_, yty)) = yb.iter().find(|(n, _)| n == name) {
            match unify_into_at(xty, yty, vars, aliases, bindings, depth) {
                Overlap::No => return Overlap::No,
                Overlap::Unknown => result = Overlap::Unknown,
                Overlap::Yes => {}
            }
        }
    }
    result
}

/// Unify two unions — the `(Union, Union)` arm of `unify_into`. The overlap check asks
/// whether two impl subjects share a common instance; at a union position that is: does
/// some substitution of the unification variables unify these two unions into one type?
/// Returns the tri-state `Overlap` (`Yes`/`No` when provable, `Unknown` when the search
/// is truncated). Only `cover_search` writes to `bindings` (committing the bindings it
/// discovers on a `Yes`); the all-ground and both-bare special-cases return `Yes` without
/// touching `bindings`, and `No`/`Unknown` leave it unchanged — so a `Yes` does not imply
/// a witness substitution was recorded.
///
/// Unions are ACI (associative, commutative, idempotent), hence *sets*: equality
/// ignores order and duplicates, and a bare top-level type-variable member can be any
/// type (including a union), so it may absorb several members at once. Deciding this is
/// therefore ACI-unification, which is NP-hard; the search is bounded by
/// `MAX_OVERLAP_SEARCH_STEPS`, past which it yields `Unknown` ("type too complex;
/// simplify it") - the only thing that ever produces `Unknown`.
///
/// Members are matched by **covering**, not a one-to-one pairing: idempotency
/// (`A | A = A`) lets several members collapse onto one, so a member need only unify with
/// *some* member of the other side (many-to-one), not a private partner. An injective or
/// bijective match would unsoundly reject collapses like `{C<T>, C<U>, C<W>}` vs
/// `{C<X>, C<Y>}` (unifiable via `T=X, U=Y, W=X`) — do not "simplify" it back. Cheap
/// special-cases run first (all-ground reduces to set equality; a bare variable on *each*
/// side is immediately `Yes`, as each absorbs the other); otherwise the covering
/// obligations — one-directional when a bare variable absorbs one side, mutual when
/// neither does — are solved jointly by `cover_search`.
fn unify_union_members_at(
    xs: &[Ty],
    ys: &[Ty],
    vars: &[Name],
    aliases: &std::collections::HashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
    depth: usize,
) -> Overlap {
    // No variable on either side: overlap is exact set equality (any size).
    if let Some(result) = try_union_set_equality(xs, ys, vars, aliases) {
        return result;
    }
    // A bare variable on *each* side absorbs the other entirely ⇒ always overlap.
    if let Some(result) = try_union_mutual_absorption(xs, ys, vars) {
        return result;
    }

    let rigids = |members: &[Ty]| -> Vec<Ty> {
        members
            .iter()
            .filter(|m| !is_bare_var(m, vars))
            .cloned()
            .collect()
    };
    let xn = rigids(xs);
    let yn = rigids(ys);
    let x_has_bare = xs.iter().any(|m| is_bare_var(m, vars));
    let y_has_bare = ys.iter().any(|m| is_bare_var(m, vars));

    // Build the covering obligations. A side with a bare variable absorbs the *other*
    // side's leftovers, so only its own rigids must be covered (one direction). With no
    // bare variable anywhere, equality requires *mutual* covering (both directions),
    // solved jointly so the substitution chosen for one direction is consistent with
    // the other (a greedy two-pass would be unsound).
    let mut obligations: Vec<(Ty, Vec<Ty>)> = Vec::new();
    if x_has_bare {
        for m in &xn {
            obligations.push((m.clone(), yn.clone()));
        }
    } else if y_has_bare {
        for m in &yn {
            obligations.push((m.clone(), xn.clone()));
        }
    } else {
        for m in &xn {
            obligations.push((m.clone(), yn.clone()));
        }
        for m in &yn {
            obligations.push((m.clone(), xn.clone()));
        }
    }

    let mut budget = MAX_OVERLAP_SEARCH_STEPS;
    cover_search(&obligations, vars, aliases, bindings, &mut budget, depth)
}

/// Test-only entry to [`unify_union_members_at`] starting at depth 0.
#[cfg(test)]
fn unify_union_members(
    xs: &[Ty],
    ys: &[Ty],
    vars: &[Name],
    aliases: &std::collections::HashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    unify_union_members_at(xs, ys, vars, aliases, bindings, 0)
}

/// Special case: with no variable on either side, overlap is exact set equality —
/// decidable precisely at any size with no search. `None` if a variable is present.
fn try_union_set_equality(
    xs: &[Ty],
    ys: &[Ty],
    vars: &[Name],
    aliases: &std::collections::HashMap<TypeName, Ty>,
) -> Option<Overlap> {
    let has_var = |members: &[Ty]| members.iter().any(|m| contains_bound_typevar(m, vars));
    if has_var(xs) || has_var(ys) {
        return None;
    }
    Some(if unions_set_equal(xs, ys, aliases) {
        Overlap::Yes
    } else {
        Overlap::No
    })
}

/// Special case: a bare (top-level) variable can be instantiated to a *union*, so
/// it absorbs any set of the other side's members. If both sides have one, a
/// common instance always exists — a proven overlap, regardless of whether any
/// package instantiates it that way. `None` if either side lacks a bare variable.
fn try_union_mutual_absorption(xs: &[Ty], ys: &[Ty], vars: &[Name]) -> Option<Overlap> {
    let has_bare = |members: &[Ty]| members.iter().any(|m| is_bare_var(m, vars));
    (has_bare(xs) && has_bare(ys)).then_some(Overlap::Yes)
}

/// True iff `m` is a bare unification-variable member — a top-level type variable
/// in `vars`, as opposed to a variable nested inside a constructor.
fn is_bare_var(m: &Ty, vars: &[Name]) -> bool {
    matches!(m, Ty::TypeVar(n, _) if vars.contains(n))
}

/// Whether two ground unions denote the same set of types (order-insensitive).
/// Members are de-duplicated, so equal cardinality plus "every member of `xs`
/// has an equal member in `ys`" implies a bijection.
fn unions_set_equal(
    xs: &[Ty],
    ys: &[Ty],
    aliases: &std::collections::HashMap<TypeName, Ty>,
) -> bool {
    xs.len() == ys.len()
        && xs.iter().all(|x| {
            ys.iter()
                .any(|y| crate::normalize::is_same_normalized_type(x, y, aliases))
        })
}

/// Whether `member` can be a *subtype* of `candidate` under some substitution
/// (committing the bindings on success). Covering uses subtype, not equality, because a
/// union member lies inside the other union iff it is a subtype of one of its members.
///
/// `unify_into` supplies the bulk (equality ⟹ subtype: same-constructor members,
/// variable binding, ground equality, and same-name interfaces via their generic args +
/// associated bindings). On top of it sit the two top-level subtypings that invariance
/// leaves above the constructor level — `literal <: base` and `variant <: enum`
/// (`is_literal_subtype`) — and the pairs `unify_into` cannot decide without the impl
/// registry — a concrete type vs an interface, two *different* interfaces, an opaque
/// `$rust_type` (`needs_conservative_membership`) — which are treated conservatively as
/// a *possible* overlap (`Yes`), never a wrong `No`. All of these arms only fire when
/// `unify_into` already returned `No` without binding anything, so they leave `bindings`
/// untouched. Registry-precise `C implements I` / `I requires J` is a later refinement.
fn cover_at(
    member: &Ty,
    candidate: &Ty,
    vars: &[Name],
    aliases: &std::collections::HashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
    depth: usize,
) -> Overlap {
    match unify_into_at(member, candidate, vars, aliases, bindings, depth) {
        Overlap::No
            if is_literal_subtype(member, candidate)
                || needs_conservative_membership(member, candidate) =>
        {
            Overlap::Yes
        }
        decided => decided,
    }
}

/// Test-only entry to [`cover_at`] starting at depth 0.
#[cfg(test)]
fn cover(
    member: &Ty,
    candidate: &Ty,
    vars: &[Name],
    aliases: &std::collections::HashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    cover_at(member, candidate, vars, aliases, bindings, 0)
}

/// Whether `member` is a top-level subtype of `candidate` by the only subtypings
/// invariance leaves above the constructor level: a literal is a subtype of its base
/// primitive (`1 <: int`), and an enum variant is a subtype of its enum
/// (`Color.Red <: Color`). Directional — `int` is *not* a subtype of `1`.
fn is_literal_subtype(member: &Ty, candidate: &Ty) -> bool {
    match (member, candidate) {
        (Ty::Literal(Literal::Int(_), _, _), Ty::Int { .. })
        | (Ty::Literal(Literal::Bigint(_), _, _), Ty::Bigint { .. })
        | (Ty::Literal(Literal::Float(_), _, _), Ty::Float { .. })
        | (Ty::Literal(Literal::String(_), _, _), Ty::String { .. })
        | (Ty::Literal(Literal::Bool(_), _, _), Ty::Bool { .. }) => true,
        (Ty::EnumVariant(variant_enum, _, _), Ty::Enum(base_enum, _)) => variant_enum == base_enum,
        _ => false,
    }
}

/// Whether this pair's subtyping cannot be decided without the impl registry, so the
/// covering oracle must fall back to a conservative `Yes`: a concrete type vs an
/// interface (needs `C implements I`), two *different* interfaces (needs `I requires J`),
/// or an opaque `$rust_type`. Two interfaces of the *same* name are decided precisely by
/// `unify_into` (generic args + associated bindings), so they are **not** conservative.
fn needs_conservative_membership(a: &Ty, b: &Ty) -> bool {
    if matches!(a, Ty::RustType { .. }) || matches!(b, Ty::RustType { .. }) {
        return true;
    }
    match (a, b) {
        (Ty::Interface(qa, ..), Ty::Interface(qb, ..)) => qa != qb,
        (Ty::Interface(..), _) | (_, Ty::Interface(..)) => true,
        _ => false,
    }
}

/// Solve the covering obligations jointly: is there one substitution under which every
/// `(member, candidates)` obligation holds — `member` a subtype of some candidate? A
/// candidate may cover several members (covering is many-to-one), and obligations may
/// carry different candidate sets, so this serves both the one-directional case (a bare
/// var absorbs one side) and the mutual case (no bare var).
///
/// Picks the most-constrained obligation first (MRV): a single viable candidate is
/// forced (unit propagation), none fails fast (`No`), otherwise it backtracks over the
/// candidates. `budget` caps the number of `cover` trials; exhausting it yields
/// `Overlap::Unknown` — the NP-hard ceiling — so easy cases (e.g. linear members)
/// decide exactly while pathological ones degrade to "simplify the type".
fn cover_search(
    obligations: &[(Ty, Vec<Ty>)],
    vars: &[Name],
    aliases: &std::collections::HashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
    budget: &mut usize,
    depth: usize,
) -> Overlap {
    if obligations.is_empty() {
        return Overlap::Yes;
    }

    // Choose the obligation with the fewest viable candidates; fail fast on a member
    // that nothing can cover.
    let mut chosen: Option<(usize, Vec<usize>)> = None;
    for (oi, (member, candidates)) in obligations.iter().enumerate() {
        let mut viable: Vec<usize> = Vec::new();
        for (ci, candidate) in candidates.iter().enumerate() {
            if *budget == 0 {
                return Overlap::Unknown;
            }
            *budget -= 1;
            let mut trial = bindings.clone();
            if cover_at(member, candidate, vars, aliases, &mut trial, depth) != Overlap::No {
                viable.push(ci);
            }
        }
        if viable.is_empty() {
            return Overlap::No;
        }
        let improves = match &chosen {
            None => true,
            Some((_, best)) => viable.len() < best.len(),
        };
        if improves {
            let forced = viable.len() == 1;
            chosen = Some((oi, viable));
            if forced {
                break; // can't be more constrained than a single candidate
            }
        }
    }

    let (oi, viable) =
        chosen.unwrap_or_else(|| unreachable!("non-empty obligations always yield a choice"));
    let (member, candidates) = &obligations[oi];
    let rest: Vec<(Ty, Vec<Ty>)> = obligations
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != oi)
        .map(|(_, o)| o.clone())
        .collect();

    let mut result = Overlap::No;
    for ci in viable {
        let mut trial = bindings.clone();
        // `ci` was viable, so this is `Yes`/`Unknown`, never `No`.
        let here = cover_at(member, &candidates[ci], vars, aliases, &mut trial, depth);
        match here.and(cover_search(
            &rest, vars, aliases, &mut trial, budget, depth,
        )) {
            Overlap::Yes => {
                *bindings = trial;
                return Overlap::Yes;
            }
            Overlap::Unknown => result = Overlap::Unknown,
            Overlap::No => {}
        }
    }
    result
}

/// Resolve a type through the current bindings: while it is a bound unification
/// variable, replace it with its binding (so callers see the representative).
fn chase_var(ty: &Ty, vars: &[Name], bindings: &TypeBindings) -> Ty {
    let mut current = ty.clone();
    while let Ty::TypeVar(name, _) = &current {
        if vars.contains(name)
            && let Some(bound) = bindings.get(name)
        {
            current = bound.clone();
        } else {
            break;
        }
    }
    current
}

/// Bind unification variable `n` to (already-chased) `t`, unifying with any
/// existing binding and rejecting cyclic bindings (the occurs check).
fn bind_unify_var(
    n: &Name,
    t: &Ty,
    vars: &[Name],
    aliases: &std::collections::HashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
    depth: usize,
) -> Overlap {
    if let Ty::TypeVar(tn, _) = t
        && tn == n
    {
        return Overlap::Yes;
    }
    if let Some(existing) = bindings.get(n).cloned() {
        return unify_into_at(&existing, t, vars, aliases, bindings, depth);
    }
    if occurs_in(n, t, vars, bindings) {
        return Overlap::No;
    }
    bindings.insert(n.clone(), t.clone());
    Overlap::Yes
}

/// Occurs check: does unification variable `n` appear anywhere in `t` (chasing
/// bound vars)? A positive answer means binding `n := t` would build an
/// infinite type, so the two subjects have no finite common instance.
fn occurs_in(n: &Name, t: &Ty, vars: &[Name], bindings: &TypeBindings) -> bool {
    let t = chase_var(t, vars, bindings);
    match &t {
        Ty::TypeVar(m, _) => m == n,
        Ty::Class(_, args, _) | Ty::Union(args, _) => {
            args.iter().any(|a| occurs_in(n, a, vars, bindings))
        }
        Ty::Interface(_, args, assoc, _) => {
            args.iter().any(|a| occurs_in(n, a, vars, bindings))
                || assoc.iter().any(|(_, ty)| occurs_in(n, ty, vars, bindings))
        }
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => occurs_in(n, inner, vars, bindings),
        Ty::Map {
            key: k, value: v, ..
        }
        | Ty::EvolvingMap(k, v, _)
        | Ty::Future(k, v, _) => occurs_in(n, k, vars, bindings) || occurs_in(n, v, vars, bindings),
        Ty::AssociatedTypeProjection {
            base, interface, ..
        } => {
            occurs_in(n, base, vars, bindings)
                || interface.tys().any(|t| occurs_in(n, t, vars, bindings))
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|FunctionParamTy { ty, .. }| occurs_in(n, ty, vars, bindings))
                || occurs_in(n, ret, vars, bindings)
                || occurs_in(n, throws, vars, bindings)
        }
        // Leaf types hold no nested type, so a variable cannot occur in them. Listed
        // explicitly (no total wildcard) so a new `Ty` variant must be classified here.
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Void { .. }
        | Ty::TypeAlias(..)
        | Ty::BuiltinUnknown { .. }
        | Ty::Never { .. }
        | Ty::Unknown { .. }
        | Ty::Error { .. }
        | Ty::Infer { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(name: &str, args: Vec<Ty>) -> Ty {
        Ty::Interface(
            TypeName::local(Name::new(name)),
            args,
            vec![],
            TyAttr::default(),
        )
    }

    fn interface_with_assoc(name: &str, assoc: Vec<(&str, Ty)>) -> Ty {
        Ty::Interface(
            TypeName::local(Name::new(name)),
            vec![],
            assoc
                .into_iter()
                .map(|(name, ty)| (Name::new(name), ty))
                .collect(),
            TyAttr::default(),
        )
    }

    fn int_literal(n: i64) -> Ty {
        Ty::Literal(
            Literal::Int(n),
            crate::ty::Freshness::Regular,
            TyAttr::default(),
        )
    }

    fn bool_literal(b: bool) -> Ty {
        Ty::Literal(
            Literal::Bool(b),
            crate::ty::Freshness::Regular,
            TyAttr::default(),
        )
    }

    fn enum_ty(name: &str) -> Ty {
        Ty::Enum(TypeName::local(Name::new(name)), TyAttr::default())
    }

    fn enum_variant(enum_name: &str, variant: &str) -> Ty {
        Ty::EnumVariant(
            TypeName::local(Name::new(enum_name)),
            Name::new(variant),
            TyAttr::default(),
        )
    }

    fn never() -> Ty {
        Ty::Never {
            attr: TyAttr::default(),
        }
    }

    /// Stub enum schema for `nf` tests: `Cmp` has variants `Less`, `Equal`, `More`.
    fn stub_enum_variants(qtn: &TypeName) -> Option<Vec<Name>> {
        (qtn.name().as_str() == "Cmp")
            .then(|| vec![Name::new("Less"), Name::new("Equal"), Name::new("More")])
    }

    #[test]
    fn union_overlap_both_bare_vars_is_yes() {
        // Each bare variable can absorb the other side, so a common instance
        // always exists.
        let vars = vec![Name::new("T"), Name::new("V")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![Ty::int(), Ty::type_var("T")];
        let ys = vec![Ty::string(), Ty::type_var("V")];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_variable_absorbs_extra_members_is_yes() {
        // `{int, T}` vs `{int, string, Foo}`: instantiating `T = string | Foo`
        // makes them the same union — a *provable* overlap, not indeterminate.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![Ty::int(), Ty::type_var("T")];
        let ys = vec![Ty::int(), Ty::string(), Ty::class("Foo")];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_unmatchable_rigid_member_is_no() {
        // `{int, T}` vs `{string, Foo}`: `int` matches no member on the right and
        // `T` cannot make it appear there, so there is no common instance.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![Ty::int(), Ty::type_var("T")];
        let ys = vec![Ty::string(), Ty::class("Foo")];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::No
        );
    }

    #[test]
    fn union_overlap_shared_rigid_members_extracted_is_yes() {
        // `{A1, A2, T}` vs `{A1, ..., A9}`: A1 and A2 each have a unique candidate,
        // so unit propagation peels them with no search and `T` absorbs the rest —
        // a proven overlap even though the candidate set is large.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![Ty::class("A1"), Ty::class("A2"), Ty::type_var("T")];
        let ys: Vec<Ty> = (1..=9).map(|i| Ty::class(&format!("A{i}"))).collect();
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_linear_large_residual_is_yes() {
        // `{List<T>, List<U>, V}` vs `{List<A1>, ..., List<A9>}`: `T` and `U` are
        // independent (each in one member), so covering is many-to-one and a witness
        // exists (e.g. `T=U=A1`, with `V` absorbing the rest) — a *provable* overlap,
        // not an NP-hard cap. The search finds it in a few steps.
        let vars = vec![Name::new("T"), Name::new("U"), Name::new("V")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![
            Ty::list(Ty::type_var("T")),
            Ty::list(Ty::type_var("U")),
            Ty::type_var("V"),
        ];
        let ys: Vec<Ty> = (1..=9)
            .map(|i| Ty::list(Ty::class(&format!("A{i}"))))
            .collect();
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_collapsing_members_is_yes() {
        // `{List<T>, List<U>, List<W>}` vs `{List<A1>, List<A2>}` (no bare var):
        // idempotency lets two members collapse, so `T=A1, U=A2, W=A1` makes the unions
        // equal. An injective matcher (the old model) wrongly rejected this; covering
        // (many-to-one, mutual) accepts it.
        let vars = vec![Name::new("T"), Name::new("U"), Name::new("W")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![
            Ty::list(Ty::type_var("T")),
            Ty::list(Ty::type_var("U")),
            Ty::list(Ty::type_var("W")),
        ];
        let ys = vec![Ty::list(Ty::class("A1")), Ty::list(Ty::class("A2"))];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_oversized_search_is_unknown() {
        // Unknown via the *breadth* of the candidate set (contrast the pigeonhole test,
        // which is Unknown via search *depth*): `{Pair<T,A1>, Pair<T,A2>}` share `T`, and
        // the candidates pair `A1`/`A2` with disjoint left classes so no single `T`
        // works — but just scanning the huge candidate list exhausts the step budget
        // before the search can prove it ⇒ Unknown.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let pair = |a: Ty, b: Ty| Ty::user_class_with_args("Pair", vec![a, b]);
        let a1 = || Ty::class("A1");
        let a2 = || Ty::class("A2");
        let xs = vec![pair(Ty::type_var("T"), a1()), pair(Ty::type_var("T"), a2())];
        let mut ys: Vec<Ty> = Vec::new();
        for i in 0..2050 {
            ys.push(pair(Ty::class(&format!("L{i}")), a1()));
            ys.push(pair(Ty::class(&format!("R{i}")), a2()));
        }
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Unknown
        );
    }

    #[test]
    fn union_overlap_literal_covered_by_base_is_yes() {
        // `{1, T}` vs `{int, string}`: the literal `1` is a *subtype* of `int`, so it is
        // covered (covering uses subtype, not equality), and `T` absorbs the rest.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![int_literal(1), Ty::type_var("T")];
        let ys = vec![Ty::int(), Ty::string()];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_base_not_covered_by_literal_is_no() {
        // `{int, T}` vs `{1, string}`: subtyping is directional — `int` is *not* a
        // subtype of the literal `1`, and `T` is on the left, so nothing covers `int`.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![Ty::int(), Ty::type_var("T")];
        let ys = vec![int_literal(1), Ty::string()];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::No
        );
    }

    #[test]
    fn interface_distinct_associated_binding_is_disjoint() {
        // `I<Item=int>` and `I<Item=string>` are distinct existential types — the
        // associated binding is part of the type's identity (one `impl I` per concrete
        // type ⇒ one `Item`), so they are provably disjoint.
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let a = interface_with_assoc("I", vec![("Item", Ty::int())]);
        let b = interface_with_assoc("I", vec![("Item", Ty::string())]);
        assert_eq!(
            unify_into(&a, &b, &[], &aliases, &mut bindings),
            Overlap::No
        );
    }

    #[test]
    fn interface_associated_binding_unifies_variable() {
        // `I<Item=int>` unifies with `I<Item=T>` by binding `T = int`.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let a = interface_with_assoc("I", vec![("Item", Ty::int())]);
        let b = interface_with_assoc("I", vec![("Item", Ty::type_var("T"))]);
        assert_eq!(
            unify_into(&a, &b, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
        assert_eq!(bindings.get(&Name::new("T")), Some(&Ty::int()));
    }

    #[test]
    fn distinct_recursive_alias_subjects_terminate_not_overflow() {
        // Two *distinct* recursive aliases as impl subjects — `type R = Box<R>` and
        // `type S = Box<S>` — expand head-first forever: `Box<R>` and `Box<S>` never
        // normalize equal (their Mu binders carry the distinct alias names), so the
        // structural `Class` arm keeps descending on the args. The depth backstop must
        // catch this and fail closed to `Overlap::Unknown` ("too complex to prove
        // disjoint"), rather than overflowing the stack — which is sound: the pair is
        // rejected, and these aliases in fact denote the same type.
        let r = TypeName::local(Name::new("R"));
        let s = TypeName::local(Name::new("S"));
        let mut aliases = std::collections::HashMap::default();
        aliases.insert(
            r.clone(),
            Ty::user_class_with_args("Box", vec![Ty::TypeAlias(r.clone(), TyAttr::default())]),
        );
        aliases.insert(
            s.clone(),
            Ty::user_class_with_args("Box", vec![Ty::TypeAlias(s.clone(), TyAttr::default())]),
        );
        let mut bindings = TypeBindings::default();
        assert_eq!(
            unify_into(
                &Ty::TypeAlias(r, TyAttr::default()),
                &Ty::TypeAlias(s, TyAttr::default()),
                &[],
                &aliases,
                &mut bindings,
            ),
            Overlap::Unknown,
        );
    }

    #[test]
    fn cover_distinct_interface_binding_is_not_conservative() {
        // Same-name interfaces are decided precisely by `unify_into`, so `cover` does not
        // fall back to the conservative `Yes` — `I<Item=int>` does not cover `I<Item=string>`.
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let a = interface_with_assoc("I", vec![("Item", Ty::int())]);
        let b = interface_with_assoc("I", vec![("Item", Ty::string())]);
        assert_eq!(cover(&a, &b, &[], &aliases, &mut bindings), Overlap::No);
    }

    #[test]
    fn cover_class_vs_interface_is_conservative_yes() {
        // Whether a concrete class implements an interface needs the impl registry, which
        // the solver does not yet consult here, so `cover` conservatively reports a
        // possible overlap (`Yes`) — never a wrong `No`.
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let a = Ty::class("A");
        let b = interface("I", vec![]);
        assert_eq!(cover(&a, &b, &[], &aliases, &mut bindings), Overlap::Yes);
    }

    #[test]
    fn nf_drops_never_and_collapses() {
        assert_eq!(
            nf(&Ty::union(vec![Ty::int(), never()]), &stub_enum_variants),
            Ty::int()
        );
    }

    #[test]
    fn nf_subsumes_literal_by_base() {
        assert_eq!(
            nf(
                &Ty::union(vec![int_literal(1), Ty::int()]),
                &stub_enum_variants
            ),
            Ty::int()
        );
    }

    #[test]
    fn alias_body_normalization_makes_alias_equal_to_its_folded_form() {
        // 4.1: an alias whose body is normalized at coherence map-build (`type TF =
        // true | false` → `bool`) must unify with `bool` — otherwise `Bar<TF>` vs
        // `Bar<bool>` is judged disjoint and both impls are admitted (fails open).
        let tf = TypeName::local(Name::new("TF"));
        let mut aliases = std::collections::HashMap::new();
        aliases.insert(
            tf.clone(),
            Ty::union(vec![bool_literal(true), bool_literal(false)]),
        );
        // Mirror `package_coherence_diagnostics`'s alias-body normalization.
        for body in aliases.values_mut() {
            *body = nf(body, &stub_enum_variants);
        }
        let mut bindings = TypeBindings::default();
        assert_eq!(
            unify_into(
                &Ty::TypeAlias(tf, TyAttr::default()),
                &Ty::bool(),
                &[],
                &aliases,
                &mut bindings,
            ),
            Overlap::Yes,
        );
    }

    #[test]
    fn var_bearing_function_arg_unifies_not_disjoint() {
        // 4.2: `(T) -> int` vs `(int) -> int` unifies at `T = int` — before the Function
        // arm this fell to the disjoint fallback, so coherence admitted two impls that
        // dispatch would both match.
        fn func(param: Ty) -> Ty {
            Ty::Function {
                params: vec![crate::ty::FunctionParamTy::required(None, param)],
                ret: Box::new(Ty::int()),
                throws: Box::new(never()),
                attr: TyAttr::default(),
            }
        }
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        assert_eq!(
            unify_into(
                &func(Ty::type_var("T")),
                &func(Ty::int()),
                &vars,
                &aliases,
                &mut bindings,
            ),
            Overlap::Yes,
        );
    }

    #[test]
    fn projection_conservatively_overlaps_not_disjoint() {
        // 4.2: a projection could stand for any type, so under the possible-worlds view
        // it *could* coincide with the opposing type — answer `Yes` (conservatively
        // reject as overlapping), never `No` (which would admit an overlapping pair) and
        // not `Unknown` (which is only for search-budget exhaustion).
        let proj = Ty::AssociatedTypeProjection {
            base: Box::new(Ty::type_var("T")),
            interface: Box::new(
                interface("Iter", vec![])
                    .as_interface()
                    .expect("interface() builds an existential"),
            ),
            member: Name::new("Item"),
            attr: TyAttr::default(),
        };
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        assert_eq!(
            unify_into(&proj, &Ty::int(), &[], &aliases, &mut bindings),
            Overlap::Yes,
        );
    }

    #[test]
    fn nf_folds_complete_bool() {
        assert_eq!(
            nf(
                &Ty::union(vec![bool_literal(true), bool_literal(false)]),
                &stub_enum_variants
            ),
            Ty::bool()
        );
    }

    #[test]
    fn nf_folds_complete_enum() {
        let all = Ty::union(vec![
            enum_variant("Cmp", "Less"),
            enum_variant("Cmp", "Equal"),
            enum_variant("Cmp", "More"),
        ]);
        assert_eq!(nf(&all, &stub_enum_variants), enum_ty("Cmp"));
    }

    #[test]
    fn nf_keeps_partial_enum() {
        // Two of `Cmp`'s three variants — not a complete base, so not folded to `Cmp`.
        // `nf` canonicalizes member order, so the result lists the variants sorted
        // (`Equal` before `Less`), independent of the input order.
        let partial = Ty::union(vec![
            enum_variant("Cmp", "Less"),
            enum_variant("Cmp", "Equal"),
        ]);
        let canonical = Ty::union(vec![
            enum_variant("Cmp", "Equal"),
            enum_variant("Cmp", "Less"),
        ]);
        assert_eq!(nf(&partial, &stub_enum_variants), canonical);
    }

    #[test]
    fn nf_absorbs_unknown() {
        assert_eq!(
            nf(
                &Ty::union(vec![Ty::int(), Ty::unknown()]),
                &stub_enum_variants
            ),
            Ty::unknown()
        );
    }

    #[test]
    fn nf_recurses_into_arguments() {
        let wrapped = Ty::user_class_with_args("Wrap", vec![Ty::union(vec![Ty::int(), never()])]);
        assert_eq!(
            nf(&wrapped, &stub_enum_variants),
            Ty::user_class_with_args("Wrap", vec![Ty::int()])
        );
    }

    #[test]
    fn union_with_var_conservatively_overlaps_enum() {
        // `Cmp.Less | T` opposite `Cmp`: the var could complete the enum
        // (`T = Cmp.Equal | Cmp.More`), so it is conservatively a possible overlap.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let u = Ty::union(vec![enum_variant("Cmp", "Less"), Ty::type_var("T")]);
        assert_eq!(
            unify_into(&u, &enum_ty("Cmp"), &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn ground_partial_enum_union_is_disjoint_from_enum() {
        // No bare variable to complete the enum, and the partial variant set is a strict
        // subset of `Cmp`, so the two are disjoint.
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let u = Ty::union(vec![
            enum_variant("Cmp", "Less"),
            enum_variant("Cmp", "Equal"),
        ]);
        assert_eq!(
            unify_into(&u, &enum_ty("Cmp"), &[], &aliases, &mut bindings),
            Overlap::No
        );
    }

    #[test]
    fn union_with_var_vs_single_class_overlaps_via_collapse() {
        // `C | T` opposite the single type `C`: at `T = C` idempotency collapses the
        // union to `C`, so they share the instance `C`. This is the union-vs-non-union
        // analogue of `union_overlap_collapsing_members_is_yes`; routing the non-union
        // operand through covering (as the singleton `{C}`) is what catches it.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let c = || Ty::class("C");
        let u = Ty::union(vec![c(), Ty::type_var("T")]);
        assert_eq!(
            unify_into(&u, &c(), &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_with_literal_and_var_vs_base_overlaps_via_collapse() {
        // `1 | T` opposite `int`: at `T = int` the literal `1 <: int` collapses, so the
        // union equals `int` — decided precisely by `cover`'s `literal <: base` oracle.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let u = Ty::union(vec![int_literal(1), Ty::type_var("T")]);
        assert_eq!(
            unify_into(&u, &Ty::int(), &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_with_var_vs_unrelated_single_is_disjoint() {
        // `D | T` opposite `C` (`C ≠ D`): the union always contains `D`, which no
        // instantiation of `T` removes, so it can never equal `C`. Routing through
        // covering keeps this precise (`No`), not a conservative over-reject.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let u = Ty::union(vec![Ty::class("D"), Ty::type_var("T")]);
        let c = Ty::class("C");
        assert_eq!(
            unify_into(&u, &c, &vars, &aliases, &mut bindings),
            Overlap::No
        );
    }

    #[test]
    fn variable_binds_to_builtin_unknown() {
        // `unknown` is the inhabited top type, so a unification variable binds to it: a
        // blanket `Box<T>` overlaps `Box<unknown>` at `T = unknown`. The old bail that
        // lumped `BuiltinUnknown` in with the error sentinel wrongly rejected this.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        assert_eq!(
            unify_into(
                &Ty::type_var("T"),
                &Ty::unknown(),
                &vars,
                &aliases,
                &mut bindings
            ),
            Overlap::Yes
        );
    }

    #[test]
    fn builtin_unknown_is_disjoint_from_distinct_concrete() {
        // `unknown` is a distinct atomic type under invariance, compared by equality:
        // `Box<unknown>` and `Box<int>` do not overlap, matching how the runtime
        // resolver matches `unknown` (only an `unknown` value inhabits `Box<unknown>`).
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        assert_eq!(
            unify_into(&Ty::unknown(), &Ty::int(), &[], &aliases, &mut bindings),
            Overlap::No
        );
    }

    // Probe helper for the pigeonhole experiments below.
    fn pigeonhole_overlap(holes: usize, pigeons: usize) -> Overlap {
        let vars: Vec<Name> = (0..holes).map(|i| Name::new(format!("T{i}"))).collect();
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let pair = |a: Ty, b: Ty| Ty::user_class_with_args("Pair", vec![a, b]);
        let xs: Vec<Ty> = (0..holes)
            .map(|i| {
                let t = Ty::type_var(&format!("T{i}"));
                pair(t.clone(), t)
            })
            .collect();
        let ys: Vec<Ty> = (0..pigeons)
            .map(|i| {
                let a = Ty::class(&format!("A{i}"));
                pair(a.clone(), a)
            })
            .collect();
        unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings)
    }

    #[test]
    fn union_overlap_small_pigeonhole_is_decided() {
        // 4 distinct variables (holes) cannot realize 5 distinct ground members
        // (pigeons), so the unions are disjoint — and at this size the search proves it
        // within budget, returning a definite `No` (contrast with the next test).
        assert_eq!(pigeonhole_overlap(4, 5), Overlap::No);
    }

    #[test]
    fn union_overlap_pigeonhole_is_unknown() {
        // The NP-hard core in miniature: 5 distinct variables (holes) cannot realize 6
        // distinct ground members (pigeons), so the unions are *provably disjoint* — but
        // ruling out every one of the ~5! arrangements overruns the step budget, so the
        // solver conservatively returns `Unknown` ("simplify your type"). An 11-member
        // union that is exponential to decide — the search's *depth*, not breadth.
        assert_eq!(pigeonhole_overlap(5, 6), Overlap::Unknown);
    }

    /// 3-SAT → union ACI-matching (adapted from the ACI paper's Lemma 5). Each SAT
    /// variable `i` is a type var `Vi` pinned to `Pos`/`Neg`; clause `j` over vars
    /// `(p,q,r)` with polarities is the member `Cl<Cj, Vp, Vq, Vr>`, whose candidates are
    /// the (≤7) ground `Pos`/`Neg` combinations that satisfy it; a bare var absorbs the
    /// candidate pool. The unions overlap iff the formula is satisfiable.
    /// `clauses[j] = [(var, positive?); 3]`.
    fn three_sat_overlap(num_vars: usize, clauses: &[[(usize, bool); 3]]) -> Overlap {
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let v = |i: usize| Ty::type_var(&format!("V{i}"));
        let val = |b: bool| Ty::class(if b { "Pos" } else { "Neg" });
        let mut xs: Vec<Ty> = Vec::new();
        let mut ys: Vec<Ty> = Vec::new();
        for (j, clause) in clauses.iter().enumerate() {
            let tag = Ty::class(&format!("C{j}"));
            xs.push(Ty::user_class_with_args(
                "Cl",
                vec![tag.clone(), v(clause[0].0), v(clause[1].0), v(clause[2].0)],
            ));
            for sp in [true, false] {
                for sq in [true, false] {
                    for sr in [true, false] {
                        let satisfied =
                            (sp == clause[0].1) || (sq == clause[1].1) || (sr == clause[2].1);
                        if satisfied {
                            ys.push(Ty::user_class_with_args(
                                "Cl",
                                vec![tag.clone(), val(sp), val(sq), val(sr)],
                            ));
                        }
                    }
                }
            }
        }
        xs.push(Ty::type_var("ABSORB"));
        let mut vars: Vec<Name> = (0..num_vars).map(|i| Name::new(format!("V{i}"))).collect();
        vars.push(Name::new("ABSORB"));
        unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings)
    }

    // All 2^n exclusion clauses over the first `n` vars ⇒ unsatisfiable.
    fn unsat_exclusion_clauses(n: usize) -> Vec<[(usize, bool); 3]> {
        assert!(n >= 3);
        let mut clauses = Vec::new();
        for mask in 0..(1usize << n) {
            // Exclude assignment `mask`; use the first 3 vars' bits as the 3 literals.
            let lit = |i: usize| (i, (mask >> i) & 1 == 0);
            clauses.push([lit(0), lit(1), lit(2)]);
        }
        clauses
    }

    #[test]
    fn union_overlap_three_sat_satisfiable_is_yes() {
        // 3-SAT reduces to coherence (the ACI-unification paper's Lemma 5, adapted to
        // unions): two `implement` blocks overlap iff the encoded formula is satisfiable.
        // `(x ∨ y ∨ z)` is satisfiable, so the blocks overlap (`Yes`) — the witness is
        // any non-all-false assignment.
        assert_eq!(
            three_sat_overlap(3, &[[(0, true), (1, true), (2, true)]]),
            Overlap::Yes,
        );
    }

    #[test]
    fn union_overlap_three_sat_unsatisfiable_is_no() {
        // The same reduction on an unsatisfiable formula (all 8 assignments of x,y,z
        // excluded) ⇒ the blocks are disjoint (`No`). At three shared variables the
        // search decides it within budget; larger spread instances are what make the
        // problem exponential.
        let clauses = unsat_exclusion_clauses(3);
        assert_eq!(three_sat_overlap(3, &clauses), Overlap::No);
    }
}
