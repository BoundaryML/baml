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

    /// Handle should not be passed to Jinja templates.
    #[error("Failed to convert value to Jinja value: {reason}")]
    ConversionError { reason: String },
}
