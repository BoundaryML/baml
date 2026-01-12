//! Core codec traits for encoding/decoding BAML values.

use crate::{
    error::BamlError,
    proto::baml_cffi_v1::{
        host_map_entry, CffiValueClass, CffiValueHolder, HostMapEntry, HostValue,
    },
};

/// Trait for decoding from CFFI protobuf format (BAML -> Rust)
pub trait BamlDecode: Sized {
    /// Decode from a `CffiValueHolder` (outbound schema)
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError>;
}

/// Trait for encoding to CFFI protobuf format (Rust -> BAML)
pub trait BamlEncode {
    /// Encode to a `HostValue` (inbound schema)
    fn baml_encode(&self) -> HostValue;
}

pub trait BamlSerializeMapKey: Sized + std::hash::Hash + Eq {
    fn baml_encode_map_key(&self) -> host_map_entry::Key;
    fn baml_decode_map_key(key: &str) -> Result<Self, BamlError>;
}

/// Helper trait for decoding BAML classes
pub trait BamlClass: Sized {
    /// Expected BAML type name
    const TYPE_NAME: &'static str;

    /// Decode from class value
    fn from_class_value(class: &CffiValueClass) -> Result<Self, BamlError>;
}

/// Helper trait for decoding BAML enums
pub trait BamlEnum: Sized {
    /// Expected BAML enum name
    const ENUM_NAME: &'static str;

    /// Decode from string variant name
    fn from_variant_name(name: &str) -> Result<Self, BamlError>;
}

/// Trait for types that can be converted to method kwargs.
///
/// This allows ergonomic method calls without manually constructing
/// `HostMapEntry` vectors.
pub(crate) trait IntoKwargs {
    fn into_kwargs(self) -> Vec<HostMapEntry>;
}

/// Empty kwargs - for methods with no arguments
impl IntoKwargs for () {
    fn into_kwargs(self) -> Vec<HostMapEntry> {
        vec![]
    }
}

/// Pre-built kwargs vector passes through
impl IntoKwargs for Vec<HostMapEntry> {
    fn into_kwargs(self) -> Vec<HostMapEntry> {
        self
    }
}

/// Single kwarg from tuple
impl<V: BamlEncode> IntoKwargs for (&str, V) {
    fn into_kwargs(self) -> Vec<HostMapEntry> {
        vec![HostMapEntry {
            key: Some(host_map_entry::Key::StringKey(self.0.to_string())),
            value: Some(self.1.baml_encode()),
        }]
    }
}

/// Multiple kwargs from slice of tuples (up to reasonable sizes)
impl<V: BamlEncode + Clone> IntoKwargs for &[(&str, V)] {
    fn into_kwargs(self) -> Vec<HostMapEntry> {
        self.iter()
            .map(|(k, v)| HostMapEntry {
                key: Some(host_map_entry::Key::StringKey((*k).to_string())),
                value: Some(v.baml_encode()),
            })
            .collect()
    }
}

/// Two kwargs from tuple pair
impl<V1: BamlEncode, V2: BamlEncode> IntoKwargs for ((&str, V1), (&str, V2)) {
    fn into_kwargs(self) -> Vec<HostMapEntry> {
        vec![
            HostMapEntry {
                key: Some(host_map_entry::Key::StringKey(self.0 .0.to_string())),
                value: Some(self.0 .1.baml_encode()),
            },
            HostMapEntry {
                key: Some(host_map_entry::Key::StringKey(self.1 .0.to_string())),
                value: Some(self.1 .1.baml_encode()),
            },
        ]
    }
}

pub(crate) trait DecodeHandle: Sized {
    fn decode_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const std::ffi::c_void,
    ) -> Result<Self, BamlError>;
}
