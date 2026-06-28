//! A [`baml_type::normalize::TypeContext`] over the running program, so the
//! canonical type algebra (`baml_type::normalize::is_subtype` / `equivalent`)
//! can be used at runtime in place of the context-free `RuntimeTy::is_subtype_of`
//! fork — keeping runtime type decisions in lockstep with the compiler.
//!
//! All but one method are wired to real runtime data, all reached from the VM:
//! - `implements_interface` → the open-world resolver
//!   (`package_baml::type_implements`) over the baked `interface_impls`.
//! - `alias_def` → the VM's `recursive_type_alias_defs`.
//! - `enum_variants` → the `Object::Enum` on the heap (via `resolved_class_names`).
//!
//! The one gap is `interface_requires`, which fails safe per the `TypeContext`
//! contract (a `false`/`None` makes the algebra conservative, never over-claiming):
//! there is no `requires`-closure entry at runtime yet (the resolver proves
//! `concrete: I`, not `I_a requires I_b`).
//!
//! NOTE: not yet wired into any caller. The `RuntimeTy::is_subtype_of` call sites
//! migrate onto this context only once the compiler adopts the canonical algebra
//! (the runtime must not become stricter than the compiler — see the
//! List/Map-invariance sequencing constraint).

use baml_type::{Interface, Name, QualifiedTypeName, RuntimeTy, Ty, normalize::TypeContext};
use bex_vm_types::types::Object;

use crate::BexVm;

/// The runtime side of the canonical type algebra: interface membership via the
/// VM's baked `interface_impls` registry, recursive type aliases from the VM, and
/// enum variants off the heap.
pub(crate) struct RuntimeTypeContext<'a> {
    vm: &'a BexVm,
}

#[expect(
    dead_code,
    reason = "constructed at the runtime subtyping call sites once the flip lands"
)]
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
        // Narrow the algebra's `Ty` operands to `RuntimeTy` and delegate to the
        // open-world resolver. A narrowing failure (a compiler-only variant that
        // cannot exist at runtime) fails safe: no membership is claimed.
        let Ok(concrete) = RuntimeTy::try_from(concrete) else {
            return false;
        };
        let Ok(args) = interface
            .generics
            .iter()
            .map(RuntimeTy::try_from)
            .collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };
        let Ok(assoc) = interface
            .associated_types
            .iter()
            .map(|(name, ty)| RuntimeTy::try_from(ty).map(|ty| (name.clone(), ty)))
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

    // `associated_type_bound` uses the trait default (`Vec::new()`): symbolic
    // associated projections don't arise over realized runtime values.
}
