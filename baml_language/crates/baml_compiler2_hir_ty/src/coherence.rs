//! Interface coherence (I7): no two impls of one interface may overlap,
//! and foreign-interface impls must satisfy the orphan rule - rustc's
//! coherence + RFC-2451 knowability, lifted from TIR's
//! `interfaces/coherence.rs` + `unify.rs` overlap engine (reference,
//! never a dependency) into this crate's substrate.
//!
//! The overlap engine is symmetric first-order EQUALITY unification with
//! both impls' generic params renamed to disjoint unification variables
//! (rustc's overlap check instantiates both impls with fresh inference
//! vars the same way), three-valued: `Yes` (common instance proven),
//! `No` (provably disjoint), `Unknown` (ACI-unification is NP-hard once
//! unions carry variables; the bounded covering search gives up rather
//! than guessing). Unions compare as ACI sets via a budgeted covering
//! search (MRV + backtracking); a bound a pinned param provably violates
//! makes a pair disjoint (checked through the I1 registry at realized
//! witnesses only - a wrong negative would admit an overlapping pair).
//!
//! Everything here speaks PLAIN `baml_type::Ty`: coherence is a
//! declaration-level judgment over impl headers, the same boundary the
//! fact oracle uses. Results are keyed by `ImplLoc` (span-free,
//! salsa-stable); S17 maps them to source ranges at render time - unlike
//! TIR, whose span-carrying query invalidates on whitespace edits.
//!
//! Deliberate carry-overs from TIR, documented there and preserved:
//! `cover`'s registry-blind conservatism for concrete-vs-interface and
//! cross-interface membership (a possible overlap, never a wrong `No`;
//! registry-precise refinement is future work), the E0138 subject gate
//! applied to alias-expanded heads, and duplicate in-body blocks
//! reported as degenerate overlaps.

use baml_compiler2_hir::{
    contributions::Definition,
    loc::ImplLoc,
    package::{PackageId, package_dependency_closure},
};
use baml_type::{
    FunctionParamTy, Interface, Literal, Name, ParamTy, RealizedTy, Ty, TyAttr, TypeName,
    interned::InterfaceRef, normalize::TypeContext,
};
use rustc_hash::FxHashMap;

use crate::impls::{ImplFacts, impl_facts, package_impl_locs};

/// The unifier's substitution: unification-variable -> bound type.
pub type TypeBindings = FxHashMap<ParamTy, Ty>;

/// Three-valued result of an overlap decision. Overlap is undecidable in
/// general (ACI-unification - NP-hard - once unions with variables are
/// involved), so the checker can report "couldn't tell" rather than
/// guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlap {
    /// A common instance provably exists - the two subjects overlap.
    Yes,
    /// No common instance can exist - provably disjoint.
    No,
    /// Undecidable within the search bounds. Callers treat it as a
    /// possible overlap (the sound direction) but report it distinctly.
    Unknown,
}

impl Overlap {
    /// Kleene conjunction: `No` dominates, then `Unknown`, then `Yes`.
    fn and(self, other: Overlap) -> Overlap {
        match (self, other) {
            (Overlap::No, _) | (_, Overlap::No) => Overlap::No,
            (Overlap::Unknown, _) | (_, Overlap::Unknown) => Overlap::Unknown,
            (Overlap::Yes, Overlap::Yes) => Overlap::Yes,
        }
    }
}

/// Budget on `cover` trials before the covering search gives up with
/// `Overlap::Unknown` ("too complex to decide - simplify").
const MAX_OVERLAP_SEARCH_STEPS: usize = 4096;

/// Recursion-depth backstop for `unify_into` and the covering search
/// (rustc's `recursion_limit` approach): two distinct recursive aliases
/// as subjects grow head-first forever without repeating a goal, so a
/// visited-set cannot catch them; the depth cap fails closed to
/// `Overlap::Unknown`.
const MAX_UNIFY_DEPTH: usize = 256;

/// Whether `ty` contains a type variable drawn from `generic_params`.
fn contains_bound_typevar(ty: &Ty, generic_params: &[ParamTy]) -> bool {
    match ty {
        Ty::TypeVar(name, _) => generic_params.contains(name),
        Ty::Class(_, args, _) | Ty::Union(args, _) => args
            .iter()
            .any(|arg| contains_bound_typevar(arg, generic_params)),
        Ty::Interface(_, args, associated_bindings, _) => {
            args.iter()
                .any(|arg| contains_bound_typevar(arg, generic_params))
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| contains_bound_typevar(ty, generic_params))
        }
        Ty::List(inner, _) => contains_bound_typevar(inner, generic_params),
        Ty::Map {
            key: k, value: v, ..
        }
        | Ty::Future(k, v, _) => {
            contains_bound_typevar(k, generic_params) || contains_bound_typevar(v, generic_params)
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|FunctionParamTy { ty, .. }| contains_bound_typevar(ty, generic_params))
                || contains_bound_typevar(ret, generic_params)
                || contains_bound_typevar(throws, generic_params)
        }
        _ => false,
    }
}

/// Whether `param` occurs anywhere *inside a union* within `ty`. A param
/// that does is bound by `cover_search` to one of several possible
/// witnesses, so its representative is non-principal - the
/// bound-refutation pass must not disprove against it.
fn var_under_union(param: &ParamTy, ty: &Ty) -> bool {
    fn occurs(param: &ParamTy, ty: &Ty, in_union: bool) -> bool {
        match ty {
            Ty::TypeVar(candidate, _) => in_union && candidate == param,
            Ty::Union(members, _) => members.iter().any(|m| occurs(param, m, true)),
            Ty::Class(_, args, _) => args.iter().any(|a| occurs(param, a, in_union)),
            Ty::Interface(_, args, assoc, _) => {
                args.iter().any(|a| occurs(param, a, in_union))
                    || assoc.iter().any(|(_, t)| occurs(param, t, in_union))
            }
            Ty::List(inner, _) => occurs(param, inner, in_union),
            Ty::Map {
                key: k, value: v, ..
            }
            | Ty::Future(k, v, _) => occurs(param, k, in_union) || occurs(param, v, in_union),
            Ty::AssociatedTypeProjection {
                base, interface, ..
            } => {
                occurs(param, base, in_union)
                    || interface.tys().any(|ty| occurs(param, ty, in_union))
            }
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                params
                    .iter()
                    .any(|FunctionParamTy { ty, .. }| occurs(param, ty, in_union))
                    || occurs(param, ret, in_union)
                    || occurs(param, throws, in_union)
            }
            _ => false,
        }
    }
    occurs(param, ty, false)
}

/// Looks up an enum's full variant-name set (`None` if unresolvable).
type EnumVariants<'a> = &'a dyn Fn(&TypeName) -> Option<Vec<Name>>;

/// Normalize toward the union canonical form the covering solver assumes:
/// flatten, drop `never`, absorb `unknown`, deduplicate, drop subsumed
/// members (`1 | int -> int`), fold complete finite bases
/// (`true | false -> bool`, all variants -> the enum), recursing into
/// every argument.
fn nf(ty: &Ty, enum_variants: EnumVariants) -> Ty {
    match ty {
        Ty::Union(members, attr) => normalize_union(
            members.iter().map(|m| nf(m, enum_variants)).collect(),
            attr.clone(),
            enum_variants,
        ),
        Ty::List(inner, attr) => Ty::List(Box::new(nf(inner, enum_variants)), attr.clone()),
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(nf(key, enum_variants)),
            value: Box::new(nf(value, enum_variants)),
            attr: attr.clone(),
        },
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

    flat = (0..flat.len())
        .filter(|&i| !(0..flat.len()).any(|j| i != j && is_literal_subtype(&flat[i], &flat[j])))
        .map(|i| flat[i].clone())
        .collect();

    fold_finite_bases(&mut flat, enum_variants);

    // Deterministic member order: overlap decisions are order-insensitive,
    // but a stable output avoids spurious salsa-cache churn.
    flat.sort();

    match flat.len() {
        0 => Ty::Never { attr },
        1 => flat
            .pop()
            .unwrap_or_else(|| unreachable!("a length-1 vec has an element")),
        _ => Ty::Union(flat, attr),
    }
}

/// Fold complete finite bases: `true | false -> bool`; an enum all of
/// whose variants are present folds to the enum.
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

/// Type aliases visible to `pkg` - its own plus its dependency closure's -
/// bodies folded toward the union canonical form. An alias-obscured union
/// must compare by the same union laws as its spelled-out form: without
/// the fold, `Bar<TF>` vs `Bar<bool>` (with `type TF = true | false`) is
/// wrongly judged disjoint - a fails-open coherence hole.
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
                    .or_insert_with(|| crate::lower::type_alias_value(db, *loc).to_plain());
            }
        }
    }
}

/// Resolve a chain of top-level aliases via the pre-normalized map,
/// bounded against alias cycles (those are a separate diagnostic). Only
/// the head; nested aliases are handled by the equality context.
fn expand_alias_head(ty: &Ty, aliases: &FxHashMap<TypeName, Ty>) -> Ty {
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

/// The fact-poor equality context for the unifier's ground fast path:
/// aliases expand (two spellings differing only by an alias denote the
/// same type); every nominal fact is opaque. Conservative equality is
/// exactly right here - fewer coincidental equalities means fail-closed
/// coherence - and it is the termination argument: a fact-rich context
/// would re-enter impl resolution from inside the overlap check.
struct AliasEquivCtx<'a>(&'a FxHashMap<TypeName, Ty>);

impl TypeContext for AliasEquivCtx<'_> {
    /// A name-based context represents a declaration by its own name, so this
    /// is the identity — no resolution step, and never `None`.
    fn head_lookup(&self, qtn: &TypeName) -> Option<TypeName> {
        Some(qtn.clone())
    }
    fn alias_def(&self, name: &TypeName) -> Option<Ty> {
        self.0.get(name).cloned()
    }
    fn implements_interface(&self, _: &Ty, _: &Interface) -> bool {
        false
    }
    fn type_var_bound(&self, _: &ParamTy) -> Vec<Interface> {
        Vec::new()
    }
    fn interface_requires(&self, _: &Interface, _: &Interface) -> bool {
        false
    }
    fn enum_variants(&self, _: &TypeName) -> Option<Vec<Name>> {
        None
    }
    fn associated_type_bound(&self, _: &Interface, _: Name) -> Vec<Interface> {
        Vec::new()
    }
    fn project(
        &self,
        _: &Ty,
        _: &Interface,
        _: &Name,
        _: u32,
    ) -> baml_type::normalize::ProjectionStep {
        baml_type::normalize::ProjectionStep::Opaque
    }
}

/// Symmetric first-order EQUALITY unification: is there a substitution of
/// `vars` (either side may bind) making `x` and `y` the same type?
/// Commits bindings on `Yes`. Equality, not subtyping: `int` and `1` are
/// distinct here; `literal <: base` lives in `cover`, its only consumer.
fn unify_into(
    x: &Ty,
    y: &Ty,
    vars: &[ParamTy],
    aliases: &FxHashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    unify_into_at(x, y, vars, aliases, bindings, 0)
}

fn unify_into_at(
    x: &Ty,
    y: &Ty,
    vars: &[ParamTy],
    aliases: &FxHashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
    depth: usize,
) -> Overlap {
    if depth >= MAX_UNIFY_DEPTH {
        return Overlap::Unknown;
    }
    let x = chase_var(x, vars, bindings);
    let y = chase_var(y, vars, bindings);
    let x = expand_alias_head(&x, aliases);
    let y = expand_alias_head(&y, aliases);

    // Error sentinels never unify: an unresolved or errored subject
    // carries its own diagnostic; admitting it as a common instance would
    // stack a spurious overlap. The inhabited top type `unknown`
    // (BuiltinUnknown) is deliberately NOT bailed: it binds an opposing
    // variable and is otherwise a distinct atomic type under invariance.
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

    if AliasEquivCtx(aliases).equivalent(&x, &y) {
        return Overlap::Yes;
    }

    match (&x, &y) {
        (Ty::Class(xq, xa, _), Ty::Class(yq, ya, _)) if xq == yq && xa.len() == ya.len() => {
            unify_all(xa, ya, vars, aliases, bindings, depth + 1)
        }
        (Ty::Interface(xq, xa, xb, _), Ty::Interface(yq, ya, yb, _))
            if xq == yq && xa.len() == ya.len() =>
        {
            // Args AND associated bindings are part of an existential's
            // identity: coherence gives each concrete type one `impl I`,
            // hence one `Item` - `I<Item=int>` and `I<Item=string>` are
            // disjoint. (Distinct from the impl's OWN interface, whose
            // bindings are outputs and dropped by `renamed_subject`.)
            unify_all(xa, ya, vars, aliases, bindings, depth + 1).and(unify_associated_bindings(
                xb,
                yb,
                vars,
                aliases,
                bindings,
                depth + 1,
            ))
        }
        (Ty::List(xi, _), Ty::List(yi, _)) => {
            unify_into_at(xi, yi, vars, aliases, bindings, depth + 1)
        }
        (
            Ty::Map {
                key: xk, value: xv, ..
            },
            Ty::Map {
                key: yk, value: yv, ..
            },
        ) => unify_into_at(xk, yk, vars, aliases, bindings, depth + 1).and(unify_into_at(
            xv,
            yv,
            vars,
            aliases,
            bindings,
            depth + 1,
        )),
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
        // Unions compare by covering on their member sets (ACI); a
        // non-union operand routes through as the singleton union, which
        // is what decides var-bearing unions opposite single types
        // precisely (`1 | T` vs `int` at `T = int`; `C | T` vs `C` via
        // idempotency; `D | T` vs `C` disjoint).
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
        // A projection could stand for any concrete type: under the
        // possible-worlds view there IS an instantiation where it
        // coincides with the opposing type - `Yes` (conservatively reject
        // as overlapping), not `Unknown` (reserved for budget exhaustion).
        (Ty::AssociatedTypeProjection { .. }, _) | (_, Ty::AssociatedTypeProjection { .. }) => {
            Overlap::Yes
        }
        // Everything else is disjoint under equality: equal subjects
        // already unified above, unions are handled above, and any
        // remaining pair is distinct types.
        _ => Overlap::No,
    }
}

/// Position-wise conjunction over equal-length arg lists.
fn unify_all(
    xs: &[Ty],
    ys: &[Ty],
    vars: &[ParamTy],
    aliases: &FxHashMap<TypeName, Ty>,
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

/// Conjunction over associated-binding names common to both sides. A name
/// on only one side does not constrain (loosens toward `Yes`, never a
/// wrong `No`).
fn unify_associated_bindings(
    xb: &[(Name, Ty)],
    yb: &[(Name, Ty)],
    vars: &[ParamTy],
    aliases: &FxHashMap<TypeName, Ty>,
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

/// Union-vs-union overlap: ACI sets decided by COVERING, not one-to-one
/// pairing - idempotency lets several members collapse onto one, so a
/// member need only unify with SOME member of the other side. Cheap
/// special-cases first (all-ground = set equality; a bare variable on
/// each side absorbs the other = `Yes`); otherwise the covering
/// obligations - one-directional when a bare variable absorbs one side,
/// mutual when neither has one - solve jointly in `cover_search`.
fn unify_union_members_at(
    xs: &[Ty],
    ys: &[Ty],
    vars: &[ParamTy],
    aliases: &FxHashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
    depth: usize,
) -> Overlap {
    if let Some(result) = try_union_set_equality(xs, ys, vars, aliases) {
        return result;
    }
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

#[cfg(test)]
fn unify_union_members(
    xs: &[Ty],
    ys: &[Ty],
    vars: &[ParamTy],
    aliases: &FxHashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    unify_union_members_at(xs, ys, vars, aliases, bindings, 0)
}

/// With no variable on either side, overlap is exact set equality.
fn try_union_set_equality(
    xs: &[Ty],
    ys: &[Ty],
    vars: &[ParamTy],
    aliases: &FxHashMap<TypeName, Ty>,
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

/// A bare variable can instantiate to a UNION, absorbing any member set;
/// one on each side means a common instance always exists.
fn try_union_mutual_absorption(xs: &[Ty], ys: &[Ty], vars: &[ParamTy]) -> Option<Overlap> {
    let has_bare = |members: &[Ty]| members.iter().any(|m| is_bare_var(m, vars));
    (has_bare(xs) && has_bare(ys)).then_some(Overlap::Yes)
}

fn is_bare_var(m: &Ty, vars: &[ParamTy]) -> bool {
    matches!(m, Ty::TypeVar(n, _) if vars.contains(n))
}

fn unions_set_equal(xs: &[Ty], ys: &[Ty], aliases: &FxHashMap<TypeName, Ty>) -> bool {
    xs.len() == ys.len()
        && xs
            .iter()
            .all(|x| ys.iter().any(|y| AliasEquivCtx(aliases).equivalent(x, y)))
}

/// The covering oracle: can `member` be a SUBTYPE of `candidate` under
/// some substitution? `unify_into` supplies the bulk (equality implies
/// subtype); on top sit the two subtypings invariance leaves above the
/// constructor level (`literal <: base`, `variant <: enum`) and the pairs
/// undecidable without the impl registry - concrete vs interface, two
/// different interfaces, opaque `$rust_type` - treated conservatively as
/// possible overlap (`Yes`), never a wrong `No`. Registry-precise
/// membership is a marked future refinement (the registry IS live in
/// this crate; changing the answer here changes accepted programs, so it
/// lands as its own deliberate step with fixtures).
fn cover_at(
    member: &Ty,
    candidate: &Ty,
    vars: &[ParamTy],
    aliases: &FxHashMap<TypeName, Ty>,
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

#[cfg(test)]
fn cover(
    member: &Ty,
    candidate: &Ty,
    vars: &[ParamTy],
    aliases: &FxHashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    cover_at(member, candidate, vars, aliases, bindings, 0)
}

/// Directional `literal <: base` / `variant <: enum`.
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

/// Pairs whose subtyping needs the impl registry: conservative `Yes`.
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

/// Solve the covering obligations jointly: one substitution under which
/// every `(member, candidates)` obligation holds. MRV first (a single
/// viable candidate is forced; none fails fast), then backtracking.
/// `budget` caps `cover` trials; exhaustion yields `Unknown`.
fn cover_search(
    obligations: &[(Ty, Vec<Ty>)],
    vars: &[ParamTy],
    aliases: &FxHashMap<TypeName, Ty>,
    bindings: &mut TypeBindings,
    budget: &mut usize,
    depth: usize,
) -> Overlap {
    if obligations.is_empty() {
        return Overlap::Yes;
    }

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
                break;
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

/// Resolve through the current bindings to the representative.
fn chase_var(ty: &Ty, vars: &[ParamTy], bindings: &TypeBindings) -> Ty {
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

/// Bind variable `n` to already-chased `t`, unifying with any existing
/// binding; the occurs check rejects infinite types.
fn bind_unify_var(
    n: &ParamTy,
    t: &Ty,
    vars: &[ParamTy],
    aliases: &FxHashMap<TypeName, Ty>,
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

/// Occurs check (chasing bound vars): binding `n := t` with `n` inside
/// `t` would build an infinite type - no finite common instance.
fn occurs_in(n: &ParamTy, t: &Ty, vars: &[ParamTy], bindings: &TypeBindings) -> bool {
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
        Ty::List(inner, _) => occurs_in(n, inner, vars, bindings),
        Ty::Map {
            key: k, value: v, ..
        }
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
        _ => false,
    }
}

/// Substitute `bindings` into a plain type by param identity.
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

// ── The per-package coherence walk ───────────────────────────────────────

/// A coherence violation: two implementations of the same interface that
/// overlap, or could not be proven disjoint. With no specialization,
/// either is a hard error; `indeterminate` words the diagnostic.
/// Location-keyed (span-free): S17 maps to source ranges at render time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct CoherenceReport<'db>(pub Vec<CoherenceViolation<'db>>);

// SAFETY: PartialEq-driven overwrite, the ImplFacts precedent.
#[allow(unsafe_code)]
unsafe impl salsa::Update for CoherenceReport<'_> {
    #[allow(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        unsafe {
            let changed = *old_pointer != new_value;
            if changed {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            changed
        }
    }
}

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
        .filter_map(|&loc| impl_facts(db, loc).as_ref().map(|facts| (loc, facts)))
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
/// (rustc's conflicting-implementations error for exact duplicates). An
/// impl whose alias-expanded for-target is not a valid implementor (the
/// E0138 concreteness gate's subjects) contributes no overlap - it
/// carries its own rejection, and stacking a spurious overlap on top
/// would double-report.
pub fn impls_conflict(
    db: &dyn baml_compiler2_ppir::Db,
    a: &ImplFacts<'_>,
    b: &ImplFacts<'_>,
    aliases: &FxHashMap<TypeName, Ty>,
) -> Overlap {
    if a.interface.name != b.interface.name {
        return Overlap::No;
    }
    if !expand_alias_head(&a.for_ty_pattern.to_plain(), aliases).is_valid_impl_subject()
        || !expand_alias_head(&b.for_ty_pattern.to_plain(), aliases).is_valid_impl_subject()
    {
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
        interface: InterfaceRef::from_constraint(&mounted.interface),
        for_ty_pattern: baml_type::interned::Ty::from_plain(&mounted.for_ty_pattern),
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
                        .map(InterfaceRef::from_constraint)
                        .collect(),
                )
            })
            .collect(),
        associated_types: mounted
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), baml_type::interned::Ty::from_plain(ty)))
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
            let bound = plain_bound(bound).map_tys(|t| substitute_plain(t, &param_witnesses));
            if bound.tys().all(|t| RealizedTy::try_from(t).is_ok())
                && crate::impls::resolve_impl(
                    db,
                    &crate::impls::interned_ty(witness),
                    &InterfaceRef::from_constraint(&bound),
                )
                .is_none()
            {
                return false;
            }
        }
    }
    true
}

/// An interned bound target as a plain constraint.
fn plain_bound(target: &InterfaceRef) -> Interface {
    Interface::new(
        target.name.clone(),
        target
            .generics
            .iter()
            .map(baml_type::interned::Ty::to_plain)
            .collect(),
        target
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), ty.to_plain()))
            .collect(),
    )
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
        .generics
        .iter()
        .map(|arg| nf(&substitute_plain(&arg.to_plain(), &rename), enum_variants))
        .collect();
    (for_ty, args)
}

// ── The orphan rule (E0139, RFC-2451 covered) ────────────────────────────

/// An orphan-rule violation on one impl: a foreign interface implemented
/// with no local type covering the impl (or an uncovered generic param
/// appearing before the first local type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanViolation<'db> {
    pub block: ImplLoc<'db>,
    pub interface: TypeName,
    /// `Some` = the RFC-2451 uncovered-param flavor; `None` = no local
    /// type anywhere in the impl's inputs.
    pub uncovered_param: Option<Name>,
}

/// Wrapper for the manual `salsa::Update` impl.
#[derive(Debug, Clone, PartialEq)]
pub struct OrphanReport<'db>(pub Vec<OrphanViolation<'db>>);

// SAFETY: PartialEq-driven overwrite, the ImplFacts precedent.
#[allow(unsafe_code)]
unsafe impl salsa::Update for OrphanReport<'_> {
    #[allow(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        unsafe {
            let changed = *old_pointer != new_value;
            if changed {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            changed
        }
    }
}

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
        let args: Vec<Ty> = facts
            .interface
            .generics
            .iter()
            .map(baml_type::interned::Ty::to_plain)
            .collect();
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
    use baml_type::Freshness;

    use super::*;

    fn param(name: &str) -> ParamTy {
        ParamTy::new(0, Name::new(name))
    }

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
        Ty::Literal(Literal::Int(n), Freshness::Regular, TyAttr::default())
    }

    fn bool_literal(b: bool) -> Ty {
        Ty::Literal(Literal::Bool(b), Freshness::Regular, TyAttr::default())
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

    /// Stub enum schema for `nf` tests: `Cmp` has `Less`, `Equal`, `More`.
    fn stub_enum_variants(qtn: &TypeName) -> Option<Vec<Name>> {
        (qtn.name().as_str() == "Cmp")
            .then(|| vec![Name::new("Less"), Name::new("Equal"), Name::new("More")])
    }

    #[test]
    fn contains_bound_typevar_checks_interface_associated_bindings() {
        let ty = Ty::Interface(
            TypeName::local(Name::new("Source")),
            vec![],
            vec![(
                Name::new("Item"),
                Ty::List(Box::new(Ty::type_var("T")), TyAttr::default()),
            )],
            TyAttr::default(),
        );

        assert!(contains_bound_typevar(&ty, &[param("T")]));
        assert!(!contains_bound_typevar(&ty, &[param("U")]));
    }

    #[test]
    fn union_overlap_both_bare_vars_is_yes() {
        let vars = vec![param("T"), param("V")];
        let aliases = FxHashMap::default();
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
        // `{int, T}` vs `{int, string, Foo}`: `T = string | Foo` makes them
        // the same union - a provable overlap.
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
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
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
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
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
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
        let vars = vec![param("T"), param("U"), param("V")];
        let aliases = FxHashMap::default();
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
        // Idempotency lets members collapse many-to-one; an injective
        // matcher wrongly rejects this.
        let vars = vec![param("T"), param("U"), param("W")];
        let aliases = FxHashMap::default();
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
        // Unknown via candidate-set breadth: scanning the huge list
        // exhausts the step budget before the search can decide.
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
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
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
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
        // Subtyping is directional: `int` is not a subtype of `1`.
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
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
        let aliases = FxHashMap::default();
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
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
        let mut bindings = TypeBindings::default();
        let a = interface_with_assoc("I", vec![("Item", Ty::int())]);
        let b = interface_with_assoc("I", vec![("Item", Ty::type_var("T"))]);
        assert_eq!(
            unify_into(&a, &b, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
        assert_eq!(bindings.get(&param("T")), Some(&Ty::int()));
    }

    #[test]
    fn distinct_recursive_alias_subjects_terminate_not_overflow() {
        // `type R = Box<R>` and `type S = Box<S>` are equirecursively the
        // SAME type (the mu-automaton's canonical forms coincide), so the
        // pair is a proven overlap - and the walk terminates instead of
        // growing head-first forever.
        let r = TypeName::local(Name::new("R"));
        let s = TypeName::local(Name::new("S"));
        let mut aliases = FxHashMap::default();
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
            Overlap::Yes,
        );
    }

    #[test]
    fn cover_distinct_interface_binding_is_not_conservative() {
        let aliases = FxHashMap::default();
        let mut bindings = TypeBindings::default();
        let a = interface_with_assoc("I", vec![("Item", Ty::int())]);
        let b = interface_with_assoc("I", vec![("Item", Ty::string())]);
        assert_eq!(cover(&a, &b, &[], &aliases, &mut bindings), Overlap::No);
    }

    #[test]
    fn cover_class_vs_interface_is_conservative_yes() {
        let aliases = FxHashMap::default();
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
        // `type TF = true | false` must unify with `bool` - otherwise
        // `Bar<TF>` vs `Bar<bool>` is judged disjoint (fails open).
        let tf = TypeName::local(Name::new("TF"));
        let mut aliases = FxHashMap::default();
        aliases.insert(
            tf.clone(),
            Ty::union(vec![bool_literal(true), bool_literal(false)]),
        );
        // Mirror `normalized_alias_map`'s alias-body normalization.
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
        fn func(param: Ty) -> Ty {
            Ty::Function {
                params: vec![FunctionParamTy::required(None, param)],
                ret: Box::new(Ty::int()),
                throws: Box::new(never()),
                attr: TyAttr::default(),
            }
        }
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
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
        let aliases = FxHashMap::default();
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
        // `Cmp.Less | T` vs `Cmp`: `T` could complete the enum.
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
        let mut bindings = TypeBindings::default();
        let u = Ty::union(vec![enum_variant("Cmp", "Less"), Ty::type_var("T")]);
        assert_eq!(
            unify_into(&u, &enum_ty("Cmp"), &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn ground_partial_enum_union_is_disjoint_from_enum() {
        let aliases = FxHashMap::default();
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
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
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
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
        let mut bindings = TypeBindings::default();
        let u = Ty::union(vec![int_literal(1), Ty::type_var("T")]);
        assert_eq!(
            unify_into(&u, &Ty::int(), &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_with_var_vs_unrelated_single_is_disjoint() {
        // `D | T` vs `C`: the union always contains `D`, which no
        // instantiation of `T` removes.
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
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
        // `unknown` is the inhabited top type: a blanket `Box<T>` overlaps
        // `Box<unknown>` at `T = unknown`.
        let vars = vec![param("T")];
        let aliases = FxHashMap::default();
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
        let aliases = FxHashMap::default();
        let mut bindings = TypeBindings::default();
        assert_eq!(
            unify_into(&Ty::unknown(), &Ty::int(), &[], &aliases, &mut bindings),
            Overlap::No
        );
    }

    fn pigeonhole_overlap(holes: usize, pigeons: usize) -> Overlap {
        let vars: Vec<ParamTy> = (0..holes)
            .map(|i| ParamTy::new(0, Name::new(format!("T{i}"))))
            .collect();
        let aliases = FxHashMap::default();
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
        assert_eq!(pigeonhole_overlap(4, 5), Overlap::No);
    }

    #[test]
    fn union_overlap_pigeonhole_is_unknown() {
        // The NP-hard core in miniature: provably disjoint, but ruling out
        // every arrangement overruns the budget - `Unknown` ("simplify").
        assert_eq!(pigeonhole_overlap(5, 6), Overlap::Unknown);
    }

    /// 3-SAT reduces to union ACI-matching (the ACI paper's Lemma 5): the
    /// unions overlap iff the encoded formula is satisfiable.
    fn three_sat_overlap(num_vars: usize, clauses: &[[(usize, bool); 3]]) -> Overlap {
        let aliases = FxHashMap::default();
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
        let mut vars: Vec<ParamTy> = (0..num_vars)
            .map(|i| ParamTy::new(0, Name::new(format!("V{i}"))))
            .collect();
        vars.push(param("ABSORB"));
        unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings)
    }

    fn unsat_exclusion_clauses(n: usize) -> Vec<[(usize, bool); 3]> {
        assert!(n >= 3);
        let mut clauses = Vec::new();
        for mask in 0..(1usize << n) {
            let lit = |i: usize| (i, (mask >> i) & 1 == 0);
            clauses.push([lit(0), lit(1), lit(2)]);
        }
        clauses
    }

    #[test]
    fn union_overlap_three_sat_satisfiable_is_yes() {
        assert_eq!(
            three_sat_overlap(3, &[[(0, true), (1, true), (2, true)]]),
            Overlap::Yes,
        );
    }

    #[test]
    fn union_overlap_three_sat_unsatisfiable_is_no() {
        let clauses = unsat_exclusion_clauses(3);
        assert_eq!(three_sat_overlap(3, &clauses), Overlap::No);
    }

    // ── orphan_check unit tests ─────────────────────────────────────────

    fn foreign(name: &str) -> TypeName {
        TypeName::new(Name::new("dep"), Vec::new(), Name::new(name))
    }

    fn local_class(package: &str, name: &str) -> Ty {
        Ty::Class(
            TypeName::new(Name::new(package), Vec::new(), Name::new(name)),
            Vec::new(),
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
