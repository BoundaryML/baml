//! [`BexVm`] as a [`baml_type::normalize::TypeContext`], so the canonical type
//! algebra (`baml_type::normalize::is_subtype` / `equivalent`) can run at
//! runtime over the running program in place of the context-free
//! `RuntimeTy::is_subtype_of` fork — keeping runtime type decisions in lockstep
//! with the compiler.
//!
//! The VM *is* the runtime type context: it holds the whole program, so it can
//! answer the nominal facts the structural algebra cannot derive on its own.
//! Most methods are wired to real runtime data:
//! - `implements_interface` → the open-world resolver
//!   (`package_baml::type_implements`) over the per-package `impl_rules`.
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

use baml_type::{Interface, Name, QualifiedTypeName, RealizedTy, Ty, normalize::TypeContext};
use bex_vm_types::types::Object;

use crate::BexVm;

impl TypeContext for BexVm {
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
        crate::package_baml::type_implements(self, &concrete, &interface.name, &args, &assoc)
    }

    fn type_var_bound(&self, _name: &Name) -> Vec<Interface> {
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
        // yet (the resolver proves `concrete: I`, not `I_a requires I_b`). Fail
        // safe — claim no proper requirement; interface-to-interface subtyping
        // degrades to identity until a baked `requires` fact (or the resolver's
        // `interface_existential_satisfies_bound`) is exposed.
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
        let Some((rule, bound_args)) =
            crate::package_baml::resolve_implements_rule(self, &base, &interface.name, &iface_args)
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
}
