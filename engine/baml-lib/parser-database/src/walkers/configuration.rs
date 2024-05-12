use internal_baml_schema_ast::ast::{self, WithIdentifier, WithSpan};

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
        let is_legacy = self.walk_functions().any(|f| {
            f.is_old_function()
                && f.walk_variants()
                    .any(|v| v.properties().output_adapter.is_some())
        });

        if is_legacy {
            return false;
        }
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
