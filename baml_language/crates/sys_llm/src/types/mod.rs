//! Types for LLM prompt rendering and error handling.

mod output_format;

pub use output_format::{Class, ClassField, Enum, EnumValue, OutputFormatContent, RenderOptions};
pub(crate) use output_format::{HoistClasses, MapStyle, RenderSetting};

/// Errors that can occur during LLM operations (render, specialize, `build_request`).
#[derive(Debug, thiserror::Error)]
pub enum LlmOpError {
    #[error("Expected {expected}, got {actual}")]
    TypeError {
        expected: &'static str,
        actual: String,
    },

    #[error("Render prompt error: {0}")]
    RenderPrompt(String),

    #[error("Parse response error: {0}")]
    ParseResponseError(String),

    #[error("{0}")]
    Other(String),

    #[error("Not implemented: {message}")]
    NotImplemented { message: String },
}
