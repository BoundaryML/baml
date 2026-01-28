//! Jinja template runtime for BAML prompts.
//!
//! This crate provides:
//! - `render_prompt()` - Main entry point for rendering templates
//! - `OutputFormatObject` - Template-accessible output format renderer
//! - Value conversion from `BexExternalValue` to minijinja values
//! - Magic delimiter handling for chat messages and media

mod error;
mod filters;
mod output_format_object;
mod render;
mod value_conversion;

pub use bex_llm_types::{OutputFormatContent, RenderOptions};
pub use error::RenderPromptError;
pub use output_format_object::OutputFormatObject;
pub use render::{RenderContext, RenderContextClient, render_prompt};

/// Magic delimiter for chat role markers.
pub const MAGIC_CHAT_ROLE_DELIMITER: &str = "BAML_CHAT_ROLE_MAGIC_STRING_DELIMITER";

/// Magic delimiter for media content.
pub const MAGIC_MEDIA_DELIMITER: &str = "BAML_MEDIA_MAGIC_STRING_DELIMITER";
