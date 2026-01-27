//! Types for LLM prompt rendering in BAML.
//!
//! This crate contains:
//! - `OutputFormatContent` - Schema for LLM output format rendering
//! - `Enum`, `Class` - Simplified type definitions
//! - `RenderOptions` - Configuration for output format rendering
//! - `RenderError` - Error type for rendering failures

mod output_format;

pub use output_format::{
    Class, Enum, HoistClasses, MapStyle, OutputFormatContent, RenderError, RenderOptions,
    RenderSetting,
};
