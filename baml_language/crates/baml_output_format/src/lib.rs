//! Output format types and rendering for BAML schemas.
//!
//! This crate provides:
//! - `OutputFormatContent` - Container for all type schemas
//! - `OutputFormatOptions` - Options for rendering output format
//! - `render` - Render output format to a string

mod render;
mod render_options;
mod types;

pub use render::{render, RenderError};
pub use render_options::{HoistClasses, MapStyle, OutputFormatOptions, RenderSetting, INLINE_RENDER_ENUM_MAX_VALUES};
pub use types::{Class, ClassField, Enum, EnumVariant, Name, OutputFormatBuilder, OutputFormatContent};
