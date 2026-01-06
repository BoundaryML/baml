//! BamlValue - the runtime value type.

use std::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Visitor;
use crate::{BamlMap, BamlMedia, BamlMediaType};

/// The core runtime value type in BAML.
///
/// BamlValue represents all possible values that can flow through the BAML runtime,
/// including primitives, collections, media, and typed values (enums/classes).
#[derive(Clone, Debug, PartialEq)]
pub enum BamlValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Map(BamlMap<String, BamlValue>),
    List(Vec<BamlValue>),
    Media(BamlMedia),
    Enum(String, String), // (enum_name, variant_name)
    Class(String, BamlMap<String, BamlValue>), // (class_name, fields)
    Null,
}

impl BamlValue {
    /// Get a string describing this value's type.
    pub fn r#type(&self) -> String {
        match self {
            BamlValue::String(_) => "string".into(),
            BamlValue::Int(_) => "int".into(),
            BamlValue::Float(_) => "float".into(),
            BamlValue::Bool(_) => "bool".into(),
            BamlValue::Map(kv) => {
                let value_types: std::collections::HashSet<_> =
                    kv.values().map(|v| v.r#type()).collect();
                let types_str: Vec<_> = value_types.into_iter().collect();
                if types_str.is_empty() {
                    "map<string, ?>".into()
                } else {
                    format!("map<string, {}>", types_str.join(" | "))
                }
            }
            BamlValue::List(k) => {
                let value_types: std::collections::HashSet<_> =
                    k.iter().map(|v| v.r#type()).collect();
                let types_str: Vec<_> = value_types.into_iter().collect();
                if types_str.is_empty() {
                    "list<?>".into()
                } else {
                    format!("list<{}>", types_str.join(" | "))
                }
            }
            BamlValue::Media(m) => match m.media_type {
                BamlMediaType::Image => "image",
                BamlMediaType::Audio => "audio",
                BamlMediaType::Pdf => "pdf",
                BamlMediaType::Video => "video",
            }
            .into(),
            BamlValue::Enum(e, _) => format!("enum {e}"),
            BamlValue::Class(c, _) => format!("class {c}"),
            BamlValue::Null => "null".into(),
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            BamlValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            BamlValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            BamlValue::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            BamlValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, BamlValue::Null)
    }

    pub fn is_map(&self) -> bool {
        matches!(self, BamlValue::Map(_))
    }

    pub fn as_map(&self) -> Option<&BamlMap<String, BamlValue>> {
        match self {
            BamlValue::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_map_owned(self) -> Option<BamlMap<String, BamlValue>> {
        match self {
            BamlValue::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&Vec<BamlValue>> {
        match self {
            BamlValue::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_list_owned(self) -> Option<Vec<BamlValue>> {
        match self {
            BamlValue::List(vals) => Some(vals),
            _ => None,
        }
    }
}

impl fmt::Display for BamlValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BamlValue::String(s) => write!(f, "\"{}\"", s),
            BamlValue::Int(i) => write!(f, "{}", i),
            BamlValue::Float(fl) => write!(f, "{}", fl),
            BamlValue::Bool(b) => write!(f, "{}", b),
            BamlValue::Null => write!(f, "null"),
            BamlValue::Map(m) => write!(f, "{:?}", m),
            BamlValue::List(l) => write!(f, "{:?}", l),
            BamlValue::Media(m) => write!(f, "<{}>", m.media_type),
            BamlValue::Enum(e, v) => write!(f, "{}::{}", e, v),
            BamlValue::Class(c, fields) => write!(f, "{} {{ {:?} }}", c, fields),
        }
    }
}

// Conversion traits
impl From<String> for BamlValue {
    fn from(s: String) -> Self {
        BamlValue::String(s)
    }
}

impl From<&str> for BamlValue {
    fn from(s: &str) -> Self {
        BamlValue::String(s.to_string())
    }
}

impl From<i64> for BamlValue {
    fn from(i: i64) -> Self {
        BamlValue::Int(i)
    }
}

impl From<i32> for BamlValue {
    fn from(i: i32) -> Self {
        BamlValue::Int(i as i64)
    }
}

impl From<f64> for BamlValue {
    fn from(f: f64) -> Self {
        BamlValue::Float(f)
    }
}

impl From<bool> for BamlValue {
    fn from(b: bool) -> Self {
        BamlValue::Bool(b)
    }
}

impl From<Vec<BamlValue>> for BamlValue {
    fn from(v: Vec<BamlValue>) -> Self {
        BamlValue::List(v)
    }
}

impl From<BamlMap<String, BamlValue>> for BamlValue {
    fn from(m: BamlMap<String, BamlValue>) -> Self {
        BamlValue::Map(m)
    }
}

impl From<BamlMedia> for BamlValue {
    fn from(m: BamlMedia) -> Self {
        BamlValue::Media(m)
    }
}

impl TryFrom<serde_json::Value> for BamlValue {
    type Error = serde_json::Error;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        serde_json::from_value(value)
    }
}

// Serialize implementation
impl Serialize for BamlValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
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

// Deserialize implementation
impl<'de> Deserialize<'de> for BamlValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(BamlValueVisitor)
    }
}

struct BamlValueVisitor;

impl<'de> Visitor<'de> for BamlValueVisitor {
    type Value = BamlValue;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a valid BamlValue")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::String(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Int(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Int(value as i64))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Float(value))
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Bool(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Null)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element()? {
            values.push(value);
        }
        Ok(BamlValue::List(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut values = BamlMap::new();
        while let Some((key, value)) = map.next_entry()? {
            values.insert(key, value);
        }
        Ok(BamlValue::Map(values))
    }
}

/// A BamlValue with associated metadata.
///
/// This is a generic wrapper that allows attaching metadata to values
/// for tracking parsing flags, constraint results, etc.
#[derive(Clone, Debug, PartialEq)]
pub enum BamlValueWithMeta<M> {
    String(String, M),
    Int(i64, M),
    Float(f64, M),
    Bool(bool, M),
    Map(BamlMap<String, BamlValueWithMeta<M>>, M),
    List(Vec<BamlValueWithMeta<M>>, M),
    Media(BamlMedia, M),
    Enum(String, String, M), // (enum_name, variant_name, meta)
    Class(String, BamlMap<String, BamlValueWithMeta<M>>, M), // (class_name, fields, meta)
    Null(M),
}

impl<M> BamlValueWithMeta<M> {
    /// Get the metadata for this value.
    pub fn meta(&self) -> &M {
        match self {
            BamlValueWithMeta::String(_, m) => m,
            BamlValueWithMeta::Int(_, m) => m,
            BamlValueWithMeta::Float(_, m) => m,
            BamlValueWithMeta::Bool(_, m) => m,
            BamlValueWithMeta::Map(_, m) => m,
            BamlValueWithMeta::List(_, m) => m,
            BamlValueWithMeta::Media(_, m) => m,
            BamlValueWithMeta::Enum(_, _, m) => m,
            BamlValueWithMeta::Class(_, _, m) => m,
            BamlValueWithMeta::Null(m) => m,
        }
    }

    /// Convert to BamlValue, discarding metadata.
    pub fn into_value(self) -> BamlValue {
        match self {
            BamlValueWithMeta::String(s, _) => BamlValue::String(s),
            BamlValueWithMeta::Int(i, _) => BamlValue::Int(i),
            BamlValueWithMeta::Float(f, _) => BamlValue::Float(f),
            BamlValueWithMeta::Bool(b, _) => BamlValue::Bool(b),
            BamlValueWithMeta::Map(m, _) => {
                BamlValue::Map(m.into_iter().map(|(k, v)| (k, v.into_value())).collect())
            }
            BamlValueWithMeta::List(l, _) => {
                BamlValue::List(l.into_iter().map(|v| v.into_value()).collect())
            }
            BamlValueWithMeta::Media(m, _) => BamlValue::Media(m),
            BamlValueWithMeta::Enum(e, v, _) => BamlValue::Enum(e, v),
            BamlValueWithMeta::Class(c, fields, _) => {
                BamlValue::Class(c, fields.into_iter().map(|(k, v)| (k, v.into_value())).collect())
            }
            BamlValueWithMeta::Null(_) => BamlValue::Null,
        }
    }
}

impl<M> From<BamlValueWithMeta<M>> for BamlValue {
    fn from(v: BamlValueWithMeta<M>) -> Self {
        v.into_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_baml_value_type() {
        assert_eq!(BamlValue::String("hello".into()).r#type(), "string");
        assert_eq!(BamlValue::Int(42).r#type(), "int");
        assert_eq!(BamlValue::Float(3.14).r#type(), "float");
        assert_eq!(BamlValue::Bool(true).r#type(), "bool");
        assert_eq!(BamlValue::Null.r#type(), "null");
    }

    #[test]
    fn test_baml_value_conversions() {
        let s: BamlValue = "hello".into();
        assert!(matches!(s, BamlValue::String(_)));

        let i: BamlValue = 42i64.into();
        assert!(matches!(i, BamlValue::Int(42)));

        let b: BamlValue = true.into();
        assert!(matches!(b, BamlValue::Bool(true)));
    }

    #[test]
    fn test_baml_value_serde() {
        let original = BamlValue::Map({
            let mut m = BamlMap::new();
            m.insert("name".to_string(), BamlValue::String("Alice".to_string()));
            m.insert("age".to_string(), BamlValue::Int(30));
            m
        });

        let json = serde_json::to_string(&original).unwrap();
        let parsed: BamlValue = serde_json::from_str(&json).unwrap();

        // Note: parsed will be a Map, not preserve the exact structure
        assert!(matches!(parsed, BamlValue::Map(_)));
    }
}
