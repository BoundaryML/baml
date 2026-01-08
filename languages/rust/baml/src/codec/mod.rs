//! # BAML Codec
//!
//! This module provides types and traits for encoding/decoding BAML values.
//!
//! ## Core Traits
//!
//! - [`BamlDecode`] - Decode from CFFI protobuf format
//! - [`BamlEncode`] - Encode to CFFI protobuf format
//! - [`BamlClass`] - Helper for decoding BAML classes
//! - [`BamlEnum`] - Helper for decoding BAML enums
//!
//! ## Dynamic Types
//!
//! - [`BamlValue`] - A dynamically-typed BAML value
//! - [`DynamicClass`] - A fully dynamic class with string-keyed fields
//! - [`DynamicEnum`] - A fully dynamic enum with name and value strings
//! - [`DynamicUnion`] - A dynamic union with variant metadata
//!
//! ## Conversion Traits
//!
//! - [`FromBamlValue`] - Extract concrete types from `BamlValue`
//! - [`FromBamlValueRef`] - Borrow concrete types from `BamlValue` (zero-copy)
//! - [`KnownTypes`] - Marker trait for `CodeGen`'d type enums

mod baml_value;
mod containers;
mod dynamic_types;
mod from_baml_value;
mod from_baml_value_ref;
mod helpers;
mod known_types;
mod primitives;
pub(crate) mod traits;

// Re-export all public items
pub use baml_value::BamlValue;
pub use dynamic_types::{DynamicClass, DynamicEnum, DynamicUnion};
pub use from_baml_value::FromBamlValue;
pub use from_baml_value_ref::FromBamlValueRef;
pub use helpers::{decode_enum, decode_field, encode_class, encode_class_dynamic, encode_enum};
pub use known_types::KnownTypes;
pub use traits::{BamlClass, BamlDecode, BamlEncode, BamlEnum, BamlSerializeMapKey};

// Re-export protobuf types needed by generated code
