use internal_baml_schema_ast::ast::{self, WithIdentifier};

use crate::types::RetryPolicy;

/// A `class` declaration in the Prisma schema.
pub type RetryPolicyWalker<'db> = super::Walker<'db, ast::ConfigurationId>;

impl RetryPolicyWalker<'_> {
    /// Get the AST node for this class.
    pub fn ast_node(&self) -> &ast::Configuration {
        &self.db.ast[self.id]
    }

    /// Get the actual configuration for this class.
    pub fn config(&self) -> &RetryPolicy {
        &self.db.types.retry_policies[&self.id]
    }
}

impl WithIdentifier for RetryPolicyWalker<'_> {
    fn identifier(&self) -> &ast::Identifier {
        self.ast_node().identifier()
    }
}
