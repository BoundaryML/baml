use internal_baml_schema_ast::ast::{self, WithIdentifier, WithSpan};

use crate::types::{RetryPolicy, TestCase};

/// A `class` declaration in the Prisma schema.
pub type ConfigurationWalker<'db> = super::Walker<'db, (ast::ValExpId, &'static str)>;

impl ConfigurationWalker<'_> {
    /// Get the AST node for this class.
    pub fn ast_node(&self) -> &ast::ValueExprBlock {
        &self.db.ast[self.id.0]
    }

    /// Get the actual configuration for this class.
    pub fn retry_policy(&self) -> &RetryPolicy {
        assert!(self.id.1 == "retry_policy");
        &self.db.types.retry_policies[&self.id.0]
    }

    /// Get as a test case configuration.
    pub fn test_case(&self) -> &TestCase {
        assert!(self.id.1 == "test");
        &self.db.types.test_cases[&self.id.0]
    }

    /// Get the function that this test case is testing.
    pub fn walk_functions(&self) -> impl Iterator<Item = super::FunctionWalker<'_>> {
        assert!(self.id.1 == "test");
        self.test_case()
            .functions
            .iter()
            .filter_map(|id| self.db.find_function_by_name(id.0.as_str()))
    }

    /// If adapters are not present we can stream
    pub fn is_streaming_supported(&self) -> bool {
        return true;
    }
}

impl WithIdentifier for ConfigurationWalker<'_> {
    fn identifier(&self) -> &ast::Identifier {
        self.ast_node().identifier()
    }
}

impl<'db> WithSpan for ConfigurationWalker<'db> {
    fn span(&self) -> &internal_baml_diagnostics::Span {
        self.ast_node().span()
    }
}
