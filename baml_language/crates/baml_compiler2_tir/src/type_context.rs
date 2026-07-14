//! The builder-free global type context.
//!
//! [`baml_type::normalize`] resolves subtyping and equality structurally, but
//! needs a handful of *nominal* facts it cannot derive on its own: what an alias
//! expands to, whether a concrete type implements an interface, a type
//! variable's bound, the `requires` relation between interfaces, and an enum's
//! variants. [`GlobalTypeContext`] supplies them from globally-available
//! information alone — the salsa database, the package resolution context (which
//! bounds package visibility), the package's alias map, and the type-variable
//! bounds the enclosing scope introduces.
//!
//! Unlike the builder's `TypeContext` impl, it holds no per-scope *inference* state
//! (`TypeInferenceBuilder`): type-level subtyping is a function of the program's
//! declarations plus the bounds a scope declares, never of the value-expression
//! inference the builder performs. The builder delegates to it so the two
//! never disagree, and type-expression lowering — which has no builder —
//! constructs one directly.

use std::collections::HashMap;

use baml_base::Name;

use crate::{
    package_interface::PackageResolutionContext,
    ty::{QualifiedTypeName, Ty},
};

/// The nominal facts [`baml_type::normalize`] needs, derived from global
/// information plus a scope's type-variable bounds. See the module docs.
///
/// Public so downstream consumers (MIR) can run the canonical algebra
/// (`normalize`, `equivalent`, projection reduction) against the same facts the
/// checker uses, instead of a parallel resolver — build one from a package's
/// [`PackageResolutionContext`], its alias map, and the relevant type-variable
/// bounds, then pass `&ctx` to the `baml_type::normalize` entry points.
pub struct GlobalTypeContext<'a, 'db> {
    pub db: &'db dyn crate::Db,
    pub res_ctx: &'db PackageResolutionContext<'db>,
    pub aliases: &'a HashMap<QualifiedTypeName, Ty>,
    /// A scope's type-variable bounds as interface-constraint conjunctions
    /// (`T: A & B`). The single representation both the builder and
    /// type-expression lowering hold.
    pub bounds: &'a crate::lower_type_expr::TypeVarBoundsMap,
}

impl baml_type::normalize::TypeContext for GlobalTypeContext<'_, '_> {
    fn alias_def(&self, name: &QualifiedTypeName) -> Option<Ty> {
        crate::inference::alias_def(self.db, name)
    }

    fn implements_interface(&self, concrete: &Ty, interface: &baml_type::Interface) -> bool {
        // Bound obligations discharge through the *same* canonical algebra
        // (driven by this context), not a parallel oracle — the realized path
        // proves them inside L1, so this `is_subtype` recursion is reached only
        // by the symbolic fallback.
        crate::interfaces::implements_interface(
            self.db,
            concrete,
            interface,
            self.aliases,
            |actual, bound| baml_type::normalize::is_subtype(actual, bound, self),
        )
    }

    fn type_var_bound(&self, name: &Name) -> Vec<baml_type::Interface> {
        // The conjunction bounding `name` (`T: A & B`), or empty if unbounded/unknown.
        self.bounds.get(name).cloned().unwrap_or_default()
    }

    fn interface_requires(&self, sub: &baml_type::Interface, sup: &baml_type::Interface) -> bool {
        crate::interfaces::interface_requires(self.db, self.res_ctx, sub, sup, |a, b| {
            baml_type::normalize::equivalent(a, b, self)
        })
    }

    fn enum_variants(&self, name: &QualifiedTypeName) -> Option<Vec<Name>> {
        crate::inference::enum_variants(self.db, self.res_ctx, name)
    }

    fn associated_type_bound(
        &self,
        interface: &baml_type::Interface,
        assoc: Name,
    ) -> Vec<baml_type::Interface> {
        crate::builder::associated_projection::associated_type_declared_bound(
            self.db, interface, &assoc,
        )
    }

    fn project(
        &self,
        base: &Ty,
        interface: &baml_type::Interface,
        member: &Name,
        // Single-step reducer: each arm returns one `Reduced`/`Opaque` result, and
        // the caller (`NormalTy::from_ty`) decrements its own fuel across the chain,
        // so there is no self-recursion here to bound.
        _fuel: u32,
    ) -> baml_type::normalize::ProjectionStep {
        use baml_type::normalize::ProjectionStep;
        // The qualifier already pins the member — that pin *is* the projection.
        if let Some((_, pin)) = interface
            .associated_types
            .iter()
            .find(|(name, _)| name == member)
        {
            return ProjectionStep::Reduced(pin.clone());
        }
        // A type variable projects through its own interface bound: `(P as Parser).Output`
        // with `P extends Parser<Output = int>` reduces to `int`, because the bound pins the
        // member. Match the qualifier against a carried bound by head — name and generic
        // args — and read its pin for `member`. (A bound reached only through the qualifier's
        // `requires` closure stays opaque here; the direct bound is the common case.)
        if let Ty::TypeVar(name, _) = base {
            for have in self.type_var_bound(name) {
                if have.name == interface.name
                    && have.generics.len() == interface.generics.len()
                    && have
                        .generics
                        .iter()
                        .zip(&interface.generics)
                        .all(|(h, i)| self.equivalent(h, i))
                    && let Some((_, pin)) = have.associated_types.iter().find(|(n, _)| n == member)
                {
                    return ProjectionStep::Reduced(pin.clone());
                }
            }
        }
        // An interface *existential* base fixes an omitted, defaulted associated type to its
        // default: `Boxed<string>` (with `type Item = T`) has `Item = string`, so
        // `(Boxed<string> as Boxed).Item` reduces to `string`. An explicit pin on the base is
        // used directly; otherwise the interface's own default is realized at the base's args,
        // with `Self` = the base so a Self-referencing default (`type Items = Self.Item[]`)
        // resolves against the base's pins. (A *bound* — the type-var arm above — never fills a
        // default, because its implementor may override it.)
        if let Ty::Interface(qtn, args, pins, _) = base {
            if let Some((_, ty)) = pins.iter().find(|(name, _)| name == member) {
                return ProjectionStep::Reduced(ty.clone());
            }
            if let Some(default) = crate::interfaces::existential_associated_default(
                self.db,
                self.res_ctx,
                qtn,
                args,
                base,
                member,
            ) {
                return ProjectionStep::Reduced(default);
            }
        }
        // A concrete-headed base determines the member through its `implements` block for
        // the written qualifier interface — `(int as Foo).Assoc` is int's `type Assoc = …`,
        // read off the realized interface the impl provides. Rigid type variables in the
        // base (`(Map<T, R> as Iterator).Item`) resolve through the impl pattern match,
        // their bounds judged against this scope's `bounds`; an unmatched symbolic base
        // stays opaque.
        if let Some(realized) =
            crate::builder::associated_projection::resolve_concrete_realized_interface(
                self.db,
                &self.res_ctx.own_package_name,
                self.bounds,
                base,
                interface,
            )
            && let Some((_, ty)) = realized
                .associated_types
                .iter()
                .find(|(name, _)| name == member)
        {
            return ProjectionStep::Reduced(ty.clone());
        }
        ProjectionStep::Opaque
    }
}

/// A [`TypeContext`] for structural type **equivalence** that expands aliases but
/// leaves every *nominal* fact opaque: no enum-completeness collapse, no interface
/// membership or `requires`, no type-variable bounds, no associated-type bounds,
/// and no projection reduction.
///
/// This is the context for the impl-head *matcher* ([`crate::interfaces::match_ty_patterns`]),
/// coherence unification, and MIR dispatch matching — every site that asks "do these
/// two already-lowered impl-head / dispatch shapes denote the same type?". It is
/// deliberately fact-poor for two reasons:
///
/// 1. **Termination.** Resolving a projection or an interface membership here would
///    re-enter impl resolution (`project`/`implements_interface` →
///    `get_implements_block` → the matcher → here) with no bound — the matcher is
///    itself a link in that chain. Union-member absorption inside
///    [`baml_type::normalize`]'s canonicalizer reaches `implements_interface`, so a
///    context that answered it would loop, not merely mis-answer.
/// 2. **Sufficiency.** Invariant equality needs none of it. An unreduced projection
///    or an unabsorbed `C | dyn I` is a faithful *opaque leaf* for equality — two
///    such spellings are equal iff structurally equal, which is exactly the
///    conservative answer these sites want (fewer coincidental equalities ⇒
///    fail-closed coherence, fewer over-eager dispatch matches).
///
/// Alias expansion *is* supplied, because two spellings that differ only by an alias
/// (`type BI = Box<int>` vs `Box<int>`) genuinely denote the same type. Recursive
/// aliases are handled by the canonicalizer's own μ-folding (an alias re-encountered
/// mid-expansion becomes a recursion variable), so no precomputed recursive-alias set
/// is needed here.
///
/// The result is *canonical structural* equivalence: it applies the set-theoretic
/// simplifications that hold regardless of nominal facts (`never` removal,
/// `1 | int == int`, `unknown` absorption, invariant container recursion) while
/// treating enums, interfaces, type variables, and projections as opaque leaves.
pub struct AliasEquivCtx<'a>(pub &'a HashMap<QualifiedTypeName, Ty>);

impl baml_type::normalize::TypeContext for AliasEquivCtx<'_> {
    fn alias_def(&self, name: &QualifiedTypeName) -> Option<Ty> {
        self.0.get(name).cloned()
    }

    fn implements_interface(&self, _concrete: &Ty, _interface: &baml_type::Interface) -> bool {
        // Opaque: an interface-membership `C | dyn I == dyn I` absorption is not
        // performed. Leaving it unabsorbed is the conservative answer for equality.
        false
    }

    fn type_var_bound(&self, _name: &Name) -> Vec<baml_type::Interface> {
        // Opaque: a type variable is only equal to itself here; its bound never
        // licenses an absorption.
        Vec::new()
    }

    fn interface_requires(&self, _sub: &baml_type::Interface, _sup: &baml_type::Interface) -> bool {
        // Opaque: `A | B == B` via `A requires B` is not performed.
        false
    }

    fn enum_variants(&self, _name: &QualifiedTypeName) -> Option<Vec<Name>> {
        // Opaque: `E.A | E.B | … == E` completeness collapse is not performed.
        None
    }

    fn associated_type_bound(
        &self,
        _interface: &baml_type::Interface,
        _assoc: Name,
    ) -> Vec<baml_type::Interface> {
        // Opaque: a still-symbolic projection carries no bound-derived membership here.
        Vec::new()
    }

    fn project(
        &self,
        _base: &Ty,
        _interface: &baml_type::Interface,
        _member: &Name,
        _fuel: u32,
    ) -> baml_type::normalize::ProjectionStep {
        // Opaque: reducing here would re-enter impl resolution unboundedly (see the
        // type-level doc). An unreduced projection is a faithful leaf for equality.
        baml_type::normalize::ProjectionStep::Opaque
    }
}
