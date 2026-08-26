//! Runtime interface-method resolver — rustc-style trait selection over the
//! program-wide baked impl-rule table
//! ([`PackageIndex`](crate::package_load::PackageIndex), via `vm.packages`).
//!
//! Given a value's concrete runtime type plus an interface and method name, it
//! returns the applicable impl's method — a concrete callee and the impl's bound
//! type args (de Bruijn order, to seed the callee's frame) — or `None` when no
//! impl applies (the caller decides the fallback).
//!
//! This mirrors the compiler's selection (`match_ty_pattern` + bound validation
//! in `baml_compiler2_hir_ty::interfaces`), run on `bex_vm_types::RealizedTy`: unify the rule's
//! `for_ty_pattern` against the concrete type (binding the impl's generic
//! params), then discharge each param's declared bound as a nested obligation.

use std::borrow::Cow;

use baml_type::{Literal, Name, normalize::TypeContext};
use bex_vm_types::{
    RealizedTy, TyTemplate, TypeHead, errors::VmInternalError, types::RuntimeImplRule,
};

use crate::{BexVm, type_context::StructuralEquivCtx};

/// A resolver candidate borrows an immutable rule from the heap. Every rule —
/// compiled into the static image, owned by a runtime package, or registered
/// as an anonymous class's witness — is an `Object::ImplRule`, so no candidate
/// ever copies rule metadata. The two variants differ only in cacheability:
/// a `Static` rule lives in the never-moving compile-time region.
pub(crate) enum RuntimeImplRuleCandidate<'vm> {
    Static(&'vm RuntimeImplRule),
    Borrowed(&'vm RuntimeImplRule),
}

impl RuntimeImplRuleCandidate<'_> {
    pub(crate) fn is_static(&self) -> bool {
        matches!(self, Self::Static(_))
    }
}

impl std::ops::Deref for RuntimeImplRuleCandidate<'_> {
    type Target = RuntimeImplRule;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Static(rule) | Self::Borrowed(rule) => rule,
        }
    }
}

/// The runtime impl resolver over a running VM. Holding the `&BexVm` here —
/// rather than threading it through every helper — keeps the whole selection
/// machinery able to reach VM facts at any depth: candidate collection reads the
/// package tables, and every leaf type comparison builds the alias-expanding
/// [`StructuralEquivCtx`], so recursive aliases fold correctly (see its docs for
/// the fact profile and why the re-entrant facts stay opaque).
#[derive(Clone, Copy)]
pub(crate) struct ImplResolver<'vm> {
    vm: &'vm BexVm,
    root_package: Option<bex_vm_types::HeapPtr>,
}

impl<'vm> ImplResolver<'vm> {
    pub(crate) fn new(vm: &'vm BexVm) -> Self {
        Self {
            vm,
            root_package: None,
        }
    }

    /// Resolve using an explicitly selected runtime package as the dynamic
    /// world root. Reflection needs this when checking a package value from
    /// code lexically owned by a different package.
    pub(crate) fn for_package(vm: &'vm BexVm, package: bex_vm_types::HeapPtr) -> Self {
        Self {
            vm,
            root_package: Some(package),
        }
    }

    /// Resolve in the dynamic world that owns `value`, falling back to the
    /// lexical frame's world for static values and primitives.
    pub(crate) fn for_value(vm: &'vm BexVm, value: bex_vm_types::Value) -> Self {
        let package = vm.value_runtime_package(value);
        if package.is_null() {
            Self::new(vm)
        } else {
            Self::for_package(vm, package)
        }
    }

    /// Every impl rule of `iface` in the program, in package-load order.
    ///
    /// The program-wide [`PackageIndex`](crate::package_load::PackageIndex) is
    /// keyed by the interface's canonical `Object::Interface` pointer, so this is
    /// one O(1) lookup over a table that already spans every package — see that
    /// type's docs for why a per-package search cannot be narrowed correctly. An
    /// unknown interface (not loaded) has no impls anywhere.
    fn rules_for(self, iface: TypeHead) -> Vec<RuntimeImplRuleCandidate<'vm>> {
        // The head *is* the canonical `Object::Interface` pointer that keys
        // every package's `impl_rules` — no name lookup on the dispatch path.
        //
        // This is also why a rooted resolver needs no viewpoint choice here. A
        // goal can name an interface the inspected package declares itself or
        // one it borrowed from a mounted dependency; resolving that *name*
        // would have to pick which of two same-named interfaces to key on. The
        // head carries the identity the goal was built from, so there is
        // nothing left to disambiguate.
        let iface_ptr = iface.ptr();
        let mut pointers = Vec::new();
        pointers.extend(self.vm.packages.impl_rules_of(iface_ptr));
        let mut packages = vec![
            self.root_package
                .unwrap_or_else(|| self.vm.current_runtime_package()),
        ];
        let mut seen = std::collections::HashSet::new();
        while let Some(package_ptr) = packages.pop() {
            if package_ptr.is_null() || !seen.insert(package_ptr) {
                continue;
            }
            let Some(package) = self.vm.get_object(package_ptr).as_package() else {
                continue;
            };
            if let Some(rules) = package.impl_rules.get(&iface_ptr) {
                pointers.extend(rules);
            }
            if let Some(runtime) = package.runtime() {
                packages.extend(runtime.dependencies.iter().copied());
            }
        }
        let mut rules = pointers
            .into_iter()
            .filter_map(|rule_ptr| {
                self.vm
                    .get_object(rule_ptr)
                    .as_impl_rule()
                    .map(RuntimeImplRuleCandidate::Borrowed)
            })
            .collect::<Vec<_>>();
        rules.extend(
            self.vm
                .dynamic_dispatch
                .rules_of(iface_ptr)
                .into_iter()
                .filter_map(|rule_ptr| {
                    self.vm
                        .get_object(rule_ptr)
                        .as_impl_rule()
                        .map(RuntimeImplRuleCandidate::Borrowed)
                }),
        );
        rules
    }
}

/// Overflow backstop for the obligation stack. Cycle detection (in
/// [`ImplResolver::prove`]) already rejects goals that *repeat*; this guards the
/// other non-terminating
/// shape — goals that *grow* without ever repeating (`T: I` ⇒ `Container<T>: I`
/// ⇒ `Container<Container<T>>: I` ⇒ …), which a cycle check cannot see. rustc
/// keeps a fixed `recursion_limit` for exactly this reason. Realistic chains are
/// 1–3 deep (each normal step shrinks the type), so only pathological bounds ever
/// reach this.
const MAX_OBLIGATION_DEPTH: usize = 128;

/// An in-progress membership goal — does `RealizedTy` implement the interface `TypeName`
/// at these args / associated bindings? Tracked on a stack so a goal that
/// recurses back to itself (an inductive cycle, with no concrete-impl base case)
/// is detected and rejected rather than spun on until the depth backstop.
type Obligation = (
    RealizedTy,
    TypeHead,
    Vec<RealizedTy>,
    Vec<(Name, RealizedTy)>,
);

/// A literal or enum-variant type behaves as its underlying concrete type for
/// impl resolution: `1` uses `int`'s impls and `Color.Red` uses `Color`'s (a
/// literal "uses its concrete type's methods"). Normalize the *top-level* type
/// to that base before consulting the registry, which is keyed by concrete
/// types; every other type passes through untouched. Only the top level is
/// normalized — nested type args keep their literal form so invariance holds
/// (`Box<1>` is not `Box<int>`).
fn concrete_base(ty: &RealizedTy) -> Cow<'_, RealizedTy> {
    match ty {
        // Persist the type's attr onto the base (consistent with the enum-variant
        // arm below), so a literal carrying a non-default attr normalizes to its
        // base with that attr intact rather than silently dropping it.
        RealizedTy::Literal(lit, _, attr) => Cow::Owned(match lit {
            Literal::Int(_) => RealizedTy::Int { attr: attr.clone() },
            Literal::Bigint(_) => RealizedTy::Bigint { attr: attr.clone() },
            Literal::Float(_) => RealizedTy::Float { attr: attr.clone() },
            Literal::String(_) => RealizedTy::String { attr: attr.clone() },
            Literal::Bool(_) => RealizedTy::Bool { attr: attr.clone() },
        }),
        RealizedTy::EnumVariant(name, _, attr) => Cow::Owned(RealizedTy::Enum(*name, attr.clone())),
        _ => Cow::Borrowed(ty),
    }
}

impl<'vm> ImplResolver<'vm> {
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
    /// [`Self::type_implements`]; that is checking, not selection.
    ///
    /// The match is **exact — no near-miss fallback**. An empty `iface_args` request
    /// matches any instantiation, the legitimate case for a *non-generic* interface
    /// (`Equals`/`Compare`), which has nothing to specify. A *generic* interface
    /// always carries its full concrete instantiation (BAML generics are specified,
    /// never truly omitted), and the type checker already proved
    /// `concrete_ty : Iface<those args>` — so `None` means no rule matches a
    /// fully-specified request, i.e. the proof and the runtime registry disagree: an
    /// invariant violation the caller surfaces (a virtual call as an internal error,
    /// a projection as opaque). Returning the first applicable rule instead would
    /// silently dispatch a different instantiation (`Converter<string>` through
    /// `Converter<int>`), reading operands and members at the wrong type.
    pub(crate) fn resolve_implements_rule(
        self,
        concrete_ty: &RealizedTy,
        iface: TypeHead,
        iface_args: &[RealizedTy],
    ) -> Option<(RuntimeImplRuleCandidate<'vm>, Vec<RealizedTy>)> {
        // Static/package-image rules are immutable and already indexed by the
        // canonical interface pointer. Try that slice directly: this is the
        // overwhelmingly common virtual-call path and avoids candidate Vecs,
        // runtime-package graph walks, and dynamic-table locking.
        {
            let iface_ptr = iface.ptr();
            for &rule_ptr in self.vm.packages.impl_rules_of(iface_ptr) {
                let Some(rule) = self.vm.get_object(rule_ptr).as_impl_rule() else {
                    continue;
                };
                if let Some(type_args) = self.requested_rule_args(rule, concrete_ty, iface_args) {
                    return Some((RuntimeImplRuleCandidate::Static(rule), type_args));
                }
            }
        }

        for rule in self.rules_for(iface) {
            if let Some(type_args) = self.requested_rule_args(&rule, concrete_ty, iface_args) {
                return Some((rule, type_args));
            }
        }
        None
    }

    fn requested_rule_args(
        self,
        rule: &RuntimeImplRule,
        concrete_ty: &RealizedTy,
        iface_args: &[RealizedTy],
    ) -> Option<Vec<RealizedTy>> {
        let type_args = self.rule_applies(rule, concrete_ty, &mut Vec::new())?;
        // Select on the interface's input args only (associated types are outputs).
        let rule_args: Vec<RealizedTy> = rule
            .interface_args
            .iter()
            .map(|t| self.substitute_checked(t, &type_args))
            .collect();
        (iface_args.is_empty() || self.ty_args_equivalent(&rule_args, iface_args))
            .then_some(type_args)
    }

    /// Realize a [`MethodImpl`](bex_vm_types::types::MethodImpl) frame template (De
    /// Bruijn over the impl's generic params) against the impl's bound type args
    /// (from [`Self::resolve_implements_rule`]). The result is the `frame.type_args`
    /// to seed the resolved callee with: the impl's own generics for an impl method,
    /// or `Self` + the interface's args + associated types for an inherited default.
    pub(crate) fn realize_frame(
        self,
        template: &[TyTemplate],
        bound_args: &[RealizedTy],
    ) -> Result<Vec<RealizedTy>, VmInternalError> {
        // Materialization boundary: seeding a bytecode frame's realized type args.
        // The impl's frame templates realize fully against the (realized) bound args —
        // every projection reduced through the impl registry — or it is an internal
        // error, never a silent `unknown`.
        template
            .iter()
            .map(|t| {
                t.substitute(bound_args, self.vm)
                    .map_err(|e| VmInternalError::TypeSubstitution {
                        message: e.to_string(),
                    })
            })
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
        self,
        concrete_ty: &RealizedTy,
        iface: TypeHead,
        requested_args: &[RealizedTy],
        requested_assoc: &[(Name, RealizedTy)],
    ) -> bool {
        self.prove(
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
        self,
        concrete_ty: &RealizedTy,
        iface: TypeHead,
        requested_args: &[RealizedTy],
        requested_assoc: &[(Name, RealizedTy)],
        stack: &mut Vec<Obligation>,
    ) -> bool {
        // The blanket stdlib impl exists to supply AnyClass's default-method
        // dispatch. Membership is narrower: class values only. Keep the
        // narrowing at the recursive proof seam so nested bounds cannot
        // observe the blanket rule's broader receiver.
        let any_class = self
            .vm
            .declaration_head(&baml_type::QualifiedTypeName::from_dotted_path(
                "reflect.AnyClass",
            ));
        if any_class.is_some_and(|head| head == iface) {
            return matches!(concrete_ty, RealizedTy::Class(..));
        }

        // Key on the normalized (literal/enum-variant → base) type so `1` and `int`
        // are the same goal for cycle purposes.
        let goal: Obligation = (
            concrete_base(concrete_ty).into_owned(),
            iface,
            requested_args.to_vec(),
            requested_assoc.to_vec(),
        );
        if stack.contains(&goal) || stack.len() >= MAX_OBLIGATION_DEPTH {
            return false;
        }
        stack.push(goal);
        // `rules_for` borrows `self.vm` immutably while `stack` is borrowed
        // mutably inside the predicate, so the candidates are collected first.
        let candidates = self.rules_for(iface);
        let proven = candidates.into_iter().any(|rule| {
            self.rule_applies(&rule, concrete_ty, stack)
                .is_some_and(|bindings| {
                    self.interface_request_matches(
                        &rule,
                        &bindings,
                        requested_args,
                        requested_assoc,
                    )
                })
        });
        stack.pop();
        proven
    }

    /// Match a rule's `for_ty_pattern` against `concrete_ty`, then discharge its
    /// bounds. On success returns the bound generic args in de Bruijn order.
    fn rule_applies(
        self,
        rule: &RuntimeImplRule,
        concrete_ty: &RealizedTy,
        stack: &mut Vec<Obligation>,
    ) -> Option<Vec<RealizedTy>> {
        let base = concrete_base(concrete_ty);
        let concrete_ty = &*base;
        let mut bindings: Vec<Option<RealizedTy>> = vec![None; rule.generic_param_bounds.len()];
        if !self.match_template(&rule.for_ty_pattern, concrete_ty, &mut bindings) {
            return None;
        }
        // The for-type pattern must constrain every generic param — a param the
        // pattern never mentions could not be inferred from the receiver.
        let type_args: Vec<RealizedTy> = bindings.into_iter().collect::<Option<_>>()?;

        // Bounds as nested obligations (rustc winnowing): every interface in a param's
        // bound set must be implemented by that param's bound type arg, at the bound's
        // substituted args/assoc. `prove` guards against cycles and runaway depth.
        for (param, bounds) in rule.generic_param_bounds.iter().enumerate() {
            for bound in bounds {
                // The bound's args/assoc may reference the impl's params (`T extends
                // Container<U>`), so substitute them with the bindings, then require
                // the arg to implement the interface at that exact instantiation.
                let req_args: Vec<RealizedTy> = bound
                    .args
                    .iter()
                    .map(|t| self.substitute_checked(t, &type_args))
                    .collect();
                let req_assoc: Vec<(Name, RealizedTy)> = bound
                    .assoc
                    .iter()
                    .map(|(n, t)| (n.clone(), self.substitute_checked(t, &type_args)))
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
                if self.interface_existential_satisfies_bound(
                    &type_args[param],
                    bound.interface,
                    &req_args,
                    &req_assoc,
                ) {
                    continue;
                }
                if !self.prove(
                    &type_args[param],
                    bound.interface,
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
        self,
        concrete_ty: &RealizedTy,
        iface: TypeHead,
        requested_args: &[RealizedTy],
        requested_assoc: &[(Name, RealizedTy)],
    ) -> bool {
        let base = concrete_base(concrete_ty);
        let RealizedTy::Interface(ex_qtn, ex_args, ex_assoc, _) = base.as_ref() else {
            return false;
        };
        *ex_qtn == iface
            && (requested_args.is_empty() || self.ty_args_equivalent(ex_args, requested_args))
            && self.associated_bindings_equivalent(ex_assoc, requested_assoc)
    }

    /// Unify a `TyTemplate` pattern against a concrete `RealizedTy`, binding each
    /// `TypeArgRef(n)` into `bindings[n]`. A repeated param must bind consistently.
    fn match_template(
        self,
        pattern: &TyTemplate,
        concrete: &RealizedTy,
        bindings: &mut [Option<RealizedTy>],
    ) -> bool {
        // A fully-realized pattern carries no frame refs or holes: compare it to the
        // concrete type semantically (union-order-insensitive, matching the type
        // checker) through the canonical fact-opaque `StructuralEquivCtx`. The
        // flattened successor to the old `Concrete(t)` arm.
        if let Ok(realized) = <&RealizedTy>::try_from(pattern) {
            return StructuralEquivCtx(self.vm).equivalent(realized.as_ty(), concrete.as_ty());
        }

        match pattern {
            TyTemplate::TypeArgRef(n) => {
                match bindings.get_mut(*n as usize) {
                    Some(slot @ None) => {
                        *slot = Some(concrete.clone());
                        true
                    }
                    // A repeated param must bind consistently; compare semantically so a
                    // union binds the same regardless of member order.
                    Some(Some(bound)) => {
                        StructuralEquivCtx(self.vm).equivalent(bound.as_ty(), concrete.as_ty())
                    }
                    None => false,
                }
            }
            TyTemplate::List(inner, _) => match concrete {
                RealizedTy::List(elem, _) => self.match_template(inner, elem, bindings),
                _ => false,
            },
            TyTemplate::Map { key, value, .. } => match concrete {
                RealizedTy::Map {
                    key: ckey,
                    value: cvalue,
                    ..
                } => {
                    self.match_template(key, ckey, bindings)
                        && self.match_template(value, cvalue, bindings)
                }
                _ => false,
            },
            TyTemplate::Class(name, args, _) => match concrete {
                RealizedTy::Class(cname, cargs, _) => {
                    name == cname && self.all_match(args, cargs, bindings)
                }
                _ => false,
            },
            TyTemplate::Interface(name, args, assoc, _) => match concrete {
                RealizedTy::Interface(cname, cargs, cassoc, _) => {
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
                        && self.all_match(args, cargs, bindings)
                        && cassoc.iter().all(|(cn, ct)| {
                            assoc
                                .iter()
                                .find(|(an, _)| an == cn)
                                .is_some_and(|(_, at)| self.match_template(at, ct, bindings))
                        })
                }
                _ => false,
            },
            // A symbolic projection whose witness type was not resolved at compile
            // time — never a match: a runtime value's concrete type carries no
            // unresolved projection to unify with.
            TyTemplate::AssociatedTypeProjection { .. } => false,
            TyTemplate::Function {
                params,
                ret,
                throws,
                ..
            } => match concrete {
                RealizedTy::Function {
                    params: cparams,
                    ret: cret,
                    throws: cthrows,
                    ..
                } => {
                    params.len() == cparams.len()
                        && params.iter().zip(cparams).all(|(p, cp)| {
                            p.name == cp.name
                                && p.mode == cp.mode
                                && self.match_template(&p.ty, &cp.ty, bindings)
                        })
                        && self.match_template(ret, cret, bindings)
                        && self.match_template(throws, cthrows, bindings)
                }
                _ => false,
            },
            TyTemplate::Future(value, error, _) => match concrete {
                RealizedTy::Future(cvalue, cerror, _) => {
                    self.match_template(value, cvalue, bindings)
                        && self.match_template(error, cerror, bindings)
                }
                _ => false,
            },
            // Order-insensitive union match: each pattern member must pair with a
            // distinct concrete member, with type-var bindings consistent across the
            // chosen pairing (so `Box<T | int>` matches a value `Box<int | string>`).
            TyTemplate::Union(parts, _) => match concrete {
                RealizedTy::Union(cparts, _) => self.match_union(parts, cparts, bindings),
                _ => false,
            },
            // Realized leaves are handled by the fast path above.
            _ => false,
        }
    }

    /// Pairwise-unify positional template args against concrete args (same arity).
    fn all_match(
        self,
        patterns: &[TyTemplate],
        concretes: &[RealizedTy],
        bindings: &mut [Option<RealizedTy>],
    ) -> bool {
        patterns.len() == concretes.len()
            && patterns
                .iter()
                .zip(concretes)
                .all(|(p, c)| self.match_template(p, c, bindings))
    }

    /// Match union members order-insensitively: every pattern member must pair with a
    /// *distinct* concrete member (a perfect matching), with type-var bindings
    /// consistent across the pairing. Backtracks — restoring `bindings` after a failed
    /// branch — because a greedy pairing can bind a type var to the wrong member
    /// (e.g. `[T, int]` against `[int, string]` must pick `T = string`, not `T = int`).
    fn match_union(
        self,
        patterns: &[TyTemplate],
        concretes: &[RealizedTy],
        bindings: &mut [Option<RealizedTy>],
    ) -> bool {
        if patterns.len() != concretes.len() {
            return false;
        }
        self.match_union_rec(
            patterns,
            concretes,
            &mut vec![false; concretes.len()],
            bindings,
        )
    }

    fn match_union_rec(
        self,
        patterns: &[TyTemplate],
        concretes: &[RealizedTy],
        used: &mut [bool],
        bindings: &mut [Option<RealizedTy>],
    ) -> bool {
        let Some((first, rest)) = patterns.split_first() else {
            return true;
        };
        for (i, concrete) in concretes.iter().enumerate() {
            if used[i] {
                continue;
            }
            let snapshot = bindings.to_vec();
            if self.match_template(first, concrete, bindings) {
                used[i] = true;
                if self.match_union_rec(rest, concretes, used, bindings) {
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
        self,
        rule: &RuntimeImplRule,
        bindings: &[RealizedTy],
        requested_args: &[RealizedTy],
        requested_assoc: &[(Name, RealizedTy)],
    ) -> bool {
        let rule_args: Vec<RealizedTy> = rule
            .interface_args
            .iter()
            .map(|t| self.substitute_checked(t, bindings))
            .collect();
        let rule_assoc: Vec<(Name, RealizedTy)> = rule
            .interface_assoc
            .iter()
            .map(|(n, t)| (n.clone(), self.substitute_checked(t, bindings)))
            .collect();
        (requested_args.is_empty() || self.ty_args_equivalent(&rule_args, requested_args))
            && self.associated_bindings_equivalent(&rule_assoc, requested_assoc)
    }

    /// Compare two generic-argument lists for *semantic* equivalence, position-wise.
    /// Each pair is related by the canonical set-theoretic algebra through the
    /// fact-opaque [`StructuralEquivCtx`]: union order is normalized away (`Box<int |
    /// string>` ≡ `Box<string | int>`), `never` and literal members collapse
    /// (`int | never == int`), recursive aliases fold (`type A = int | A[]` ≡
    /// `type B = int | B[]`), and interface bindings compare as an order-insensitive
    /// set — while the re-entrant nominal facts stay opaque, so this never re-enters
    /// the resolver (see [`StructuralEquivCtx`]).
    pub(super) fn ty_args_equivalent(self, a: &[RealizedTy], b: &[RealizedTy]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b.iter())
                .all(|(x, y)| StructuralEquivCtx(self.vm).equivalent(x.as_ty(), y.as_ty()))
    }

    pub(super) fn associated_bindings_equivalent(
        self,
        impl_bindings: &[(Name, RealizedTy)],
        requested_bindings: &[(Name, RealizedTy)],
    ) -> bool {
        requested_bindings.is_empty()
            || requested_bindings
                .iter()
                .all(|(requested_name, requested_ty)| {
                    impl_bindings.iter().any(|(impl_name, impl_ty)| {
                        impl_name == requested_name
                            && StructuralEquivCtx(self.vm)
                                .equivalent(impl_ty.as_ty(), requested_ty.as_ty())
                    })
                })
    }
}

impl ImplResolver<'_> {
    /// Realize an impl rule's interface-arg [`TyTemplate`] against a *complete* env
    /// (one binding per impl generic param) for the resolver's own arg *comparison* —
    /// not value materialization. An out-of-range `TypeArgRef` (a malformed rule) is
    /// reported by `substitute` itself and asserted here in debug; in release, any
    /// substitution failure — including a projection a non-matching candidate's
    /// bindings legitimately cannot reduce — falls back to the top type, keeping
    /// selection conservative without aborting the query. (Value materialization
    /// uses [`Self::realize_frame`], which is strict.)
    fn substitute_checked(self, template: &TyTemplate, env: &[RealizedTy]) -> RealizedTy {
        template.substitute(env, self.vm).unwrap_or_else(|e| {
            debug_assert!(
                !matches!(e, baml_type::SubstituteError::TypeArgRefOutOfRange { .. }),
                "impl rule template is malformed: {e}",
            );
            RealizedTy::unknown()
        })
    }
}

impl ImplResolver<'_> {}

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
        let vm = crate::vm::tests::test_vm(Vec::new());
        let resolver = ImplResolver::new(&vm);
        let one = RealizedTy::Literal(Literal::Int(1), Freshness::Regular, TyAttr::default());
        let pattern = TyTemplate::from(RealizedTy::Literal(
            Literal::Int(1),
            Freshness::Regular,
            TyAttr::default(),
        ));
        let mut binds: Vec<Option<RealizedTy>> = Vec::new();
        assert!(resolver.match_template(&pattern, &one, &mut binds));
        assert!(!resolver.match_template(&pattern, &RealizedTy::int(), &mut binds));
    }
}
