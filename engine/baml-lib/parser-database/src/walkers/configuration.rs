use internal_baml_schema_ast::ast::{self, WithIdentifier};

use crate::types::{PrinterType, RetryPolicy, TestCase};

/// A `class` declaration in the Prisma schema.
pub type ConfigurationWalker<'db> = super::Walker<'db, (ast::ConfigurationId, &'static str)>;

impl ConfigurationWalker<'_> {
    /// Get the AST node for this class.
    pub fn ast_node(&self) -> &ast::Configuration {
        &self.db.ast[self.id.0]
    }

    /// Get the actual configuration for this class.
    pub fn retry_policy(&self) -> &RetryPolicy {
        assert!(self.id.1 == "retry_policy");
        &self.db.types.retry_policies[&self.id.0]
    }

    /// Get as a printer configuration.
    pub fn printer(&self) -> &PrinterType {
        assert!(self.id.1 == "printer");
        &self.db.types.printers[&self.id.0]
    }

    /// Get as a test case configuration.
    pub fn test_case(&self) -> &TestCase {
        assert!(self.id.1 == "test_case");
        &self.db.types.test_cases[&self.id.0]
    }

    /// Get the function that this test case is testing.
    pub fn walk_function(&self) -> super::FunctionWalker<'_> {
        assert!(self.id.1 == "test_case");
        self.db
            .find_function_by_name(&self.test_case().function.0)
            .unwrap()
    }
}

impl WithIdentifier for ConfigurationWalker<'_> {
    fn identifier(&self) -> &ast::Identifier {
        self.ast_node().identifier()
    }
}
