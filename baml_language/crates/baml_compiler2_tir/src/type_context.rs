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
