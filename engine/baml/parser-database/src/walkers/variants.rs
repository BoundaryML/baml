use crate::{
    ast::{self, WithName},
    types::VariantProperties,
};

use super::{ClientWalker, FunctionWalker, Walker};

/// A `function` declaration in the Prisma schema.
pub type VariantWalker<'db> = Walker<'db, ast::VariantConfigId>;

impl<'db> VariantWalker<'db> {
    /// The name of the function.
    pub fn name(self) -> &'db str {
        self.ast_variant().name()
    }

    /// The name of the function.
    pub fn function_name(self) -> &'db str {
        self.ast_variant().function_name()
    }

    /// The ID of the function in the db
    pub fn variant_id(self) -> ast::VariantConfigId {
        self.id
    }

    /// Helper to access every client.
    pub fn client(self) -> Option<ClientWalker<'db>> {
        self.db
            .types
            .variant_properties
            .get(&self.id)
            .and_then(|f| self.db.find_client(&f.client))
    }

    /// The function node.
    pub fn walk_function(self) -> Option<FunctionWalker<'db>> {
        self.db.find_function(self.function_name())
    }

    /// The AST node.
    pub fn ast_variant(self) -> &'db ast::Variant {
        &self.db.ast[self.id]
    }

    /// The properties of the variant.
    pub fn properties(self) -> &'db VariantProperties {
        &self.db.types.variant_properties[&self.id]
    }
}
