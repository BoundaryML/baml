//! Error types for the Jinja runtime.

use thiserror::Error;

/// Errors that can occur during prompt rendering.
#[derive(Debug, Error)]
pub enum RenderPromptError {
    /// Template syntax or rendering error from minijinja.
    #[error("Template error: {0}")]
    TemplateError(#[from] minijinja::Error),

    /// Missing or invalid template variable.
    #[error("Missing variable: {name}")]
    MissingVariable { name: String },

    /// Value could not be converted to a Jinja-compatible form (e.g. Handle or unsupported type).
    #[error("Failed to convert value to Jinja value: {reason}")]
    ConversionError { reason: String },
}
