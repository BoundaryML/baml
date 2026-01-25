//! Stub types for IR compatibility during migration.
//!
//! This crate provides stub types that will be replaced by `bex_vm_types::Value`
//! once the VM-native render_prompt migration is complete.
//!
//! TODO: Remove this crate after Phase 6 of the render_prompt migration.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Type alias for BAML maps.
pub type BamlMap<K, V> = IndexMap<K, V>;

/// Jinja expression for predicate evaluation.
#[derive(Clone, Debug)]
pub struct JinjaExpression(pub String);

/// BAML value type (legacy, to be replaced by bex_vm_types::Value).
#[derive(Clone, Debug, PartialEq)]
pub enum BamlValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Map(BamlMap<String, BamlValue>),
    List(Vec<BamlValue>),
    Media(BamlMedia),
    Enum(String, String),
    Class(String, BamlMap<String, BamlValue>),
    Null,
}

impl serde::Serialize for BamlValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            BamlValue::String(s) => serializer.serialize_str(s),
            BamlValue::Int(i) => serializer.serialize_i64(*i),
            BamlValue::Float(f) => serializer.serialize_f64(*f),
            BamlValue::Bool(b) => serializer.serialize_bool(*b),
            BamlValue::Map(m) => m.serialize(serializer),
            BamlValue::List(l) => l.serialize(serializer),
            BamlValue::Media(m) => m.serialize(serializer),
            BamlValue::Enum(_, v) => serializer.serialize_str(v),
            BamlValue::Class(_, m) => m.serialize(serializer),
            BamlValue::Null => serializer.serialize_none(),
        }
    }
}

/// Media type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BamlMediaType {
    Image,
    Audio,
    Video,
    Pdf,
}

/// Media content representation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BamlMediaContent {
    #[serde(rename = "url")]
    Url(BamlMediaUrl),
    #[serde(rename = "base64")]
    Base64(BamlMediaBase64),
    #[serde(rename = "file")]
    File(BamlMediaFile),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BamlMediaUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BamlMediaBase64 {
    pub base64: String,
    pub media_type: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BamlMediaFile {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

/// Media value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BamlMedia {
    pub media_type: BamlMediaType,
    pub content: BamlMediaContent,
}
