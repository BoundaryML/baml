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
//! - [`KnownTypes`] - Marker trait for CodeGen'd type enums

mod traits;
mod primitives;
mod containers;
mod helpers;
mod known_types;
mod baml_value;
mod dynamic_types;
mod from_baml_value;
mod from_baml_value_ref;

// Re-export all public items
pub use traits::{BamlDecode, BamlEncode, BamlClass, BamlEnum, IntoKwargs};
pub use helpers::{decode_enum, decode_field, decode_optional_field, encode_class, encode_enum};
pub use known_types::KnownTypes;
pub use baml_value::BamlValue;
pub use dynamic_types::{DynamicClass, DynamicEnum, DynamicUnion};
pub use from_baml_value::FromBamlValue;
pub use from_baml_value_ref::FromBamlValueRef;

// Re-export protobuf types needed by generated code
pub use crate::proto::baml_cffi_v1::{
    CffiMapEntry, CffiValueClass, CffiValueHolder, HostMapEntry, HostValue,
};
