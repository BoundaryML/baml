//! Core BAML types for the runtime.
//!
//! This crate provides the fundamental types used throughout the BAML runtime:
//! - `BamlValue` - The runtime value type
//! - `BamlMap` - Type alias for IndexMap
//! - `BamlMedia` - Media content representation
//! - `TypeIR` - Type intermediate representation
//! - Constraint types for validation

mod baml_value;
mod completion;
mod constraint;
mod map;
mod media;
mod minijinja_expr;
mod type_ir;

pub use baml_value::{BamlValue, BamlValueWithMeta};
pub use completion::CompletionState;
pub use constraint::{Constraint, ConstraintLevel, ResponseCheck};
pub use map::BamlMap;
pub use media::{BamlMedia, BamlMediaContent, BamlMediaType};
pub use minijinja_expr::JinjaExpression;
pub use type_ir::{TypeIR, TypeValue, StreamingMode, LiteralValue};
