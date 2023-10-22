use crate::ast::{self, WithName};

use super::{field::FieldWalker, Walker};

/// A `function` declaration in the Prisma schema.
pub type FunctionWalker<'db> = super::Walker<'db, ast::FunctionId>;

impl<'db> FunctionWalker<'db> {
    /// The name of the function.
    pub fn name(self) -> &'db str {
        self.ast_function().name()
    }

    /// The ID of the function in the db
    pub fn function_id(self) -> ast::FunctionId {
        self.id
    }

    /// The AST node.
    pub fn ast_function(self) -> &'db ast::Function {
        &self.db.ast[self.id]
    }

    // The input to the function.
    // pub fn input(self) -> &'db ast::Field {
    //     &self.ast_function().input()
    // }

    // The output of the function.
    // pub fn output(self) -> &'db ast::Field {
    //     &self.ast_function().output()
    // }
}
