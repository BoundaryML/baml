//! Conversion from BamlValue to minijinja::Value.
//!
//! This module provides conversion of BAML runtime values to minijinja values
//! for template rendering. Custom Object implementations handle:
//! - Media (images/audio) with magic delimiter for later extraction
//! - Enums with value/alias display
//! - Classes with field aliasing
//! - Lists and Maps

use std::sync::Arc;

use indexmap::IndexMap;
use minijinja::value::{Enumerator, Object, ObjectRepr};

use ir_stub::{BamlMedia, BamlValue};

/// Trait for converting BAML values to minijinja values.
pub trait IntoMiniJinjaValue {
    fn to_minijinja_value(&self) -> minijinja::Value;
}

impl IntoMiniJinjaValue for BamlValue {
    fn to_minijinja_value(&self) -> minijinja::Value {
        match self {
            BamlValue::String(s) => minijinja::Value::from(s.clone()),
            BamlValue::Int(n) => minijinja::Value::from(*n),
            BamlValue::Float(n) => minijinja::Value::from(*n),
            BamlValue::Bool(b) => minijinja::Value::from(*b),
            BamlValue::Map(m) => {
                let map = m
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.to_minijinja_value()));
                minijinja::Value::from_iter(map)
            }
            BamlValue::List(l) => {
                let list: Vec<minijinja::Value> =
                    l.iter().map(|v| v.to_minijinja_value()).collect();
                minijinja::Value::from_object(MinijinjaBamlList { list })
            }
            BamlValue::Media(m) => minijinja::Value::from_object(MinijinjaBamlMedia::from(m.clone())),
            BamlValue::Enum(name, value) => {
                // Without IR access, we can't resolve aliases
                // Just use the value as-is
                minijinja::Value::from_object(MinijinjaBamlEnumValue {
                    value: value.clone(),
                    alias: None,
                    enum_name: name.clone(),
                })
            }
            BamlValue::Class(name, m) => {
                let map: IndexMap<String, minijinja::Value> = m
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_minijinja_value()))
                    .collect();

                // Without IR access, we can't resolve field aliases
                // Just use field names as-is
                minijinja::Value::from_object(MinijinjaBamlClass {
                    class_name: name.clone(),
                    fields: map,
                    key_to_alias: IndexMap::new(),
                })
            }
            BamlValue::Null => minijinja::Value::from(()),
        }
    }
}

// ============================================================================
// Media
// ============================================================================

pub(crate) const MAGIC_MEDIA_DELIMITER: &str = "BAML_MEDIA_MAGIC_STRING_DELIMITER";

pub(crate) struct MinijinjaBamlMedia {
    media: BamlMedia,
}

impl From<BamlMedia> for MinijinjaBamlMedia {
    fn from(media: BamlMedia) -> MinijinjaBamlMedia {
        MinijinjaBamlMedia { media }
    }
}

impl std::fmt::Display for MinijinjaBamlMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{MAGIC_MEDIA_DELIMITER}:baml-start-media:{}:baml-end-media:{MAGIC_MEDIA_DELIMITER}",
            serde_json::json!(self.media)
        )
    }
}

impl std::fmt::Debug for MinijinjaBamlMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl Object for MinijinjaBamlMedia {
    fn call(
        self: &Arc<Self>,
        _state: &minijinja::State<'_, '_>,
        args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        Err(minijinja::Error::new(
            minijinja::ErrorKind::UnknownMethod,
            format!("BamlMedia has no callable attribute '{args:#?}'"),
        ))
    }

    fn is_true(self: &Arc<Self>) -> bool {
        true
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

// ============================================================================
// Enum
// ============================================================================

#[derive(Clone)]
pub struct MinijinjaBamlEnumValue {
    pub value: String,
    pub alias: Option<String>,
    pub enum_name: String,
}

impl std::fmt::Display for MinijinjaBamlEnumValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.alias.as_ref().unwrap_or(&self.value))
    }
}

impl std::fmt::Debug for MinijinjaBamlEnumValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl serde::Serialize for MinijinjaBamlEnumValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.alias.as_ref().unwrap_or(&self.value))
    }
}

impl Object for MinijinjaBamlEnumValue {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        match key.as_str()? {
            "value" => Some(minijinja::Value::from(self.value.clone())),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::NonEnumerable
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }

    fn custom_cmp(
        self: &Arc<Self>,
        other: &minijinja::value::DynObject,
    ) -> Option<std::cmp::Ordering> {
        let other = other.downcast_ref::<Self>()?;
        Some(
            self.value
                .cmp(&other.value)
                .then(self.alias.cmp(&other.alias)),
        )
    }
}

// ============================================================================
// Class
// ============================================================================

pub(crate) struct MinijinjaBamlClass {
    #[allow(dead_code)]
    pub(crate) class_name: String,
    pub(crate) fields: IndexMap<String, minijinja::Value>,
    pub(crate) key_to_alias: IndexMap<String, String>,
}

impl std::fmt::Display for MinijinjaBamlClass {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut map = IndexMap::new();
        for (k, v) in self.fields.iter() {
            let alias = self.key_to_alias.get(k).unwrap_or(k);
            let value = if v.is_none() {
                minijinja::Value::from_object(BamlNull)
            } else {
                v.clone()
            };
            map.insert(alias.to_string(), value);
        }
        write!(f, "{map:#?}")
    }
}

impl std::fmt::Debug for MinijinjaBamlClass {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl serde::Serialize for MinijinjaBamlClass {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.fields.len()))?;
        for (k, v) in self.fields.iter() {
            let alias = self.key_to_alias.get(k).unwrap_or(k);
            map.serialize_entry(alias, v)?;
        }
        map.end()
    }
}

impl Object for MinijinjaBamlClass {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        let name = key.as_str()?;
        self.fields.get(name).cloned()
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        let keys: Vec<minijinja::Value> = self
            .fields
            .keys()
            .map(|k| minijinja::Value::from(k.as_str()))
            .collect();
        Enumerator::Values(keys)
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

// ============================================================================
// List
// ============================================================================

pub(crate) struct MinijinjaBamlList {
    pub(crate) list: Vec<minijinja::Value>,
}

impl std::fmt::Display for MinijinjaBamlList {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut list = f.debug_list();
        for value in &self.list {
            if value.is_none() {
                list.entry(&minijinja::Value::from_object(BamlNull));
            } else {
                list.entry(value);
            }
        }
        list.finish()
    }
}

impl std::fmt::Debug for MinijinjaBamlList {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl serde::Serialize for MinijinjaBamlList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.list.len()))?;
        for value in &self.list {
            seq.serialize_element(value)?;
        }
        seq.end()
    }
}

impl Object for MinijinjaBamlList {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Seq
    }

    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        self.list.get(key.as_usize()?).cloned()
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Seq(self.list.len())
    }

    fn enumerator_len(self: &Arc<Self>) -> Option<usize> {
        Some(self.list.len())
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

// ============================================================================
// Null
// ============================================================================

/// Custom null type that renders as "null" instead of minijinja's "none".
#[derive(Debug)]
pub(crate) struct BamlNull;

impl std::fmt::Display for BamlNull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("null")
    }
}

impl serde::Serialize for BamlNull {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_none()
    }
}

impl Object for BamlNull {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Plain
    }

    fn is_true(self: &Arc<Self>) -> bool {
        false
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("null")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir_stub::BamlMap;

    #[test]
    fn test_string_conversion() {
        let val = BamlValue::String("hello".to_string());
        let jinja_val = val.to_minijinja_value();
        assert_eq!(jinja_val.as_str(), Some("hello"));
    }

    #[test]
    fn test_int_conversion() {
        let val = BamlValue::Int(42);
        let jinja_val = val.to_minijinja_value();
        assert_eq!(jinja_val.as_i64(), Some(42));
    }

    #[test]
    fn test_map_conversion() {
        let mut map = BamlMap::new();
        map.insert("key".to_string(), BamlValue::String("value".to_string()));
        let val = BamlValue::Map(map);
        let jinja_val = val.to_minijinja_value();

        let key_val = jinja_val.get_item(&minijinja::Value::from("key")).unwrap();
        assert_eq!(key_val.as_str(), Some("value"));
    }

    #[test]
    fn test_list_conversion() {
        let list = vec![BamlValue::Int(1), BamlValue::Int(2), BamlValue::Int(3)];
        let val = BamlValue::List(list);
        let jinja_val = val.to_minijinja_value();

        // Check it's iterable
        let items: Vec<_> = jinja_val.try_iter().unwrap().collect();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn test_enum_conversion() {
        let val = BamlValue::Enum("Color".to_string(), "Red".to_string());
        let jinja_val = val.to_minijinja_value();

        // Should render as the value name
        assert_eq!(format!("{}", jinja_val), "Red");
    }

    #[test]
    fn test_class_conversion() {
        let mut fields = BamlMap::new();
        fields.insert("name".to_string(), BamlValue::String("Alice".to_string()));
        fields.insert("age".to_string(), BamlValue::Int(30));
        let val = BamlValue::Class("Person".to_string(), fields);
        let jinja_val = val.to_minijinja_value();

        let name_val = jinja_val.get_item(&minijinja::Value::from("name")).unwrap();
        assert_eq!(name_val.as_str(), Some("Alice"));
    }
}
