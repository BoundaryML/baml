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
//! Unlike the builder's `NormalizeCtx`, it holds no per-scope *inference* state
//! (`TypeInferenceBuilder`): type-level subtyping is a function of the program's
//! declarations plus the bounds a scope declares, never of the value-expression
//! inference the builder performs. `NormalizeCtx` delegates to it so the two
//! never disagree, and type-expression lowering — which has no builder —
//! constructs one directly.

use std::collections::HashMap;

use baml_base::Name;
use rustc_hash::FxHashMap;

use crate::{
    package_interface::PackageResolutionContext,
    ty::{QualifiedTypeName, Ty},
};

/// The type-variable bounds a [`GlobalTypeContext`] resolves, in whichever
/// representation the caller already holds — so neither the builder (which keeps
/// bounds as `Ty`) nor type-expression lowering (which keeps them as
/// [`baml_type::Interface`] constraints) has to round-trip through the other.
///
/// The two variants are the same concept in two encodings; they collapse into
/// one once the builder's bound side table is retyped to interface constraints.
pub(crate) enum TypeVarBounds<'a> {
    /// Bounds stored as a single lowered `Ty` per variable (the builder's
    /// representation).
    #[deprecated = "Legacy, lossy representation: a single lowered `Ty` per \
        variable cannot express an intersection bound (`A & B`) and silently \
        drops a non-interface bound (e.g. `T extends int`). The correct \
        representation is `Interfaces` — a conjunction of interface constraints; \
        retype the builder's `generic_param_bounds` side table and delete this."]
    Tys(&'a FxHashMap<Name, Ty>),
    /// Bounds lowered to interface constraints — the representation held by
    /// type-expression lowering, which resolves projections without a builder.
    /// The `Vec` is the conjunction of an intersection bound (`T: A & B`).
    Interfaces(&'a FxHashMap<Name, Vec<baml_type::Interface>>),
}

impl TypeVarBounds<'_> {
    /// The interface constraints bounding `name` — the conjunction of an
    /// intersection bound, or empty if `name` is unbounded, unknown, or bounded
    /// only by a non-interface type.
    fn interface_bounds(&self, name: &Name) -> Vec<baml_type::Interface> {
        match self {
            // The bridge must read the legacy representation until it is retired;
            // a single lowered `Ty` yields at most one interface constraint.
            #[expect(deprecated, reason = "bridging the legacy `Tys` representation")]
            Self::Tys(map) => map
                .get(name)
                .and_then(Ty::as_interface)
                .into_iter()
                .collect(),
            Self::Interfaces(map) => map.get(name).cloned().unwrap_or_default(),
        }
    }
}

/// The nominal facts [`baml_type::normalize`] needs, derived from global
/// information plus a scope's type-variable bounds. See the module docs.
pub(crate) struct GlobalTypeContext<'a, 'db> {
    pub(crate) db: &'db dyn crate::Db,
    pub(crate) res_ctx: &'db PackageResolutionContext<'db>,
    pub(crate) aliases: &'a HashMap<QualifiedTypeName, Ty>,
    pub(crate) bounds: TypeVarBounds<'a>,
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
        self.bounds.interface_bounds(name)
    }

    fn interface_requires(&self, sub: &baml_type::Interface, sup: &baml_type::Interface) -> bool {
        crate::interfaces::interface_requires(self.db, self.res_ctx, sub, sup, |a, b| {
            baml_type::normalize::equivalent(a, b, self)
        })
    }

    fn enum_variants(&self, name: &QualifiedTypeName) -> Option<Vec<Name>> {
        let variants = crate::inference::enum_variants(self.db, self.res_ctx, name);
        (!variants.is_empty()).then_some(variants)
    }
}
