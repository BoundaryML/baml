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

use baml_type::{
    Interface, Name, ParamTy, QualifiedTypeName, RealizedTy, Ty, normalize::TypeContext,
};
use bex_vm_types::types::Object;

use crate::BexVm;

impl TypeContext for BexVm {
    /// A name-based context represents a declaration by its own name, so this
    /// is the identity — no resolution step, and never `None`.
    fn head_lookup(
        &self,
        qtn: &baml_type::QualifiedTypeName,
    ) -> Option<baml_type::QualifiedTypeName> {
        Some(qtn.clone())
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
}

/// A [`TypeContext`] for order-insensitive *structural* equivalence over realized
/// runtime types: **aliases are expanded** — so recursive aliases are
/// equirecursively correct (`type A = int | A[]` and `type B = int | B[]` denote
/// the same type at any unfolding depth, via the canonicalizer's
/// μ-canonicalization: α-invariant de Bruijn binders plus automaton
/// minimization; `TYPE_SYSTEM.md` §Type Aliases and Recursive Types) — while every
/// *re-entrant* nominal fact stays an opaque leaf. It runs the canonical
/// set-theoretic algebra (union flatten/sort/dedup, `never` removal,
/// literal-into-base collapse), and interface bindings compare
/// order-insensitively because the canonicalizer sorts them.
///
/// This is the runtime twin of the compiler matcher's `AliasEquivCtx` and shares
/// its exact fact profile — alias-real, nominal-opaque — which keeps runtime
/// dispatch in agreement with compile-time coherence. `alias_def` is safe to
/// answer because it is non-re-entrant (a map lookup; the expanded body normalizes
/// under this same opaque context). `implements_interface` and `project` MUST stay
/// opaque: the resolver is a link in the membership chain
/// ([`BexVm::implements_interface`](TypeContext::implements_interface) →
/// `type_implements` → `rule_applies` → the equivalence check → here), so
/// answering them would re-enter it — `implements_interface` unboundedly (via the
/// canonicalizer's `absorb_subtypes`), `project` fuel-bounded but wastefully.
///
/// Two conservative *misses* remain, documented and shared with the compiler
/// matcher (so the two sides agree): interface-membership absorption
/// (`Shape | Sq` ≡ `Shape` when `Sq <: Shape`) and enum completeness
/// (`E.A | E.B | …` ≡ `E`). Bool completeness (`true | false ≡ bool`) is *not*
/// among them: its variant family is closed, so the collapse is context-free
/// and applies here too. Membership is *forced* opaque by re-entry; enum is
/// non-re-entrant and could be made real, but only in lockstep with
/// `AliasEquivCtx`. Their soundness rests on the compiler emitting dispatch
/// requests already in absorbed form. The general fix is a unified goal solver
/// whose in-progress table spans the equivalence↔membership boundary (so
/// membership can be answered as a *goal* with per-sort cycle semantics instead
/// of severed here); until then, opacity is the termination guarantee.
pub(crate) struct StructuralEquivCtx<'a>(pub(crate) &'a BexVm);

impl TypeContext for StructuralEquivCtx<'_> {
    /// A name-based context represents a declaration by its own name, so this
    /// is the identity — no resolution step, and never `None`.
    fn head_lookup(
        &self,
        qtn: &baml_type::QualifiedTypeName,
    ) -> Option<baml_type::QualifiedTypeName> {
        Some(qtn.clone())
    }

    fn alias_def(&self, name: &QualifiedTypeName) -> Option<Ty> {
        // Same alias facts as the full context — non-re-entrant, and required
        // for recursive-alias folding.
        self.0.alias_def(name)
    }

    fn implements_interface(&self, _concrete: &Ty, _interface: &Interface) -> bool {
        false // Opaque: re-entrant (→ the resolver), unboundedly.
    }

    fn type_var_bound(&self, _param: &ParamTy) -> Vec<Interface> {
        Vec::new()
    }

    fn interface_requires(&self, _sub: &Interface, _sup: &Interface) -> bool {
        false // Opaque.
    }

    fn enum_variants(&self, _name: &QualifiedTypeName) -> Option<Vec<Name>> {
        None // Opaque.
    }

    fn associated_type_bound(&self, _interface: &Interface, _assoc: Name) -> Vec<Interface> {
        Vec::new()
    }

    fn project(
        &self,
        _base: &Ty,
        _interface: &Interface,
        _member: &Name,
        _fuel: u32,
    ) -> baml_type::normalize::ProjectionStep {
        baml_type::normalize::ProjectionStep::Opaque // Opaque: re-entrant (→ the resolver).
    }
}
