// ============================================================================
// Name Resolution Errors
// ============================================================================

use baml_base::Span;

/// Name resolution errors that can occur during compilation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NameError {
    /// Duplicate definition of a name in the same namespace.
    DuplicateName {
        name: String,
        kind: &'static str,
        first: Span,
        first_path: String,
        second: Span,
        second_path: String,
    },

    /// Duplicate test targeting the same function.
    ///
    /// Tests with the same name are allowed if they target different functions,
    /// but two tests with the same name cannot target the same function.
    DuplicateTestForFunction {
        test_name: String,
        function_name: String,
        first: Span,
        first_path: String,
        second: Span,
        second_path: String,
    },

    /// Unknown function named in test block.
    ///
    /// Tests must reference functions or template strings defined in the project.
    UnknownFunctionInTest { function_name: String, span: Span },
}
