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
}
