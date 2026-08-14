//! Types for LLM prompt rendering and error handling.

mod output_format;
mod sap;

pub use output_format::OutputFormatContent;
pub(crate) use output_format::{
    Class, ClassField, Enum, EnumValue, HoistClasses, MapStyle, RenderOptions, RenderSetting,
    is_text_or_image_union,
};
pub use sap::SapParseCache;

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

    #[error("Jsonish error: {0}")]
    JsonishError(::bex_sap::jsonish::JsonishError),

    #[error("SAP error: {0}")]
    SapError(::bex_sap::deserializer::coercer::ParsingError),

    #[error("{0}")]
    Other(String),

    #[error("Not implemented: {message}")]
    NotImplemented { message: String },
}

impl From<LlmOpError> for ::sys_types::VmRustFnError {
    fn from(e: LlmOpError) -> Self {
        let baml: ::sys_types::VmBamlError = match e {
            LlmOpError::TypeError { expected, actual } => {
                ::sys_types::VmBamlError::InvalidArgument {
                    message: format!("expected {expected}, got {actual}"),
                }
            }
            LlmOpError::RenderPrompt(msg) => {
                ::sys_types::VmBamlError::RenderPrompt { message: msg }
            }
            LlmOpError::Other(msg) => ::sys_types::VmBamlError::DevOther { message: msg },
            LlmOpError::ParseResponseError(e) => ::sys_types::VmBamlError::LlmClient { message: e },
            LlmOpError::JsonishError(e) => ::sys_types::VmBamlError::LlmClient {
                message: e.to_string(),
            },
            LlmOpError::SapError(e) => ::sys_types::VmBamlError::LlmClient {
                message: e.to_string(),
            },
            LlmOpError::NotImplemented { message } => {
                ::sys_types::VmBamlError::NotImplemented { message }
            }
        };
        baml.into()
    }
}
