//! Conversion from bex_vm_types::Value to minijinja::Value.
//!
//! This module provides conversion of VM runtime values to minijinja values
//! for template rendering. It replaces the BamlValue-based conversion
//! with direct VM value conversion.

use std::sync::Arc;

use indexmap::IndexMap;
use minijinja::value::{Enumerator, Object, ObjectRepr};

use bex_vm_types::{Class, Enum, Instance, MediaContent, MediaValue, ObjectIndex, Value, Variant};

/// Magic delimiter for media in templates.
pub(crate) const MAGIC_MEDIA_DELIMITER: &str = "BAML_MEDIA_MAGIC_STRING_DELIMITER";

// ============================================================================
// Heap Accessor Trait
// ============================================================================

/// Trait for accessing objects in the VM heap.
///
/// This abstraction allows the conversion code to work with different
/// heap implementations (e.g., Program's ObjectPool, BexHeap, etc.)
pub trait HeapAccessor<F = ()> {
    /// Get an object by its index.
    fn get_object(&self, idx: ObjectIndex) -> &bex_vm_types::Object<F>;
}

// Implement HeapAccessor for ObjectPool
impl<F> HeapAccessor<F> for bex_vm_types::ObjectPool<F> {
    fn get_object(&self, idx: ObjectIndex) -> &bex_vm_types::Object<F> {
        &self[idx]
    }
}

// ============================================================================
// IntoMiniJinjaValue Trait
// ============================================================================

/// Trait for converting VM values to minijinja values.
pub trait IntoMiniJinjaValue {
    /// Convert to a minijinja Value using the provided heap for object resolution.
    fn to_minijinja_value<F>(&self, heap: &impl HeapAccessor<F>) -> minijinja::Value;
}

impl IntoMiniJinjaValue for Value {
    fn to_minijinja_value<F>(&self, heap: &impl HeapAccessor<F>) -> minijinja::Value {
        match self {
            Value::Null => minijinja::Value::from(()),
            Value::Int(n) => minijinja::Value::from(*n),
            Value::Float(n) => minijinja::Value::from(*n),
            Value::Bool(b) => minijinja::Value::from(*b),
            Value::Object(idx) => object_to_minijinja(*idx, heap),
        }
    }
}

/// Convert an object to minijinja::Value.
fn object_to_minijinja<F>(idx: ObjectIndex, heap: &impl HeapAccessor<F>) -> minijinja::Value {
    let obj = heap.get_object(idx);
    match obj {
        bex_vm_types::Object::String(s) => minijinja::Value::from(s.clone()),

        bex_vm_types::Object::Array(items) => {
            let list: Vec<minijinja::Value> =
                items.iter().map(|v| v.to_minijinja_value(heap)).collect();
            minijinja::Value::from_object(MiniJinjaVmList { list })
        }

        bex_vm_types::Object::Map(map) => {
            let entries: IndexMap<String, minijinja::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), v.to_minijinja_value(heap)))
                .collect();
            minijinja::Value::from_object(MiniJinjaVmMap { entries })
        }

        bex_vm_types::Object::Instance(Instance { class, fields }) => {
            // Get class definition to find field names
            let class_obj = heap.get_object(*class);
            if let bex_vm_types::Object::Class(Class {
                name, field_names, ..
            }) = class_obj
            {
                let field_values: IndexMap<String, minijinja::Value> = field_names
                    .iter()
                    .zip(fields.iter())
                    .map(|(name, value)| (name.clone(), value.to_minijinja_value(heap)))
                    .collect();
                minijinja::Value::from_object(MiniJinjaVmClass {
                    class_name: name.clone(),
                    fields: field_values,
                })
            } else {
                // Shouldn't happen - class index should point to a Class object
                minijinja::Value::from(())
            }
        }

        bex_vm_types::Object::Variant(Variant { enm, index }) => {
            // Get enum definition to find variant name
            let enum_obj = heap.get_object(*enm);
            if let bex_vm_types::Object::Enum(Enum {
                name,
                variant_names,
            }) = enum_obj
            {
                let variant_name = variant_names.get(*index).cloned().unwrap_or_default();
                minijinja::Value::from_object(MiniJinjaVmEnumValue {
                    enum_name: name.clone(),
                    value: variant_name,
                    alias: None, // TODO: Add alias support when available in Enum definition
                })
            } else {
                // Shouldn't happen
                minijinja::Value::from(())
            }
        }

        bex_vm_types::Object::Media(media) => {
            minijinja::Value::from_object(MiniJinjaVmMedia {
                object_index: idx,
                media: media.clone(),
            })
        }

        // Other object types that don't have a natural minijinja representation
        bex_vm_types::Object::Function(_)
        | bex_vm_types::Object::Class(_)
        | bex_vm_types::Object::Enum(_)
        | bex_vm_types::Object::Future(_)
        | bex_vm_types::Object::PrimitiveClient(_)
        | bex_vm_types::Object::PromptAst(_)
        | bex_vm_types::Object::HttpRequest(_) => minijinja::Value::from(()),
    }
}

// ============================================================================
// Media
// ============================================================================

/// Media value wrapper for minijinja.
pub(crate) struct MiniJinjaVmMedia {
    /// The ObjectIndex for this media in the VM heap.
    pub(crate) object_index: ObjectIndex,
    /// The media content (for serialization).
    pub(crate) media: MediaValue,
}

impl std::fmt::Display for MiniJinjaVmMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // Serialize media for template rendering with magic delimiters
        // Include object_index for VM-native rendering
        let content_json = match &self.media.content {
            MediaContent::Url { url, .. } => {
                serde_json::json!({
                    "type": "url",
                    "url": url,
                    "media_type": self.media.mime_type.as_deref().unwrap_or(""),
                    "object_index": self.object_index.raw(),
                })
            }
            MediaContent::Base64 { base64_data } => {
                serde_json::json!({
                    "type": "base64",
                    "base64": base64_data,
                    "media_type": self.media.mime_type.as_deref().unwrap_or(""),
                    "object_index": self.object_index.raw(),
                })
            }
            MediaContent::File { file, .. } => {
                serde_json::json!({
                    "type": "file",
                    "path": file,
                    "media_type": self.media.mime_type.as_deref().unwrap_or(""),
                    "object_index": self.object_index.raw(),
                })
            }
        };

        write!(
            f,
            "{MAGIC_MEDIA_DELIMITER}:baml-start-media:{}:baml-end-media:{MAGIC_MEDIA_DELIMITER}",
            content_json
        )
    }
}

impl std::fmt::Debug for MiniJinjaVmMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl Object for MiniJinjaVmMedia {
    fn call(
        self: &Arc<Self>,
        _state: &minijinja::State<'_, '_>,
        args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        Err(minijinja::Error::new(
            minijinja::ErrorKind::UnknownMethod,
            format!("Media has no callable attribute '{args:#?}'"),
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
// Enum Value
// ============================================================================

/// Enum value wrapper for minijinja.
#[derive(Clone)]
pub struct MiniJinjaVmEnumValue {
    pub enum_name: String,
    pub value: String,
    pub alias: Option<String>,
}

impl std::fmt::Display for MiniJinjaVmEnumValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.alias.as_ref().unwrap_or(&self.value))
    }
}

impl std::fmt::Debug for MiniJinjaVmEnumValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl serde::Serialize for MiniJinjaVmEnumValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.alias.as_ref().unwrap_or(&self.value))
    }
}

impl Object for MiniJinjaVmEnumValue {
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
// Class Instance
// ============================================================================

/// Class instance wrapper for minijinja.
pub(crate) struct MiniJinjaVmClass {
    pub(crate) class_name: String,
    pub(crate) fields: IndexMap<String, minijinja::Value>,
}

impl std::fmt::Display for MiniJinjaVmClass {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut map = IndexMap::new();
        for (k, v) in self.fields.iter() {
            let value = if v.is_none() {
                minijinja::Value::from_object(VmNull)
            } else {
                v.clone()
            };
            map.insert(k.clone(), value);
        }
        write!(f, "{map:#?}")
    }
}

impl std::fmt::Debug for MiniJinjaVmClass {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl serde::Serialize for MiniJinjaVmClass {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.fields.len()))?;
        for (k, v) in self.fields.iter() {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl Object for MiniJinjaVmClass {
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

/// List wrapper for minijinja.
pub(crate) struct MiniJinjaVmList {
    pub(crate) list: Vec<minijinja::Value>,
}

impl std::fmt::Display for MiniJinjaVmList {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut list = f.debug_list();
        for value in &self.list {
            if value.is_none() {
                list.entry(&minijinja::Value::from_object(VmNull));
            } else {
                list.entry(value);
            }
        }
        list.finish()
    }
}

impl std::fmt::Debug for MiniJinjaVmList {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl serde::Serialize for MiniJinjaVmList {
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

impl Object for MiniJinjaVmList {
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
// Map
// ============================================================================

/// Map wrapper for minijinja.
pub(crate) struct MiniJinjaVmMap {
    pub(crate) entries: IndexMap<String, minijinja::Value>,
}

impl std::fmt::Display for MiniJinjaVmMap {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut map = IndexMap::new();
        for (k, v) in self.entries.iter() {
            let value = if v.is_none() {
                minijinja::Value::from_object(VmNull)
            } else {
                v.clone()
            };
            map.insert(k.clone(), value);
        }
        write!(f, "{map:#?}")
    }
}

impl std::fmt::Debug for MiniJinjaVmMap {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl serde::Serialize for MiniJinjaVmMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (k, v) in self.entries.iter() {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl Object for MiniJinjaVmMap {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        let name = key.as_str()?;
        self.entries.get(name).cloned()
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        let keys: Vec<minijinja::Value> = self
            .entries
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
// Null
// ============================================================================

/// Custom null type that renders as "null" instead of minijinja's "none".
#[derive(Debug)]
pub(crate) struct VmNull;

impl std::fmt::Display for VmNull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("null")
    }
}

impl serde::Serialize for VmNull {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_none()
    }
}

impl Object for VmNull {
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
    use bex_vm_types::{Object, ObjectPool};

    #[test]
    fn test_null_conversion() {
        let heap: ObjectPool<()> = ObjectPool::new();
        let val = Value::Null;
        let jinja_val = val.to_minijinja_value(&heap);
        assert!(jinja_val.is_none());
    }

    #[test]
    fn test_int_conversion() {
        let heap: ObjectPool<()> = ObjectPool::new();
        let val = Value::Int(42);
        let jinja_val = val.to_minijinja_value(&heap);
        assert_eq!(jinja_val.as_i64(), Some(42));
    }

    #[test]
    fn test_float_conversion() {
        let heap: ObjectPool<()> = ObjectPool::new();
        let val = Value::Float(3.14);
        let jinja_val = val.to_minijinja_value(&heap);
        // minijinja uses f64 internally, check via string representation
        let float_val: f64 = f64::try_from(jinja_val).unwrap();
        assert!((float_val - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn test_bool_conversion() {
        let heap: ObjectPool<()> = ObjectPool::new();
        let val = Value::Bool(true);
        let jinja_val = val.to_minijinja_value(&heap);
        assert!(jinja_val.is_true());
    }

    #[test]
    fn test_string_conversion() {
        let mut heap: ObjectPool<()> = ObjectPool::new();
        heap.push(Object::String("hello".to_string()));
        let idx = ObjectIndex::from_raw(0);

        let val = Value::Object(idx);
        let jinja_val = val.to_minijinja_value(&heap);
        assert_eq!(jinja_val.as_str(), Some("hello"));
    }

    #[test]
    fn test_list_conversion() {
        let mut heap: ObjectPool<()> = ObjectPool::new();
        let list = vec![Value::Int(1), Value::Int(2), Value::Int(3)];
        heap.push(Object::Array(list));
        let idx = ObjectIndex::from_raw(0);

        let val = Value::Object(idx);
        let jinja_val = val.to_minijinja_value(&heap);

        // Check it's iterable
        let items: Vec<_> = jinja_val.try_iter().unwrap().collect();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn test_map_conversion() {
        let mut heap: ObjectPool<()> = ObjectPool::new();
        let mut map = IndexMap::new();
        map.insert("key".to_string(), Value::Int(42));
        heap.push(Object::Map(map));
        let idx = ObjectIndex::from_raw(0);

        let val = Value::Object(idx);
        let jinja_val = val.to_minijinja_value(&heap);

        let key_val = jinja_val
            .get_item(&minijinja::Value::from("key"))
            .unwrap();
        assert_eq!(key_val.as_i64(), Some(42));
    }
}
