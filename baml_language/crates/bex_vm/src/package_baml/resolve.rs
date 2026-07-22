//! Runtime interface-method resolver — rustc-style trait selection over each
//! package's baked impl-rule tables (`Package::impl_rules`, via `vm.packages`).
//!
//! Given a value's concrete runtime type plus an interface and method name, it
//! returns the applicable impl's method — a concrete callee and the impl's bound
//! type args (de Bruijn order, to seed the callee's frame) — or `None` when no
//! impl applies (the caller decides the fallback).
//!
//! This mirrors the compiler's selection (`match_ty_pattern` + bound validation
//! in `baml_compiler2_tir::interfaces`), run on `baml_type::RealizedTy`: unify the rule's
//! `for_ty_pattern` against the concrete type (binding the impl's generic
//! params), then discharge each param's declared bound as a nested obligation.

use std::borrow::Cow;

use baml_type::{
    Literal, MediaKind, Name, RealizedTy, TyAttr, TyTemplate, TypeName, normalize::TypeContext,
};
use bex_vm_types::{
    HeapPtr,
    errors::VmInternalError,
    types::{Object, Package, RuntimeImplRule},
};

use crate::{BexVm, type_context::StructuralEquivCtx};

/// The runtime impl resolver over a running VM. Holding the `&BexVm` here —
/// rather than threading it through every helper — keeps the whole selection
/// machinery able to reach VM facts at any depth: candidate collection reads the
/// package tables, and every leaf type comparison builds the alias-expanding
/// [`StructuralEquivCtx`], so recursive aliases fold correctly (see its docs for
/// the fact profile and why the re-entrant facts stay opaque).
#[derive(Clone, Copy)]
pub(crate) struct ImplResolver<'vm> {
    vm: &'vm BexVm,
}

impl<'vm> ImplResolver<'vm> {
    pub(crate) fn new(vm: &'vm BexVm) -> Self {
        Self { vm }
    }

    /// Dereference a package pointer to its [`Package`]. The runtime `vm.packages`
    /// index only ever holds `Object::Package` pointers.
    fn deref_package(self, ptr: HeapPtr) -> &'vm Package {
        self.vm
            .get_object(ptr)
            .as_package()
            .unwrap_or_else(|| unreachable!("vm.packages pointer is not an Object::Package"))
    }

    /// Append every impl rule of the interface at `iface_ptr` declared in `package`
    /// to `out`. `impl_rules` is keyed by the interface's canonical `Object::Interface`
    /// pointer, so this is an O(1) lookup.
    fn collect_package_rules(
        self,
        package: &'vm Package,
        iface_ptr: HeapPtr,
        out: &mut Vec<&'vm RuntimeImplRule>,
    ) {
        let Some(rule_ptrs) = package.impl_rules.get(&iface_ptr) else {
            return;
        };
        for &rule_ptr in rule_ptrs {
            if let Some(rule) = self.vm.get_object(rule_ptr).as_impl_rule() {
                out.push(rule);
            }
        }
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
    TypeName,
    Vec<RealizedTy>,
    Vec<(Name, RealizedTy)>,
);

/// The package that owns `ty`, if any. Primitives/containers have none — their
/// impls live in the interface's package (orphan rule).
fn type_package(ty: &RealizedTy) -> Option<&Name> {
    match ty {
        RealizedTy::Class(tn, ..) | RealizedTy::Enum(tn, ..) | RealizedTy::Interface(tn, ..) => {
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
        RealizedTy::EnumVariant(name, _, attr) => {
            Cow::Owned(RealizedTy::Enum(name.clone(), attr.clone()))
        }
        _ => Cow::Borrowed(ty),
    }
}

impl<'vm> ImplResolver<'vm> {
    /// Candidate rules for `<concrete_ty as iface>`. By the orphan rule an
    /// `implement iface for concrete_ty` lives in `concrete_ty`'s package or
    /// `iface`'s package (a blanket impl lives in `iface`'s package even when it
    /// matches a type owned elsewhere), so gather `iface`'s rules from both — deduped
    /// when they are the same package. A newly-loaded package only adds entries, so
    /// it never changes an existing pair's answer.
    fn candidate_rules(
        self,
        concrete_ty: &RealizedTy,
        iface: &TypeName,
    ) -> Vec<&'vm RuntimeImplRule> {
        let base = concrete_base(concrete_ty);
        let concrete_ty = &*base;
        // Resolve the interface's canonical object pointer once; every package's
        // `impl_rules` is keyed by it, turning rule collection into an O(1) lookup.
        // An unknown interface has no impls anywhere.
        let Some(iface_ptr) = self.vm.lookup_interface(iface) else {
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
            let Some(&pkg_ptr) = self.vm.packages.get(pkg) else {
                continue;
            };
            self.collect_package_rules(self.deref_package(pkg_ptr), iface_ptr, &mut out);
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
        iface: &TypeName,
        iface_args: &[RealizedTy],
    ) -> Option<(&'vm RuntimeImplRule, Vec<RealizedTy>)> {
        for rule in self.candidate_rules(concrete_ty, iface) {
            let Some(type_args) = self.rule_applies(rule, concrete_ty, &mut Vec::new()) else {
                continue;
            };
            // Select on the interface's input args only (associated types are outputs).
            let rule_args: Vec<RealizedTy> = rule
                .interface_args
                .iter()
                .map(|t| self.substitute_checked(t, &type_args))
                .collect();
            if iface_args.is_empty() || self.ty_args_equivalent(&rule_args, iface_args) {
                return Some((rule, type_args));
            }
        }
        None
    }

    /// Realize a [`MethodImpl`](bex_vm_types::types::MethodImpl) frame template (De
    /// Bruijn over the impl's generic params) against the impl's bound type args
    /// (from [`Self::resolve_implements_rule`]). The result is the `frame.type_args`
    /// to seed the resolved callee with: the impl's own generics for an impl method,
    /// or the interface's args + associated types for an inherited default.
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
        iface: &TypeName,
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
        iface: &TypeName,
        requested_args: &[RealizedTy],
        requested_assoc: &[(Name, RealizedTy)],
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
        let proven = self
            .candidate_rules(concrete_ty, iface)
            .into_iter()
            .any(|rule| {
                self.rule_applies(rule, concrete_ty, stack)
                    .is_some_and(|bindings| {
                        self.interface_request_matches(
                            rule,
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
                    &bound.interface,
                    &req_args,
                    &req_assoc,
                ) {
                    continue;
                }
                if !self.prove(
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
        self,
        concrete_ty: &RealizedTy,
        iface: &TypeName,
        requested_args: &[RealizedTy],
        requested_assoc: &[(Name, RealizedTy)],
    ) -> bool {
        let base = concrete_base(concrete_ty);
        let RealizedTy::Interface(ex_qtn, ex_args, ex_assoc, _) = base.as_ref() else {
            return false;
        };
        ex_qtn == iface
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

/// One implementor of an interface: the concrete implementor type plus the
/// interface args / associated bindings that impl pins (empty = "matches any
/// instantiation", for blanket impls and typevar-bearing dimensions).
type ImplementorEntry = (RealizedTy, Vec<RealizedTy>, Vec<(Name, RealizedTy)>);

impl ImplResolver<'_> {
    /// The concrete types that nominally implement `iface`, each paired with the
    /// interface instantiation its impl fixes — the inverse of
    /// [`Self::type_implements`]. Reflection (`type.implementors()`) filters these by
    /// the requested args/assoc.
    ///
    /// By the orphan rule an impl of `iface` may live in any package, so every
    /// package's table is scanned. A non-generic impl contributes its concrete
    /// for-type; a generic class contributes its base (instantiations can't be
    /// enumerated, so typevar-bearing interface args are erased to "match any"); a
    /// blanket `for T` contributes every loaded concrete class whose bounds it
    /// satisfies. Container/union for-types have no nominal implementor to list.
    pub(super) fn implementor_entries(self, iface: &TypeName) -> Vec<ImplementorEntry> {
        let mut out: Vec<ImplementorEntry> = Vec::new();
        // Resolve the interface's canonical object pointer once; an unknown interface
        // has no implementors. Every package's `impl_rules` is keyed by this pointer.
        let Some(iface_ptr) = self.vm.lookup_interface(iface) else {
            return out;
        };
        for &pkg_ptr in self.vm.packages.values() {
            let mut rules: Vec<&RuntimeImplRule> = Vec::new();
            self.collect_package_rules(self.deref_package(pkg_ptr), iface_ptr, &mut rules);
            for rule in rules {
                match &rule.for_ty_pattern {
                    TyTemplate::TypeArgRef(_) => {
                        // Blanket impl: its bounds decide membership; every concrete
                        // type satisfying them is an implementor, at the interface
                        // instantiation the blanket pins (typevar dimensions erased).
                        let (args, assoc) = self.pinned_interface_instantiation(rule);
                        for ty in self.concrete_types() {
                            if self.rule_applies(rule, &ty, &mut Vec::new()).is_some() {
                                push_unique(&mut out, (ty, args.clone(), assoc.clone()));
                            }
                        }
                    }
                    // A concrete for-type (`int`, a monomorphic class, `int[]`, …)
                    // narrows to a realized type — the implementor is that type.
                    other if <&RealizedTy>::try_from(other).is_ok() => {
                        let realized = <&RealizedTy>::try_from(other)
                            .unwrap_or_else(|_| unreachable!("guarded by the `is_ok` above"));
                        let (args, assoc) = self.pinned_interface_instantiation(rule);
                        push_unique(&mut out, (realized.clone(), args, assoc));
                    }
                    // A generic class for-type (`Foo<T>`) is reported by its base.
                    TyTemplate::Class(name, _, _) => {
                        let (args, assoc) = self.pinned_interface_instantiation(rule);
                        let base = RealizedTy::Class(name.clone(), Vec::new(), TyAttr::default());
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
        self,
        rule: &RuntimeImplRule,
    ) -> (Vec<RealizedTy>, Vec<(Name, RealizedTy)>) {
        let args = if rule.interface_args.iter().any(template_has_type_arg_ref) {
            Vec::new()
        } else {
            rule.interface_args
                .iter()
                .map(|t| self.substitute_checked(t, &[]))
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
                .map(|(n, t)| (n.clone(), self.substitute_checked(t, &[])))
                .collect()
        };
        (args, assoc)
    }

    /// Realize an impl rule's interface-arg [`TyTemplate`] against a *complete* env
    /// (one binding per impl generic param) for the resolver's own arg *comparison* —
    /// not value materialization. A debug assert catches an out-of-range `TypeArgRef`
    /// (a malformed rule); in release, a substitution failure falls back to the top
    /// type, keeping selection conservative without aborting the query. (Value
    /// materialization uses [`Self::realize_frame`], which is strict.)
    fn substitute_checked(self, template: &TyTemplate, env: &[RealizedTy]) -> RealizedTy {
        debug_assert!(
            max_type_arg_ref(template).is_none_or(|n| (n as usize) < env.len()),
            "impl rule references a type arg out of range for an env of {}",
            env.len(),
        );
        template
            .substitute(env, self.vm)
            .unwrap_or_else(|_| RealizedTy::unknown())
    }
}

/// The largest `TypeArgRef` de Bruijn index anywhere in `t`, if any.
fn max_type_arg_ref(t: &TyTemplate) -> Option<u32> {
    match t {
        TyTemplate::TypeArgRef(n) => Some(*n),
        TyTemplate::List(inner, _) => max_type_arg_ref(inner),
        TyTemplate::Map { key, value, .. } | TyTemplate::Future(key, value, _) => {
            max_type_arg_ref(key).max(max_type_arg_ref(value))
        }
        TyTemplate::Union(parts, _) => parts.iter().filter_map(max_type_arg_ref).max(),
        TyTemplate::Class(_, args, _) => args.iter().filter_map(max_type_arg_ref).max(),
        TyTemplate::Interface(_, args, assoc, _) => args
            .iter()
            .filter_map(max_type_arg_ref)
            .chain(assoc.iter().filter_map(|(_, t)| max_type_arg_ref(t)))
            .max(),
        TyTemplate::Function {
            params,
            ret,
            throws,
            ..
        } => params
            .iter()
            .filter_map(|p| max_type_arg_ref(&p.ty))
            .chain(max_type_arg_ref(ret))
            .chain(max_type_arg_ref(throws))
            .max(),
        TyTemplate::AssociatedTypeProjection {
            base, interface, ..
        } => max_type_arg_ref(base)
            .into_iter()
            .chain(
                interface
                    .generics
                    .iter()
                    .filter_map(max_type_arg_ref)
                    .chain(
                        interface
                            .associated_types
                            .iter()
                            .filter_map(|(_, t)| max_type_arg_ref(t)),
                    ),
            )
            .max(),
        // Realized leaves carry no frame ref.
        _ => None,
    }
}

/// Whether a template references any impl generic parameter (a `TypeArgRef`).
fn template_has_type_arg_ref(t: &TyTemplate) -> bool {
    match t {
        TyTemplate::TypeArgRef(_) => true,
        TyTemplate::List(inner, _) => template_has_type_arg_ref(inner),
        TyTemplate::Map { key, value, .. } | TyTemplate::Future(key, value, _) => {
            template_has_type_arg_ref(key) || template_has_type_arg_ref(value)
        }
        TyTemplate::Union(parts, _) => parts.iter().any(template_has_type_arg_ref),
        TyTemplate::Class(_, args, _) => args.iter().any(template_has_type_arg_ref),
        TyTemplate::Interface(_, args, assoc, _) => {
            args.iter().any(template_has_type_arg_ref)
                || assoc.iter().any(|(_, t)| template_has_type_arg_ref(t))
        }
        TyTemplate::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params.iter().any(|p| template_has_type_arg_ref(&p.ty))
                || template_has_type_arg_ref(ret)
                || template_has_type_arg_ref(throws)
        }
        TyTemplate::AssociatedTypeProjection {
            base, interface, ..
        } => {
            template_has_type_arg_ref(base)
                || interface.generics.iter().any(template_has_type_arg_ref)
                || interface
                    .associated_types
                    .iter()
                    .any(|(_, t)| template_has_type_arg_ref(t))
        }
        // Realized leaves carry no frame ref.
        _ => false,
    }
}

impl ImplResolver<'_> {
    /// Every loaded concrete type — classes, enums, and the primitives — as a `RealizedTy` at
    /// its base (no type args). The candidate set a blanket impl's bounds are checked
    /// against: a blanket `for T` can be satisfied by any concrete type, not just
    /// classes. (`all_class_and_enum_ptrs` covers both classes and enums across every
    /// loaded package.) `$stream` companions are filtered later by
    /// `push_unique`.
    fn concrete_types(self) -> Vec<RealizedTy> {
        let mut types: Vec<RealizedTy> = self
            .vm
            .all_class_and_enum_ptrs()
            .filter_map(|ptr| match self.vm.get_object(ptr) {
                Object::Class(class) => Some(RealizedTy::Class(
                    class.name.clone(),
                    Vec::new(),
                    TyAttr::default(),
                )),
                Object::Enum(enum_def) => {
                    Some(RealizedTy::Enum(enum_def.name.clone(), TyAttr::default()))
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
            RealizedTy::Int {
                attr: TyAttr::default(),
            },
            RealizedTy::Bigint {
                attr: TyAttr::default(),
            },
            RealizedTy::Float {
                attr: TyAttr::default(),
            },
            RealizedTy::String {
                attr: TyAttr::default(),
            },
            RealizedTy::Bool {
                attr: TyAttr::default(),
            },
            RealizedTy::Null {
                attr: TyAttr::default(),
            },
            RealizedTy::Uint8Array {
                attr: TyAttr::default(),
            },
            RealizedTy::Type {
                attr: TyAttr::default(),
            },
            RealizedTy::Resource {
                attr: TyAttr::default(),
            },
            RealizedTy::PromptAst {
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
            .map(|kind| RealizedTy::Media(kind, TyAttr::default())),
        );
        types
    }
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
fn is_stream_companion(ty: &RealizedTy) -> bool {
    match ty {
        RealizedTy::Class(tn, ..) | RealizedTy::Enum(tn, ..) => tn.name().ends_with("$stream"),
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
