//! [`BexVm`] as a [`baml_type::normalize::TypeContext`], so the canonical type
//! algebra (`baml_type::normalize::is_subtype` / `equivalent`) runs at runtime
//! over the running program — keeping runtime type decisions in lockstep with
//! the compiler. (This replaced the since-deleted context-free structural
//! subtype fork.)
//!
//! The VM *is* the runtime type context: it holds the whole program, so it can
//! answer the nominal facts the structural algebra cannot derive on its own.
//! Most methods are wired to real runtime data:
//! - `implements_interface` → the open-world resolver
//!   (`ImplResolver::type_implements`) over the per-package `impl_rules`.
//! - `alias_def` → the VM's recursive type aliases (via the `packages` index).
//! - `enum_variants` → the `Object::Enum` on the heap (via `vm.lookup_type`, the
//!   `packages` index).
//! - `project` → the resolver's associated-type binding, realized against the
//!   selected impl.
//!
//! `type_var_bound` and `associated_type_bound` return empty by the
//! realized-operand invariant: runtime subtype queries are always over realized
//! types, so the algebra's `TypeVar` / symbolic-projection arms — the only
//! callers of these two — are never reached (a `debug_assert!` guards it). The
//! one genuine fail-safe gap is `interface_requires`, which returns `false`
//! (conservative, never over-claiming) because no `requires`-closure entry exists
//! at runtime yet — the resolver proves `concrete: I`, not `I_a requires I_b`.
//!
//! Pass a `&BexVm` directly wherever the algebra wants a `&impl TypeContext`
//! (e.g. `normalize::is_subtype(a, b, vm)`). Call sites migrate onto this only as
//! the surrounding relation is made canonical (the runtime must not become
//! stricter than the compiler where it would break a proven-exhaustive match —
//! see the List/Map-invariance sequencing constraint).

use std::ops::ControlFlow;

use baml_type::{
    ClauseId, ImplClause, Interface, Name, ParamTy, QualifiedTypeName, RealizedTy, Ty, TypeName,
    normalize::{Limits, TypeContext},
};
use bex_vm_types::types::Object;

use crate::BexVm;

/// Overflow backstop for membership-goal recursion. Cycle detection (the
/// solver's canonical-keyed repeat scan) already rejects goals that *repeat*;
/// this guards the other non-terminating
/// shape — goals that *grow* without ever repeating (`T: I` ⇒ `Container<T>: I`
/// ⇒ `Container<Container<T>>: I` ⇒ …), which a cycle check cannot see. rustc
/// keeps a fixed `recursion_limit` for exactly this reason. Realistic chains
/// are 1–3 deep (each normal step shrinks the type), so only pathological
/// bounds ever reach this.
pub(crate) const MAX_OBLIGATION_DEPTH: usize = 128;

impl TypeContext for BexVm {
    fn limits(&self) -> Limits {
        // The one configuration source for every derivation over this program;
        // the profiles below forward here so a session runs under the same
        // limits whichever fact profile it was built over.
        Limits {
            recursion_limit: MAX_OBLIGATION_DEPTH,
            ..Limits::DEFAULT
        }
    }

    fn alias_def(&self, name: &QualifiedTypeName) -> Option<Ty> {
        // Only recursive aliases survive to runtime; non-recursive ones were
        // expanded inline at lowering. Widen the stored `RuntimeTy` up to `Ty`.
        self.recursive_type_alias(name).map(Ty::from)
    }

    fn implements_interface(&self, concrete: &Ty, interface: &Interface) -> bool {
        // Narrow the algebra's `Ty` operands to the runtime's `RealizedTy` and
        // delegate to the open-world resolver. A narrowing failure (a non-realized
        // variant such as `TypeVar` — which can be a runtime type as data, but
        // never the type of an actual value) fails safe: no membership is claimed.
        let Ok(concrete) = RealizedTy::try_from(concrete) else {
            return false;
        };
        let Ok(args) = interface
            .generics
            .iter()
            .map(RealizedTy::try_from)
            .collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };
        let Ok(assoc) = interface
            .associated_types
            .iter()
            .map(|(name, ty)| RealizedTy::try_from(ty).map(|ty| (name.clone(), ty)))
            .collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };
        crate::package_baml::ImplResolver::new(self).type_implements(
            &concrete,
            &interface.name,
            &args,
            &assoc,
        )
    }

    fn type_var_bound(&self, _param: &ParamTy) -> Vec<Interface> {
        // Runtime subtype queries are always over realized operands: templates are
        // substituted against realized frame args before any comparison, and the
        // resolver narrows to `RealizedTy` — so a `NormalTy::TypeVar` node (the
        // only thing that reaches this method in the canonical algebra) never
        // arises here. Unrealized types *do* exist at runtime as data (`RuntimeTy`
        // carries `TypeVar`, used by type constructors and reflection metadata),
        // but they are realized before being subtyped; a bare type-var *name* here
        // would moreover carry no owning scope for the VM to resolve a bound
        // against, unlike the compiler's `GlobalTypeContext` (which reads the
        // enclosing scope's `T: A & B` conjunctions).
        //
        // Assert that invariant loudly in debug — a hit means an unrealized operand
        // reached the algebra without being realized upstream (the bug is there,
        // not here) — and fall back to the sound, fail-closed answer in release:
        // empty bounds make `T <: I` yield `false`, never over-claiming membership.
        debug_assert!(
            false,
            "runtime `type_var_bound` reached: an unrealized operand was subtyped \
             without being realized first"
        );
        Vec::new()
    }

    fn interface_requires(&self, _sub: &Interface, _sup: &Interface) -> bool {
        // TODO(runtime-requires): no `requires`-closure entry exists at runtime
        // yet (the solver proves `concrete: I`, not `I_a requires I_b`). Fail
        // safe — claim no proper requirement; interface-to-interface subtyping
        // degrades to identity until a baked `requires` fact is exposed.
        false
    }

    fn enum_variants(&self, name: &QualifiedTypeName) -> Option<Vec<Name>> {
        // Enums live on the heap; look the qualified name up through its package
        // (classes and enums share one type namespace).
        let ptr = self.lookup_type(name)?;
        match self.get_object(ptr) {
            Object::Enum(en) => Some(
                en.variants
                    .iter()
                    .map(|v| Name::new(v.name.as_str()))
                    .collect(),
            ),
            _ => None,
        }
    }

    fn associated_type_bound(&self, _interface: &Interface, _assoc: Name) -> Vec<Interface> {
        // Empty, for the same reason as `type_var_bound`: runtime subtype queries
        // are over realized operands, so a still-symbolic `(_ as I).assoc`
        // projection never reaches the subtype rule that would consult this bound —
        // and the VM holds no scope to resolve one anyway. Loud in debug if that is
        // ever violated; sound and fail-closed in release (an unresolvable
        // projection is simply not judged a subtype).
        debug_assert!(
            false,
            "runtime `associated_type_bound` reached: an unrealized projection was \
             subtyped without being realized first"
        );
        Vec::new()
    }

    fn project(
        &self,
        base: &Ty,
        interface: &Interface,
        member: &Name,
        fuel: u32,
    ) -> baml_type::normalize::ProjectionStep {
        use baml_type::normalize::ProjectionStep;
        // Reduce `(base as I).member` to the impl's binding when the base is a
        // realized runtime type — the runtime twin of the compiler's projection
        // reduction. A symbolic (non-realized) base, an interface arg that is not
        // realized, no applicable impl, or a binding that does not itself realize
        // all leave the projection opaque (equal only to itself), never a wrong
        // reduction.
        let Ok(base) = RealizedTy::try_from(base) else {
            return ProjectionStep::Opaque;
        };
        let Ok(iface_args) = interface
            .generics
            .iter()
            .map(RealizedTy::try_from)
            .collect::<Result<Vec<_>, _>>()
        else {
            return ProjectionStep::Opaque;
        };
        // Select the applicable impl and read its associated-type binding template.
        let Some((rule, bound_args)) = crate::package_baml::ImplResolver::new(self)
            .resolve_implements_rule(&base, &interface.name, &iface_args)
        else {
            return ProjectionStep::Opaque;
        };
        let Some((_, template)) = rule.interface_assoc.iter().find(|(n, _)| n == member) else {
            return ProjectionStep::Opaque;
        };
        // Realize the binding against the impl's bound args; widen back into `Ty`
        // for the canonical algebra. `fuel` is threaded on so a cyclic
        // associated-type binding — whose realization re-enters `project` — is
        // bounded rather than recursing forever (the runtime twin of `from_ty`).
        match template.substitute_with_fuel(&bound_args, self, fuel) {
            Ok(reduced) => ProjectionStep::Reduced(reduced.into()),
            Err(_) => ProjectionStep::Opaque,
        }
    }

    /// The VM as a clause supplier.
    ///
    /// The baked table is already in clause form — `RuntimeImplRule` carries the
    /// `for`-pattern, the interface's arguments and bindings, and the per-parameter
    /// bounds as templates over the impl's own generics — so supplying a clause is
    /// borrowing those fields, not building anything. What the rule *also* carries
    /// (methods, field links) stays behind: that is what a caller reads once a
    /// clause has been selected, not what decides the selection.
    ///
    /// **Order** is the program-wide impl table's: packages in load order
    /// (dependencies first), and within a package the order the bake fixed — both
    /// deterministic for a given program, as the trait contract requires.
    ///
    /// The clause's [`ClauseId`] is the rule's heap *object* pointer, so whoever
    /// selected a clause can recover the rule — and therefore the method table —
    /// without a second search.
    fn for_each_clause<'a>(
        &'a self,
        interface: &TypeName,
        visit: &mut dyn FnMut(ImplClause<'a>) -> ControlFlow<()>,
    ) {
        // An interface no package loaded has no implementations anywhere, which is
        // a legitimate answer rather than a failure.
        let Some(interface_ptr) = self.lookup_interface(interface) else {
            return;
        };
        for &rule_ptr in self.packages.impl_rules_of(interface_ptr) {
            let Some(rule) = self.get_object(rule_ptr).as_impl_rule() else {
                continue;
            };
            let clause = ImplClause {
                id: ClauseId(rule_ptr.as_ptr() as u64),
                num_vars: rule.generic_param_bounds.len(),
                self_pattern: &rule.for_ty_pattern,
                iface_args: &rule.interface_args,
                iface_assoc: &rule.interface_assoc,
                bounds: &rule.generic_param_bounds,
            };
            if visit(clause).is_break() {
                return;
            }
        }
    }
}

/// The **dispatch clause profile**: the world a runtime impl-selection session
/// ([`baml_type::normalize::SolverSession`]) reasons under, expressed as data —
/// which facts of this VM's program are live and which stay severed.
///
/// Live:
/// - **Aliases** ([`TypeContext::alias_def`]) — non-re-entrant map lookups,
///   required for equirecursive folding in pattern comparisons.
/// - **Clauses** ([`TypeContext::for_each_clause`]) — the baked impl table; the
///   session's own clause search consumes them, which is exactly what makes the
///   severed membership fact below safe to sever.
/// - **Projection reduction** ([`TypeContext::project`]) — clause-template
///   *realization* (`TyTemplate::substitute` against this profile) must reduce
///   associated-type projections for real, as today's substitution path does
///   with the full context; its re-entry into selection is fuel-bounded.
///   Verdict-safe for comparisons because realized runtime operands structurally
///   exclude projections (the `RealizedTy` axis), so a *comparison* never
///   consults this — only realization does.
///
/// Severed (each a fail-safe `false`/`None`/empty, never an over-claim):
/// - **Membership** (`implements_interface`): membership is the session's own
///   goal, decided by its clause search — a fact that answered it here would be
///   a second, severed search re-entered from inside the first (unboundedly,
///   via the canonicalizer's `absorb_subtypes`). Flips when the shared profile
///   turns fact-driven absorption on for compiler and runtime in lockstep.
/// - **`interface_requires`**: no requires-closure exists at runtime yet.
/// - **Enum completeness** (`enum_variants`): non-re-entrant and could be made
///   real, but only in lockstep with the compiler matcher's `AliasEquivCtx` —
///   the documented conservative miss both sides share.
/// - **Type-variable / associated bounds**: realized operands never reach the
///   arms that consult them.
///
/// Limits forward to the VM's ([`TypeContext::limits`] on [`BexVm`]), so every
/// derivation over this program runs under one configuration.
pub(crate) struct DispatchProfile<'a>(pub(crate) &'a BexVm);

impl TypeContext for DispatchProfile<'_> {
    fn limits(&self) -> Limits {
        self.0.limits()
    }

    fn alias_def(&self, name: &QualifiedTypeName) -> Option<Ty> {
        self.0.alias_def(name)
    }

    fn implements_interface(&self, _concrete: &Ty, _interface: &Interface) -> bool {
        false // Severed: membership is the session's own clause search.
    }

    fn type_var_bound(&self, _param: &ParamTy) -> Vec<Interface> {
        Vec::new()
    }

    fn interface_requires(&self, _sub: &Interface, _sup: &Interface) -> bool {
        false // Severed: no runtime requires-closure yet.
    }

    fn enum_variants(&self, _name: &QualifiedTypeName) -> Option<Vec<Name>> {
        None // Severed: lockstep miss with the compiler matcher.
    }

    fn associated_type_bound(&self, _interface: &Interface, _assoc: Name) -> Vec<Interface> {
        Vec::new()
    }

    fn project(
        &self,
        base: &Ty,
        interface: &Interface,
        member: &Name,
        fuel: u32,
    ) -> baml_type::normalize::ProjectionStep {
        // Real: realization needs actual reductions (fuel-bounded re-entry into
        // selection, the same re-entrancy today's substitution path pays).
        self.0.project(base, interface, member, fuel)
    }

    fn for_each_clause<'a>(
        &'a self,
        interface: &TypeName,
        visit: &mut dyn FnMut(ImplClause<'a>) -> ControlFlow<()>,
    ) {
        // Real: the session's clause search runs over the baked impl table.
        self.0.for_each_clause(interface, visit);
    }
}
