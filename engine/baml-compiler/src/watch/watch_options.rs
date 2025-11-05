use internal_baml_ast::{self, ast::Identifier};
use internal_baml_diagnostics::Span;

/// The user-specified options for a watched variable.
#[derive(Clone, Debug, PartialEq)]
pub struct WatchSpec {
    pub when: WatchWhen,
    pub name: String,
    pub span: Span,
}

/// The user-specified option for when to auto-notify watchers for a variable.
#[derive(Clone, Debug, PartialEq)]
pub enum WatchWhen {
    Manual, // Manual notification only (via .$watch.notify())
    Auto,
    Never,
    FunctionName(Identifier),
}

impl WatchSpec {
    /// Create a default WatchSpec for a watched variable.
    /// Configuration will be provided via VAR_NAME.$watch.options() method calls.
    pub fn default_for_variable(variable_name: String, span: Span) -> Self {
        WatchSpec {
            when: WatchWhen::Auto,
            name: variable_name,
            span,
        }
    }
}
