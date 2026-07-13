//! A [`baml_type::normalize::TypeContext`] over the running program, so the
//! canonical type algebra (`baml_type::normalize::is_subtype` / `equivalent`)
//! can be used at runtime in place of the context-free `RuntimeTy::is_subtype_of`
//! fork — keeping runtime type decisions in lockstep with the compiler.
//!
//! All but one method are wired to real runtime data, all reached from the VM:
//! - `implements_interface` → the open-world resolver
//!   (`package_baml::type_implements`) over the per-package `impl_rules`.
//! - `alias_def` → the VM's recursive type aliases (via the `packages` index).
//! - `enum_variants` → the `Object::Enum` on the heap (via `vm.lookup_type`, the
//!   `packages` index).
//!
//! The one gap is `interface_requires`, which fails safe per the `TypeContext`
//! contract (a `false`/`None` makes the algebra conservative, never over-claiming):
//! there is no `requires`-closure entry at runtime yet (the resolver proves
//! `concrete: I`, not `I_a requires I_b`).
//!
//! First wired into [`crate::type_match`] — the `IsType` value matcher — as of
//! the canonical-algebra unit. Other `RuntimeTy::is_subtype_of` call sites still
//! migrate onto this context only as the surrounding relation is made canonical
//! (the runtime must not become stricter than the compiler where it would break
//! a proven-exhaustive match — see the List/Map-invariance sequencing constraint;
//! the matcher's callers gate structural tests accordingly).

use baml_type::{Interface, Name, QualifiedTypeName, RealizedTy, Ty, normalize::TypeContext};
use bex_vm_types::types::Object;

use crate::BexVm;

/// The runtime side of the canonical type algebra: interface membership via the
/// VM's per-package `impl_rules` registry, recursive type aliases from the VM, and
/// enum variants off the heap.
pub(crate) struct RuntimeTypeContext<'a> {
    vm: &'a BexVm,
}

impl<'a> RuntimeTypeContext<'a> {
    pub(crate) fn new(vm: &'a BexVm) -> Self {
        Self { vm }
    }
}

impl TypeContext for RuntimeTypeContext<'_> {
    fn alias_def(&self, name: &QualifiedTypeName) -> Option<Ty> {
        // Only recursive aliases survive to runtime; non-recursive ones were
        // expanded inline at lowering. Widen the stored `RuntimeTy` up to `Ty`.
        self.vm.recursive_type_alias(name).map(Ty::from)
    }

    fn implements_interface(&self, concrete: &Ty, interface: &Interface) -> bool {
        // Narrow the algebra's `Ty` operands to the runtime's `RealizedTy` and
        // delegate to the open-world resolver. A narrowing failure (a non-realized
        // variant that cannot exist as a runtime value) fails safe: no membership
        // is claimed.
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
        crate::package_baml::type_implements(self.vm, &concrete, &interface.name, &args, &assoc)
    }

    fn type_var_bound(&self, _name: &Name) -> Vec<Interface> {
        // Runtime types are realized: a bare type variable only appears in
        // reflective metadata, never as a value's type, so there is no bound to
        // discharge here.
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
        let ptr = self.vm.lookup_type(name)?;
        match self.vm.get_object(ptr) {
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
        // Explicitly empty: symbolic associated projections don't arise over
        // realized runtime values, so there is never a `(_ as I).assoc` for the
        // subtype rule to bound here. (The trait requires this method precisely so
        // this "no bounds" decision is deliberate, not a forgotten default.)
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
        let Some((rule, bound_args)) = crate::package_baml::resolve_implements_rule(
            self.vm,
            &base,
            &interface.name,
            &iface_args,
        ) else {
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
