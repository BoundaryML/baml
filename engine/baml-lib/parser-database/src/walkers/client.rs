use crate::{
    ast::{self, WithIdentifier},
    types::ClientProperties,
};

/// A `function` declaration in the Prisma schema.
pub type ClientWalker<'db> = super::Walker<'db, ast::ClientId>;

impl<'db> ClientWalker<'db> {
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

// with identifier
impl<'db> WithIdentifier for ClientWalker<'db> {
    fn identifier(&self) -> &ast::Identifier {
        self.ast_client().identifier()
    }
}
