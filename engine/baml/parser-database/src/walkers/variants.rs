use crate::ast::{self, WithName};

use super::{ClientWalker, FunctionWalker, Walker};

/// A `function` declaration in the Prisma schema.
pub type FunctionImplWalker<'db> = Walker<'db, (ast::FunctionId, ast::VariantConfigId)>;

impl<'db> FunctionImplWalker<'db> {
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
        self.id.1
    }

    /// Helper to access every client.
    pub fn client(self) -> ClientWalker<'db> {
        let client_name = self.ast_variant().default_client().unwrap();
        match self.db.find_client(client_name) {
            Some(client) => client,
            None => panic!("Client {} not found", client_name),
        }
    }

    /// The ID of the function in the db
    pub fn function_id(self) -> ast::FunctionId {
        self.id.0
    }

    /// The function node.
    pub fn walk_function(self) -> FunctionWalker<'db> {
        Walker {
            db: &self.db,
            id: self.id.0,
        }
    }

    /// The AST node.
    pub fn ast_variant(self) -> &'db ast::Variant {
        &self.db.ast[self.id.1]
    }
}
