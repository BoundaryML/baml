//! Generic parameters for functions and types.
//!
//! Following rust-analyzer's pattern, generic parameters are queried separately
//! from the `ItemTree` to maintain the invalidation barrier. Changes to generic
//! parameters don't invalidate the `ItemTree`.

use baml_base::Name;
use la_arena::{Arena, Idx};

/// Type parameter in a generic definition.
///
/// Example: `T` in `class Foo<T>`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeParam {
    pub name: Name,
    // Future: bounds, defaults, constraints
}

/// Local index for a type parameter within its `GenericParams`.
pub type LocalTypeParamId = Idx<TypeParam>;

/// Generic parameters for an item (function, class, enum, etc.).
///
/// This is queried separately from the `ItemTree` for incrementality.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GenericParams {
    /// Type parameters arena.
    pub type_params: Arena<TypeParam>,
    // Future: const parameters, lifetime parameters, where clauses
}

impl GenericParams {
    /// Create empty generic parameters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if there are any generic parameters.
    pub fn is_empty(&self) -> bool {
        self.type_params.is_empty()
    }

    /// Get all type parameter names.
    pub fn type_param_names(&self) -> impl Iterator<Item = &Name> {
        self.type_params.iter().map(|(_, p)| &p.name)
    }
}

impl std::ops::Index<LocalTypeParamId> for GenericParams {
    type Output = TypeParam;

    fn index(&self, index: LocalTypeParamId) -> &Self::Output {
        &self.type_params[index]
    }
}
