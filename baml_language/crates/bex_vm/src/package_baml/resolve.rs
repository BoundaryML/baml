//! Runtime interface-method resolver — rustc-style trait selection over each
//! package's baked impl-rule tables (`Package::impl_rules`, via `vm.packages`).
//!
//! Given a value's concrete runtime type plus an interface and method name, it
//! returns the applicable impl's method — a concrete callee and the impl's bound
//! type args (de Bruijn order, to seed the callee's frame) — or `None` when no
//! impl applies (the caller decides the fallback).
//!
//! This mirrors the compiler's selection (`match_ty_pattern` + bound validation
//! in `baml_compiler2_tir::interfaces`), run on `baml_type::RuntimeTy`: unify the rule's
//! `for_ty_pattern` against the concrete type (binding the impl's generic
//! params), then discharge each param's declared bound as a nested obligation.

use std::borrow::Cow;

use baml_type::{
    Literal, MediaKind, Name, RuntimeInterface, RuntimeTy, TyAttr, TyTemplate, TypeName,
};
use bex_vm_types::{
    HeapPtr,
    types::{Object, Package, RuntimeImplRule},
};

use crate::BexVm;

/// Dereference a package pointer to its [`Package`]. The runtime `vm.packages`
/// index only ever holds `Object::Package` pointers.
fn deref_package(vm: &BexVm, ptr: HeapPtr) -> &Package {
    vm.get_object(ptr)
        .as_package()
        .unwrap_or_else(|| unreachable!("vm.packages pointer is not an Object::Package"))
}

/// Append every impl rule of the interface at `iface_ptr` declared in `package`
/// to `out`. `impl_rules` is keyed by the interface's canonical `Object::Interface`
/// pointer, so this is an O(1) lookup.
fn collect_package_rules<'a>(
    vm: &'a BexVm,
    package: &'a Package,
    iface_ptr: HeapPtr,
    out: &mut Vec<&'a RuntimeImplRule>,
) {
    let Some(rule_ptrs) = package.impl_rules.get(&iface_ptr) else {
        return;
    };
    for &rule_ptr in rule_ptrs {
        if let Some(rule) = vm.get_object(rule_ptr).as_impl_rule() {
            out.push(rule);
        }
    }
}

/// Overflow backstop for the obligation stack. Cycle detection (in [`prove`])
/// already rejects goals that *repeat*; this guards the other non-terminating
/// shape — goals that *grow* without ever repeating (`T: I` ⇒ `Container<T>: I`
/// ⇒ `Container<Container<T>>: I` ⇒ …), which a cycle check cannot see. rustc
/// keeps a fixed `recursion_limit` for exactly this reason. Realistic chains are
/// 1–3 deep (each normal step shrinks the type), so only pathological bounds ever
/// reach this.
const MAX_OBLIGATION_DEPTH: usize = 128;

/// An in-progress membership goal — does `RuntimeTy` implement the interface `TypeName`
/// at these args / associated bindings? Tracked on a stack so a goal that
/// recurses back to itself (an inductive cycle, with no concrete-impl base case)
/// is detected and rejected rather than spun on until the depth backstop.
type Obligation = (RuntimeTy, TypeName, Vec<RuntimeTy>, Vec<(Name, RuntimeTy)>);

/// The package that owns `ty`, if any. Primitives/containers have none — their
/// impls live in the interface's package (orphan rule).
fn type_package(ty: &RuntimeTy) -> Option<&Name> {
    match ty {
        RuntimeTy::Class(tn, ..) | RuntimeTy::Enum(tn, ..) | RuntimeTy::Interface(tn, ..) => {
            Some(tn.package())
        }
        _ => None,
    }
}

/// A literal or enum-variant type behaves as its underlying concrete type for
/// impl resolution: `1` uses `int`'s impls and `Color.Red` uses `Color`'s (a
/// literal "uses its concrete type's methods"). Normalize the *top-level* type
/// to that base before consulting the registry, which is keyed by concrete
/// types; every other type passes through untouched. Only the top level is
/// normalized — nested type args keep their literal form so invariance holds
/// (`Box<1>` is not `Box<int>`).
fn concrete_base(ty: &RuntimeTy) -> Cow<'_, RuntimeTy> {
    match ty {
        // Persist the type's attr onto the base (consistent with the enum-variant
        // arm below), so a literal carrying a non-default attr normalizes to its
        // base with that attr intact rather than silently dropping it.
        RuntimeTy::Literal(lit, _, attr) => Cow::Owned(match lit {
            Literal::Int(_) => RuntimeTy::Int { attr: attr.clone() },
            Literal::Bigint(_) => RuntimeTy::Bigint { attr: attr.clone() },
            Literal::Float(_) => RuntimeTy::Float { attr: attr.clone() },
            Literal::String(_) => RuntimeTy::String { attr: attr.clone() },
            Literal::Bool(_) => RuntimeTy::Bool { attr: attr.clone() },
        }),
        RuntimeTy::EnumVariant(name, _, attr) => {
            Cow::Owned(RuntimeTy::Enum(name.clone(), attr.clone()))
        }
        _ => Cow::Borrowed(ty),
    }
}

/// Candidate rules for `<concrete_ty as iface>`. By the orphan rule an
/// `implement iface for concrete_ty` lives in `concrete_ty`'s package or
/// `iface`'s package (a blanket impl lives in `iface`'s package even when it
/// matches a type owned elsewhere), so gather `iface`'s rules from both — deduped
/// when they are the same package. A newly-loaded package only adds entries, so
/// it never changes an existing pair's answer.
fn candidate_rules<'a>(
    vm: &'a BexVm,
    concrete_ty: &RuntimeTy,
    iface: &TypeName,
) -> Vec<&'a RuntimeImplRule> {
    let base = concrete_base(concrete_ty);
    let concrete_ty = &*base;
    // Resolve the interface's canonical object pointer once; every package's
    // `impl_rules` is keyed by it, turning rule collection into an O(1) lookup.
    // An unknown interface has no impls anywhere.
    let Some(iface_ptr) = vm.lookup_interface(iface) else {
        return Vec::new();
    };
    let mut pkgs: Vec<&Name> = Vec::with_capacity(2);
    if let Some(p) = type_package(concrete_ty) {
        pkgs.push(p);
    }
    let iface_pkg = iface.package();
    if !pkgs.contains(&iface_pkg) {
        pkgs.push(iface_pkg);
    }
    let mut out = Vec::new();
    for pkg in pkgs {
        let Some(&pkg_ptr) = vm.packages.get(pkg) else {
            continue;
        };
        collect_package_rules(vm, deref_package(vm, pkg_ptr), iface_ptr, &mut out);
    }
    out
}

/// Resolve `(Self, Iface<Args>)` to the single applicable `implements` rule, plus
/// the impl's bound type args — its generics realized by matching `concrete_ty`
/// against the rule's `for` pattern. That rule is the canonical handle: read the
/// concrete method off `rule.methods` (defaults are merged in at bake time,
/// overrides winning), the associated bindings off `rule.interface_assoc`, etc.;
/// the returned type args are the realization env for them.
///
/// Selection is keyed on `Self` (`concrete_ty`, including its own type args) and
/// the interface's **input** args only. Associated types are *outputs*
/// (functionally determined by the impl), so they never affect which impl is
/// selected — coherence is per `(Self, Iface<Args>)`. A caller holding an
/// *expected* associated projection can validate it against the resolved rule via
/// [`type_implements`]; that is checking, not selection. An empty `iface_args`
/// request matches any instantiation — the case for non-generic interfaces
/// (`Equals`/`Compare`), which have exactly one rule per type.
///
/// `None` when no impl of `iface` applies to `concrete_ty`. When several rules
/// apply — a type implementing one generic interface at distinct instantiations
/// (`Converter<int>` + `Converter<float>`) — the one whose args match the request
/// wins; the fallback to the first applicable rule serves the empty-request /
/// single-impl cases and is defensive for the (compile-time-coherence-forbidden)
/// no-match case.
pub(crate) fn resolve_implements_rule<'a>(
    vm: &'a BexVm,
    concrete_ty: &RuntimeTy,
    iface: &TypeName,
    iface_args: &[RuntimeTy],
) -> Option<(&'a RuntimeImplRule, Vec<RuntimeTy>)> {
    let mut fallback: Option<(&'a RuntimeImplRule, Vec<RuntimeTy>)> = None;
    for rule in candidate_rules(vm, concrete_ty, iface) {
        let Some(type_args) = rule_applies(vm, rule, concrete_ty, &mut Vec::new()) else {
            continue;
        };
        // Select on the interface's input args only (associated types are outputs).
        let rule_args: Vec<RuntimeTy> = rule
            .interface_args
            .iter()
            .map(|t| substitute_checked(t, &type_args))
            .collect();
        if iface_args.is_empty() || ty_args_equivalent(&rule_args, iface_args) {
            return Some((rule, type_args));
        }
        fallback.get_or_insert((rule, type_args));
    }
    fallback
}

/// Realize a [`MethodImpl`](bex_vm_types::types::MethodImpl) frame template (De
/// Bruijn over the impl's generic params) against the impl's bound type args
/// (from [`resolve_implements_rule`]). The result is the `frame.type_args` to
/// seed the resolved callee with: the impl's own generics for an impl method, or
/// the interface's args + associated types for an inherited default.
pub(crate) fn realize_frame(template: &[TyTemplate], bound_args: &[RuntimeTy]) -> Vec<RuntimeTy> {
    template
        .iter()
        .map(|t| substitute_checked(t, bound_args))
        .collect()
}

/// Whether `concrete_ty` implements `iface` at the requested args / associated
/// bindings — an empty request matches any instantiation for that dimension.
/// The runtime twin of proving a `T: Iface<Args, Assoc = …>` obligation: select
/// an impl (match its for-type, discharge its own bounds), then confirm the
/// impl's interface args/assoc — concretised with its bindings — equal the
/// request. Used both by reflection's membership queries and to discharge a
/// bounded impl's nested obligations.
pub(crate) fn type_implements(
    vm: &BexVm,
    concrete_ty: &RuntimeTy,
    iface: &TypeName,
    requested_args: &[RuntimeTy],
    requested_assoc: &[(Name, RuntimeTy)],
) -> bool {
    prove(
        vm,
        concrete_ty,
        iface,
        requested_args,
        requested_assoc,
        &mut Vec::new(),
    )
}

/// Discharge one membership goal against the in-progress obligation `stack`.
/// A goal already on the stack is an inductive cycle — a BAML interface is only
/// satisfied by a concrete impl, so a goal that depends on itself has no base
/// case and is unprovable. An over-budget stack (a goal that grows without
/// repeating) is likewise rejected.
fn prove(
    vm: &BexVm,
    concrete_ty: &RuntimeTy,
    iface: &TypeName,
    requested_args: &[RuntimeTy],
    requested_assoc: &[(Name, RuntimeTy)],
    stack: &mut Vec<Obligation>,
) -> bool {
    // Key on the normalized (literal/enum-variant → base) type so `1` and `int`
    // are the same goal for cycle purposes.
    let goal: Obligation = (
        concrete_base(concrete_ty).into_owned(),
        iface.clone(),
        requested_args.to_vec(),
        requested_assoc.to_vec(),
    );
    if stack.contains(&goal) || stack.len() >= MAX_OBLIGATION_DEPTH {
        return false;
    }
    stack.push(goal);
    let proven = candidate_rules(vm, concrete_ty, iface)
        .into_iter()
        .any(|rule| {
            rule_applies(vm, rule, concrete_ty, stack).is_some_and(|bindings| {
                interface_request_matches(rule, &bindings, requested_args, requested_assoc)
            })
        });
    stack.pop();
    proven
}

/// Match a rule's `for_ty_pattern` against `concrete_ty`, then discharge its
/// bounds. On success returns the bound generic args in de Bruijn order.
fn rule_applies(
    vm: &BexVm,
    rule: &RuntimeImplRule,
    concrete_ty: &RuntimeTy,
    stack: &mut Vec<Obligation>,
) -> Option<Vec<RuntimeTy>> {
    let base = concrete_base(concrete_ty);
    let concrete_ty = &*base;
    let mut bindings: Vec<Option<RuntimeTy>> = vec![None; rule.generic_param_bounds.len()];
    if !match_template(&rule.for_ty_pattern, concrete_ty, &mut bindings) {
        return None;
    }
    // The for-type pattern must constrain every generic param — a param the
    // pattern never mentions could not be inferred from the receiver.
    let type_args: Vec<RuntimeTy> = bindings.into_iter().collect::<Option<_>>()?;

    // Bounds as nested obligations (rustc winnowing): every interface in a param's
    // bound set must be implemented by that param's bound type arg, at the bound's
    // substituted args/assoc. `prove` guards against cycles and runaway depth.
    for (param, bounds) in rule.generic_param_bounds.iter().enumerate() {
        for bound in bounds {
            // The bound's args/assoc may reference the impl's params (`T extends
            // Container<U>`), so substitute them with the bindings, then require
            // the arg to implement the interface at that exact instantiation.
            let req_args: Vec<RuntimeTy> = bound
                .args
                .iter()
                .map(|t| substitute_checked(t, &type_args))
                .collect();
            let req_assoc: Vec<(Name, RuntimeTy)> = bound
                .assoc
                .iter()
                .map(|(n, t)| (n.clone(), substitute_checked(t, &type_args)))
                .collect();
            // An interface-existential type arg satisfies a bound that names the
            // same interface directly: the type checker only forms `Iterable<Item
            // = int>` for a value that already implements `Iterable`, and there is
            // no concrete impl rule to find for an interface *type*. This is the
            // only way `T extends Iterable` is discharged when `T` is realized to
            // an interface-existential (e.g. `Flatten<I extends Iterable>`
            // instantiated with `I = Iterable<int>`). It is bound-discharge only,
            // *not* general membership — an interface type does not *implement* the
            // interface (reflection's "does X implement I"), so it stays out of
            // `prove`/`type_implements`. (Super-interface bounds — an `Iterator`
            // existential under an `Iterable` bound — would additionally consult
            // the interface's `requires` closure; not handled here.)
            if interface_existential_satisfies_bound(
                &type_args[param],
                &bound.interface,
                &req_args,
                &req_assoc,
            ) {
                continue;
            }
            if !prove(
                vm,
                &type_args[param],
                &bound.interface,
                &req_args,
                &req_assoc,
                stack,
            ) {
                return None;
            }
        }
    }
    Some(type_args)
}

/// Whether `concrete_ty` is an interface-existential that discharges a bound
/// naming `iface` directly (same interface, equivalent args, matching assoc).
///
/// This is *bound discharge*, not membership: an interface type does not
/// *implement* an interface, so this must not be folded into `prove` /
/// `type_implements` (reflection relies on `Iterable` not implementing itself).
fn interface_existential_satisfies_bound(
    concrete_ty: &RuntimeTy,
    iface: &TypeName,
    requested_args: &[RuntimeTy],
    requested_assoc: &[(Name, RuntimeTy)],
) -> bool {
    let base = concrete_base(concrete_ty);
    let RuntimeTy::Interface(ex_qtn, ex_args, ex_assoc, _) = base.as_ref() else {
        return false;
    };
    ex_qtn == iface
        && (requested_args.is_empty() || ty_args_equivalent(ex_args, requested_args))
        && associated_bindings_equivalent(ex_assoc, requested_assoc)
}

/// Unify a `TyTemplate` pattern against a concrete `RuntimeTy`, binding each
/// `TypeArgRef(n)` into `bindings[n]`. A repeated param must bind consistently.
fn match_template(
    pattern: &TyTemplate,
    concrete: &RuntimeTy,
    bindings: &mut [Option<RuntimeTy>],
) -> bool {
    match pattern {
        TyTemplate::Wildcard => true,
        // `TypeArgRefOrWildcard` is a de Bruijn ref like `TypeArgRef` (substitution
        // treats them identically); bind it the same way here.
        #[expect(deprecated)]
        TyTemplate::TypeArgRef(n) | TyTemplate::TypeArgRefOrWildcard(n) => {
            match bindings.get_mut(*n as usize) {
                Some(slot @ None) => {
                    *slot = Some(concrete.clone());
                    true
                }
                // A repeated param must bind consistently; compare semantically so a
                // union binds the same regardless of member order.
                Some(Some(bound)) => ty_equivalent(bound, concrete),
                None => false,
            }
        }
        // Compare semantically rather than via derived `==`: union members are
        // order-insensitive (`int | string` ≡ `string | int`), matching the type
        // checker. `ty_equivalent` falls back to structural `==` (incl. `TyAttr`)
        // for non-union leaves; for-type patterns and the runtime types we match
        // carry default attrs (stream artifacts are keyed under separate `$stream`
        // names).
        TyTemplate::Concrete(t) => ty_equivalent(t, concrete),
        TyTemplate::Array(inner) => match concrete {
            RuntimeTy::List(elem, _) => match_template(inner, elem, bindings),
            _ => false,
        },
        TyTemplate::Map(k, v) => match concrete {
            RuntimeTy::Map { key, value, .. } => {
                match_template(k, key, bindings) && match_template(v, value, bindings)
            }
            _ => false,
        },
        TyTemplate::Class(name, args) => match concrete {
            RuntimeTy::Class(cname, cargs, _) => name == cname && all_match(args, cargs, bindings),
            _ => false,
        },
        TyTemplate::Interface(name, args, assoc) => match concrete {
            RuntimeTy::Interface(cname, cargs, cassoc, _) => {
                // Each *concrete* binding must match a same-named pattern binding, found
                // order-insensitively; extra pattern bindings don't constrain. This
                // direction mirrors the compiler's selection matcher
                // (`match_ty_pattern_into`'s `Interface` arm, which iterates the concrete
                // bindings and requires each in the pattern) so runtime dispatch never
                // selects an impl compile-time selection would reject. A positional,
                // length-locked `zip` would instead diverge if the two declaration orders
                // differed. (A top-level interface for-type is rejected by
                // `is_valid_impl_subject`; this is only reached for a nested interface
                // argument, where the binding sets coincide in well-formed code.)
                name == cname
                    && all_match(args, cargs, bindings)
                    && cassoc.iter().all(|(cn, ct)| {
                        assoc
                            .iter()
                            .find(|(an, _)| an == cn)
                            .is_some_and(|(_, at)| match_template(at, ct, bindings))
                    })
            }
            _ => false,
        },
        // Order-insensitive union match: each pattern member must pair with a
        // distinct concrete member, with type-var bindings consistent across the
        // chosen pairing (so `Box<T | int>` matches a value `Box<int | string>`).
        TyTemplate::Union(parts) => match concrete {
            RuntimeTy::Union(cparts, _) => match_union(parts, cparts, bindings),
            _ => false,
        },
    }
}

/// Pairwise-unify positional template args against concrete args (same arity).
fn all_match(
    patterns: &[TyTemplate],
    concretes: &[RuntimeTy],
    bindings: &mut [Option<RuntimeTy>],
) -> bool {
    patterns.len() == concretes.len()
        && patterns
            .iter()
            .zip(concretes)
            .all(|(p, c)| match_template(p, c, bindings))
}

/// Match union members order-insensitively: every pattern member must pair with a
/// *distinct* concrete member (a perfect matching), with type-var bindings
/// consistent across the pairing. Backtracks — restoring `bindings` after a failed
/// branch — because a greedy pairing can bind a type var to the wrong member
/// (e.g. `[T, int]` against `[int, string]` must pick `T = string`, not `T = int`).
fn match_union(
    patterns: &[TyTemplate],
    concretes: &[RuntimeTy],
    bindings: &mut [Option<RuntimeTy>],
) -> bool {
    if patterns.len() != concretes.len() {
        return false;
    }
    match_union_rec(
        patterns,
        concretes,
        &mut vec![false; concretes.len()],
        bindings,
    )
}

fn match_union_rec(
    patterns: &[TyTemplate],
    concretes: &[RuntimeTy],
    used: &mut [bool],
    bindings: &mut [Option<RuntimeTy>],
) -> bool {
    let Some((first, rest)) = patterns.split_first() else {
        return true;
    };
    for (i, concrete) in concretes.iter().enumerate() {
        if used[i] {
            continue;
        }
        let snapshot = bindings.to_vec();
        if match_template(first, concrete, bindings) {
            used[i] = true;
            if match_union_rec(rest, concretes, used, bindings) {
                return true;
            }
            used[i] = false;
        }
        // Restore bindings mutated by the (failed) trial before trying the next.
        bindings.clone_from_slice(&snapshot);
    }
    false
}

/// Whether a matched impl's implemented-interface instantiation satisfies the
/// request. The rule's interface args / bindings are concretised with the
/// `for_ty_pattern` bindings, then compared; an empty request matches any.
fn interface_request_matches(
    rule: &RuntimeImplRule,
    bindings: &[RuntimeTy],
    requested_args: &[RuntimeTy],
    requested_assoc: &[(Name, RuntimeTy)],
) -> bool {
    let rule_args: Vec<RuntimeTy> = rule
        .interface_args
        .iter()
        .map(|t| substitute_checked(t, bindings))
        .collect();
    let rule_assoc: Vec<(Name, RuntimeTy)> = rule
        .interface_assoc
        .iter()
        .map(|(n, t)| (n.clone(), substitute_checked(t, bindings)))
        .collect();
    (requested_args.is_empty() || ty_args_equivalent(&rule_args, requested_args))
        && associated_bindings_equivalent(&rule_assoc, requested_assoc)
}

/// Compare two generic-argument lists for *semantic* equivalence. Union
/// arguments are compared as unordered sets, so `Box<int | string>` and
/// `Box<string | int>` are the same instantiation (union member order is
/// semantically irrelevant — the type checker treats `int | string` and
/// `string | int` as identical). Falls back to structural `==` for non-union
/// leaves.
pub(super) fn ty_args_equivalent(a: &[RuntimeTy], b: &[RuntimeTy]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| ty_equivalent(x, y))
}

/// Whether two runtime types are equivalent up to union-member order at *every* nesting
/// level (`int | string` ≡ `string | int`). Union order is non-canonical at the runtime
/// boundary — neither lowering nor `dedup_and_collapse` sorts members — so any equality
/// over runtime types must be order-insensitive, and a union can hide under any
/// constructor. Every variant that carries a nested type recurses here; bare leaves
/// (primitives, enums, opaque handles, sentinels) and mismatched-constructor pairs fall
/// to structural `==`, which is exact for them.
pub(super) fn ty_equivalent(a: &RuntimeTy, b: &RuntimeTy) -> bool {
    match (a, b) {
        // Order-insensitive comparison of union members via a one-to-one matching:
        // each `a` member must pair with a *distinct* equivalent `b` member. A
        // plain `all(|x| any(|y| ...))` would wrongly accept `int | int` as
        // equivalent to `int | string` (both left members match the single `int`
        // on the right), so consume each matched member.
        (RuntimeTy::Union(am, _), RuntimeTy::Union(bm, _)) => {
            if am.len() != bm.len() {
                return false;
            }
            let mut used = vec![false; bm.len()];
            'next_a: for x in am {
                for (j, y) in bm.iter().enumerate() {
                    if !used[j] && ty_equivalent(x, y) {
                        used[j] = true;
                        continue 'next_a;
                    }
                }
                return false;
            }
            true
        }
        // Recurse into nested generic instantiations (`Box<Slot<int | string>>`).
        (RuntimeTy::Class(an, aa, _), RuntimeTy::Class(bn, ba, _)) => {
            an == bn && ty_args_equivalent(aa, ba)
        }
        (RuntimeTy::Interface(an, aa, ab, _), RuntimeTy::Interface(bn, ba, bb, _)) => {
            an == bn && ty_args_equivalent(aa, ba) && associated_bindings_exactly_equivalent(ab, bb)
        }
        // Recurse through container wrappers so a union nested inside them is still
        // compared order-insensitively (`Box<(int | string)?>` ==
        // `Box<(string | int)?>`); otherwise the wrapper would fall to the
        // structural `==` below and defeat the union-set comparison.
        (RuntimeTy::List(ai, _), RuntimeTy::List(bi, _)) => ty_equivalent(ai, bi),
        (
            RuntimeTy::Map {
                key: ak, value: av, ..
            },
            RuntimeTy::Map {
                key: bk, value: bv, ..
            },
        ) => ty_equivalent(ak, bk) && ty_equivalent(av, bv),
        // The remaining nested-type-bearing constructors, so a union buried in a future,
        // watch accessor, function signature, or associated projection is still compared
        // order-insensitively rather than falling to the order-sensitive `==` below.
        (RuntimeTy::Future(av, ae, _), RuntimeTy::Future(bv, be, _)) => {
            ty_equivalent(av, bv) && ty_equivalent(ae, be)
        }
        (RuntimeTy::WatchAccessor(ai, _), RuntimeTy::WatchAccessor(bi, _)) => ty_equivalent(ai, bi),
        (
            RuntimeTy::AssociatedTypeProjection {
                base: abase,
                interface: aiface,
                member: amember,
                ..
            },
            RuntimeTy::AssociatedTypeProjection {
                base: bbase,
                interface: biface,
                member: bmember,
                ..
            },
        ) => {
            amember == bmember
                && ty_equivalent(abase, bbase)
                && opt_iface_equivalent(aiface.as_deref(), biface.as_deref())
        }
        (
            RuntimeTy::Function {
                params: ap,
                ret: aret,
                throws: athrows,
                ..
            },
            RuntimeTy::Function {
                params: bp,
                ret: bret,
                throws: bthrows,
                ..
            },
        ) => {
            ap.len() == bp.len()
                && ap.iter().zip(bp).all(|(x, y)| {
                    x.name == y.name && x.mode == y.mode && ty_equivalent(&x.ty, &y.ty)
                })
                && ty_equivalent(aret, bret)
                && ty_equivalent(athrows, bthrows)
        }
        _ => a == b,
    }
}

/// Equivalence of two optional interface *constraints* (the `as I` annotation of
/// an associated-type projection), mirroring the `RuntimeTy::Interface` arm of
/// [`ty_equivalent`]: same name, equivalent generic arguments, and exactly
/// equivalent associated-type bindings.
fn opt_iface_equivalent(a: Option<&RuntimeInterface>, b: Option<&RuntimeInterface>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => {
            x.name == y.name
                && ty_args_equivalent(&x.generics, &y.generics)
                && associated_bindings_exactly_equivalent(&x.associated_types, &y.associated_types)
        }
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    }
}

pub(super) fn associated_bindings_equivalent(
    impl_bindings: &[(Name, RuntimeTy)],
    requested_bindings: &[(Name, RuntimeTy)],
) -> bool {
    requested_bindings.is_empty()
        || requested_bindings
            .iter()
            .all(|(requested_name, requested_ty)| {
                impl_bindings.iter().any(|(impl_name, impl_ty)| {
                    impl_name == requested_name && ty_equivalent(impl_ty, requested_ty)
                })
            })
}

fn associated_bindings_exactly_equivalent(
    a: &[(Name, RuntimeTy)],
    b: &[(Name, RuntimeTy)],
) -> bool {
    a.len() == b.len()
        && associated_bindings_equivalent(a, b)
        && associated_bindings_equivalent(b, a)
}

/// One implementor of an interface: the concrete implementor type plus the
/// interface args / associated bindings that impl pins (empty = "matches any
/// instantiation", for blanket impls and typevar-bearing dimensions).
type ImplementorEntry = (RuntimeTy, Vec<RuntimeTy>, Vec<(Name, RuntimeTy)>);

/// The concrete types that nominally implement `iface`, each paired with the
/// interface instantiation its impl fixes — the inverse of [`type_implements`].
/// Reflection (`type.implementors()`) filters these by the requested args/assoc.
///
/// By the orphan rule an impl of `iface` may live in any package, so every
/// package's table is scanned. A non-generic impl contributes its concrete
/// for-type; a generic class contributes its base (instantiations can't be
/// enumerated, so typevar-bearing interface args are erased to "match any"); a
/// blanket `for T` contributes every loaded concrete class whose bounds it
/// satisfies. Container/union for-types have no nominal implementor to list.
pub(super) fn implementor_entries(vm: &BexVm, iface: &TypeName) -> Vec<ImplementorEntry> {
    let mut out: Vec<ImplementorEntry> = Vec::new();
    // Resolve the interface's canonical object pointer once; an unknown interface
    // has no implementors. Every package's `impl_rules` is keyed by this pointer.
    let Some(iface_ptr) = vm.lookup_interface(iface) else {
        return out;
    };
    for &pkg_ptr in vm.packages.values() {
        let mut rules: Vec<&RuntimeImplRule> = Vec::new();
        collect_package_rules(vm, deref_package(vm, pkg_ptr), iface_ptr, &mut rules);
        for rule in rules {
            match &rule.for_ty_pattern {
                TyTemplate::TypeArgRef(_) => {
                    // Blanket impl: its bounds decide membership; every concrete
                    // type satisfying them is an implementor, at the interface
                    // instantiation the blanket pins (typevar dimensions erased).
                    let (args, assoc) = pinned_interface_instantiation(rule);
                    for ty in concrete_types(vm) {
                        if rule_applies(vm, rule, &ty, &mut Vec::new()).is_some() {
                            push_unique(&mut out, (ty, args.clone(), assoc.clone()));
                        }
                    }
                }
                TyTemplate::Concrete(ty) => {
                    let (args, assoc) = pinned_interface_instantiation(rule);
                    push_unique(&mut out, (ty.clone(), args, assoc));
                }
                TyTemplate::Class(name, _) => {
                    let (args, assoc) = pinned_interface_instantiation(rule);
                    let base = RuntimeTy::Class(name.clone(), Vec::new(), TyAttr::default());
                    push_unique(&mut out, (base, args, assoc));
                }
                _ => {}
            }
        }
    }
    out.sort_by_cached_key(|(ty, _, _)| ty.to_string());
    out
}

/// The interface args / associated bindings a non-blanket impl pins, with any
/// dimension that references the impl's generic params erased to empty (a
/// generic class is reported by its base, so a per-instantiation arg can't be
/// named — empty then matches any requested instantiation).
fn pinned_interface_instantiation(
    rule: &RuntimeImplRule,
) -> (Vec<RuntimeTy>, Vec<(Name, RuntimeTy)>) {
    let args = if rule.interface_args.iter().any(template_has_type_arg_ref) {
        Vec::new()
    } else {
        rule.interface_args
            .iter()
            .map(|t| substitute_checked(t, &[]))
            .collect()
    };
    let assoc = if rule
        .interface_assoc
        .iter()
        .any(|(_, t)| template_has_type_arg_ref(t))
    {
        Vec::new()
    } else {
        rule.interface_assoc
            .iter()
            .map(|(n, t)| (n.clone(), substitute_checked(t, &[])))
            .collect()
    };
    (args, assoc)
}

/// [`TyTemplate::substitute`], asserting in debug builds that every `TypeArgRef`
/// is in range. The resolver always substitutes a *complete* env (one binding per
/// impl generic param), so an out-of-range index means a malformed rule —
/// `substitute` would silently yield `unknown`, which could then make distinct
/// types compare equal (`unknown == unknown`). Release builds keep the graceful
/// fallback. (We can't assert inside `substitute` itself: stdlib sys-op paths
/// rely on its out-of-range → `unknown` behavior.)
fn substitute_checked(template: &TyTemplate, env: &[RuntimeTy]) -> RuntimeTy {
    debug_assert!(
        max_type_arg_ref(template).is_none_or(|n| (n as usize) < env.len()),
        "impl rule references a type arg out of range for an env of {}",
        env.len(),
    );
    template.substitute(env)
}

/// The largest `TypeArgRef` de Bruijn index anywhere in `t`, if any.
fn max_type_arg_ref(t: &TyTemplate) -> Option<u32> {
    match t {
        #[expect(deprecated)]
        TyTemplate::TypeArgRef(n) | TyTemplate::TypeArgRefOrWildcard(n) => Some(*n),
        TyTemplate::Concrete(_) | TyTemplate::Wildcard => None,
        TyTemplate::Array(inner) => max_type_arg_ref(inner),
        TyTemplate::Map(k, v) => max_type_arg_ref(k).max(max_type_arg_ref(v)),
        TyTemplate::Union(parts) => parts.iter().filter_map(max_type_arg_ref).max(),
        TyTemplate::Class(_, args) => args.iter().filter_map(max_type_arg_ref).max(),
        TyTemplate::Interface(_, args, assoc) => args
            .iter()
            .filter_map(max_type_arg_ref)
            .chain(assoc.iter().filter_map(|(_, t)| max_type_arg_ref(t)))
            .max(),
    }
}

/// Whether a template references any impl generic parameter (a `TypeArgRef`).
fn template_has_type_arg_ref(t: &TyTemplate) -> bool {
    match t {
        #[expect(deprecated)]
        TyTemplate::TypeArgRef(_) | TyTemplate::TypeArgRefOrWildcard(_) => true,
        TyTemplate::Concrete(_) | TyTemplate::Wildcard => false,
        TyTemplate::Array(inner) => template_has_type_arg_ref(inner),
        TyTemplate::Map(k, v) => template_has_type_arg_ref(k) || template_has_type_arg_ref(v),
        TyTemplate::Union(parts) => parts.iter().any(template_has_type_arg_ref),
        TyTemplate::Class(_, args) => args.iter().any(template_has_type_arg_ref),
        TyTemplate::Interface(_, args, assoc) => {
            args.iter().any(template_has_type_arg_ref)
                || assoc.iter().any(|(_, t)| template_has_type_arg_ref(t))
        }
    }
}

/// Every loaded concrete type — classes, enums, and the primitives — as a `RuntimeTy` at
/// its base (no type args). The candidate set a blanket impl's bounds are checked
/// against: a blanket `for T` can be satisfied by any concrete type, not just
/// classes. (`all_class_and_enum_ptrs` covers both classes and enums across every
/// loaded package.) `$stream` companions are filtered later by
/// `push_unique`.
fn concrete_types(vm: &BexVm) -> Vec<RuntimeTy> {
    let mut types: Vec<RuntimeTy> = vm
        .all_class_and_enum_ptrs()
        .filter_map(|ptr| match vm.get_object(ptr) {
            Object::Class(class) => Some(RuntimeTy::Class(
                class.name.clone(),
                Vec::new(),
                TyAttr::default(),
            )),
            Object::Enum(enum_def) => {
                Some(RuntimeTy::Enum(enum_def.name.clone(), TyAttr::default()))
            }
            _ => None,
        })
        .collect();
    // Primitives and the other arg-less builtin value types have no owning object, so
    // they aren't in any package's class/enum tables; a blanket `for T` (or one whose bound they
    // satisfy) admits them, so add the fixed set. This mirrors `Ty::is_valid_impl_subject`'s
    // arg-less accepted variants — keeping reflection's blanket-implementor enumeration
    // from silently dropping, e.g., `uint8array` (which has a builtin `Equals` impl).
    // Parameterized subjects (`List`/`Map`) and non-value types are intentionally omitted:
    // their instantiations can't be enumerated.
    types.extend([
        RuntimeTy::Int {
            attr: TyAttr::default(),
        },
        RuntimeTy::Bigint {
            attr: TyAttr::default(),
        },
        RuntimeTy::Float {
            attr: TyAttr::default(),
        },
        RuntimeTy::String {
            attr: TyAttr::default(),
        },
        RuntimeTy::Bool {
            attr: TyAttr::default(),
        },
        RuntimeTy::Null {
            attr: TyAttr::default(),
        },
        RuntimeTy::Uint8Array {
            attr: TyAttr::default(),
        },
        RuntimeTy::Type {
            attr: TyAttr::default(),
        },
        RuntimeTy::Resource {
            attr: TyAttr::default(),
        },
        RuntimeTy::PromptAst {
            attr: TyAttr::default(),
        },
    ]);
    // `Media` carries a kind, so a media value's concrete type pins one — enumerate each.
    types.extend(
        [
            MediaKind::Image,
            MediaKind::Audio,
            MediaKind::Video,
            MediaKind::Pdf,
            MediaKind::Generic,
        ]
        .map(|kind| RuntimeTy::Media(kind, TyAttr::default())),
    );
    types
}

fn push_unique(out: &mut Vec<ImplementorEntry>, entry: ImplementorEntry) {
    // Hide `…$stream` companion types: they implement their base's interfaces so
    // dispatch works on stream values, but they are compiler-synthesized and must
    // not surface in reflection's implementor lists.
    if is_stream_companion(&entry.0) || out.contains(&entry) {
        return;
    }
    out.push(entry);
}

/// Whether `ty` is an internal `…$stream` companion type.
fn is_stream_companion(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::Class(tn, ..) | RuntimeTy::Enum(tn, ..) => tn.name().ends_with("$stream"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use baml_type::{Freshness, Literal, TyAttr};

    use super::*;

    // Pins matcher invariance: a `Concrete` literal pattern matches only that exact
    // literal type, never its base primitive. The compiler applies `1 <: int` only at
    // the *top* of an overlap check (`cover`); under a constructor (`Box<1>` vs
    // `Box<int>`) both sides stay invariant, and this runtime matcher agrees — so a
    // `Box<1>` value does not dispatch an `implement … for Box<int>`. (Reached via
    // reflection today; this pins the behavior before the `==` dispatch wires the
    // resolver, where the literal-/attr-sensitivity would otherwise be untested.)
    #[test]
    fn concrete_literal_pattern_matches_only_the_literal() {
        let one = RuntimeTy::Literal(Literal::Int(1), Freshness::Regular, TyAttr::default());
        let pattern = TyTemplate::Concrete(one.clone());
        let mut binds: Vec<Option<RuntimeTy>> = Vec::new();
        assert!(match_template(&pattern, &one, &mut binds));
        assert!(!match_template(&pattern, &RuntimeTy::int(), &mut binds));
    }

    #[test]
    fn ty_equivalent_order_insensitive_under_non_container_constructors() {
        // A union nested inside a `Future` (a non-container constructor) must still be
        // compared as an unordered set: `Future<int | string>` and `Future<string | int>`
        // are the same type. Regression for the old `_ => a == b` fallback that compared
        // such nested unions order-sensitively (only `Class`/`List`/`Map`/`Interface`
        // recursed). Built with explicit member order so the two genuinely differ.
        let attr = || TyAttr::default();
        let never = || RuntimeTy::Never { attr: attr() };
        let str_ty = || RuntimeTy::String { attr: attr() };
        let int_string = RuntimeTy::Union(vec![RuntimeTy::int(), str_ty()], attr());
        let string_int = RuntimeTy::Union(vec![str_ty(), RuntimeTy::int()], attr());

        let a = RuntimeTy::Future(Box::new(int_string), Box::new(never()), attr());
        let b = RuntimeTy::Future(Box::new(string_int), Box::new(never()), attr());
        assert!(
            ty_equivalent(&a, &b),
            "Future<int | string> ≡ Future<string | int>"
        );

        // A genuinely different value type is still distinguished.
        let c = RuntimeTy::Future(Box::new(RuntimeTy::int()), Box::new(never()), attr());
        assert!(!ty_equivalent(&a, &c));
    }
}
