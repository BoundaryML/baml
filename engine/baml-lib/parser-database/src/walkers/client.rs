use crate::{
    ast::{self, WithName},
    types::ClientProperties,
};

/// A `function` declaration in the Prisma schema.
pub type ClientWalker<'db> = super::Walker<'db, ast::ClientId>;

impl<'db> ClientWalker<'db> {
    /// The name of the function.
    pub fn name(self) -> &'db str {
        self.ast_client().name()
    }

    /// The ID of the function in the db
    pub fn client_id(self) -> ast::ClientId {
        self.id
    }

    /// The AST node.
    pub fn ast_client(self) -> &'db ast::Client {
        &self.db.ast[self.id]
    }

    /// The properties of the variant.
    pub fn properties(self) -> &'db ClientProperties {
        &self.db.types.client_properties[&self.id]
    }
}
