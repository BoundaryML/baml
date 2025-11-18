use std::sync::Arc;

use baml_types::EvaluationContext;
use indexmap::IndexMap;
use internal_baml_core::ir::{repr::IntermediateRepr, IRHelper};
use minijinja::value::{Enumerator, Object, ObjectRepr};

use crate::{BamlMedia, BamlValue};

pub trait IntoMiniJinjaValue {
    fn to_minijinja_value(
        &self,
        ir: &IntermediateRepr,
        eval_ctx: &EvaluationContext<'_>,
    ) -> minijinja::Value;
}

impl IntoMiniJinjaValue for BamlValue {
    fn to_minijinja_value(
        &self,
        ir: &IntermediateRepr,
        eval_ctx: &EvaluationContext<'_>,
    ) -> minijinja::Value {
        match self {
            BamlValue::String(s) => minijinja::Value::from(s.clone()),
            BamlValue::Int(n) => minijinja::Value::from(*n),
            BamlValue::Float(n) => minijinja::Value::from(*n),
            BamlValue::Bool(b) => minijinja::Value::from(*b),
            BamlValue::Map(m) => {
                let map = m
                    .into_iter()
                    .map(|(k, v)| (k.as_str(), v.to_minijinja_value(ir, eval_ctx)));
                minijinja::Value::from_iter(map)
            }
            BamlValue::List(l) => {
                let list: Vec<minijinja::Value> = l
                    .iter()
                    .map(|v| v.to_minijinja_value(ir, eval_ctx))
                    .collect();
                minijinja::Value::from_object(MinijinjaBamlList { list })
            }
            BamlValue::Media(i) => i.to_minijinja_value(ir, eval_ctx),
            // For enums and classes we compute the aliases from the IR, and generate custom jinja structs that print out the alias if stringified.
            BamlValue::Enum(name, value) => {
                let mut alias: Option<String> = None;
                if let Ok(e) = ir.find_enum(name) {
                    if let Some(enum_value) = e
                        .walk_values()
                        .find(|ir_enum_value| ir_enum_value.item.elem.0 == *value)
                    {
                        alias = enum_value.alias(eval_ctx).ok().and_then(|a| a);
                    }
                }
                minijinja::Value::from_object(MinijinjaBamlEnumValue {
                    value: value.clone(),
                    alias,
                    enum_name: name.clone(),
                })
            }
            BamlValue::Class(name, m) => {
                let map = m
                    .into_iter()
                    .map(|(k, v)| (k.as_str(), v.to_minijinja_value(ir, eval_ctx)));

                let mut key_to_alias = IndexMap::new();
                if let Ok(c) = ir.find_class(name) {
                    for field in c.walk_fields() {
                        let key = field
                            .alias(eval_ctx)
                            .ok()
                            .and_then(|a| a)
                            .unwrap_or_else(|| field.name().to_string());
                        key_to_alias.insert(field.name().to_string(), key);
                    }
                }

                minijinja::Value::from_object(MinijinjaBamlClass {
                    class: map.map(|(k, v)| (k.to_string(), v)).collect(),
                    key_to_alias,
                })
            }
            BamlValue::Null => minijinja::Value::from(()),
        }
    }
}

pub(crate) struct MinijinjaBamlMedia {
    media: BamlMedia,
}

impl From<BamlMedia> for MinijinjaBamlMedia {
    fn from(media: BamlMedia) -> MinijinjaBamlMedia {
        MinijinjaBamlMedia { media }
    }
}

impl IntoMiniJinjaValue for BamlMedia {
    fn to_minijinja_value(
        &self,
        _ir: &IntermediateRepr,
        _eval_ctx: &EvaluationContext<'_>,
    ) -> minijinja::Value {
        minijinja::Value::from_object(MinijinjaBamlMedia::from(self.clone()))
    }
}

const MAGIC_MEDIA_DELIMITER: &str = "BAML_MEDIA_MAGIC_STRING_DELIMITER";

impl std::fmt::Display for MinijinjaBamlMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{MAGIC_MEDIA_DELIMITER}:baml-start-media:{}:baml-end-media:{MAGIC_MEDIA_DELIMITER}",
            serde_json::json!(self.media)
        )
    }
}

// Necessary for nested instances of MinijinjaBamlImage to get rendered correctly in prompts
// See https://github.com/BoundaryML/baml/pull/855 for explanation
impl std::fmt::Debug for MinijinjaBamlMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl minijinja::value::Object for MinijinjaBamlMedia {
    fn call(
        self: &Arc<Self>,
        _state: &minijinja::State<'_, '_>,
        args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        Err(minijinja::Error::new(
            minijinja::ErrorKind::UnknownMethod,
            format!("BamlImage has no callable attribute '{args:#?}'"),
        ))
    }

    fn is_true(self: &Arc<Self>) -> bool {
        true
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

#[derive(Debug)]
pub struct MinijinjaBamlEnumType {
    pub enum_name: String,
    pub enum_values: IndexMap<String, MinijinjaBamlEnumValue>,
}

impl Object for MinijinjaBamlEnumType {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        self.enum_values
            .get(key.as_str()?)
            .map(|v| minijinja::value::Value::from_object(v.clone()))
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::NonEnumerable
    }

    fn enumerator_len(self: &Arc<Self>) -> Option<usize> {
        None
    }
}

// Enums

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

impl serde::Serialize for MinijinjaBamlEnumValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize as a string using the alias if available, otherwise the value name
        serializer.serialize_str(self.alias.as_ref().unwrap_or(&self.value))
    }
}

impl std::fmt::Debug for MinijinjaBamlEnumValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl Object for MinijinjaBamlEnumValue {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        // TODO: add jinja type-checking for this in evaluate_type to allow this
        match key.as_str()? {
            "value" => Some(minijinja::Value::from(self.value.clone())),
            // CLAUDE: do not add more fields here.
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::NonEnumerable
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }

    fn value_cmp(self: &Arc<Self>, other: &minijinja::Value) -> Option<std::cmp::Ordering> {
        // Compare to strings - compare against value name only, NOT alias
        // This is critical to preserving backwards compatibility with enum value handling
        // pre-0.206.0, where enum values were simply modelled as `string` in minijinja.
        if let Some(other_str) = other.as_str() {
            return Some(self.value.as_str().cmp(other_str));
        }

        // Delegate to custom_cmp for object comparisons
        if let Some(other_obj) = other.as_object() {
            return self.custom_cmp(other_obj);
        }

        None
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

pub(crate) struct MinijinjaBamlClass {
    pub(crate) class: IndexMap<String, minijinja::Value>,
    pub(crate) key_to_alias: IndexMap<String, String>,
}

impl std::fmt::Display for MinijinjaBamlClass {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut map = IndexMap::new();
        // replace the keys with the aliases
        for (k, v) in self.class.iter() {
            let alias = self.key_to_alias.get(k).unwrap_or(k);

            // This handles nested none values.
            // Top level none values are handled in jinja-runtime/src/jinja_helpers.rs.
            let value = if v.is_none() {
                minijinja::Value::from_object(BamlNull)
            } else {
                v.clone()
            };

            map.insert(alias.to_string(), value);
        }
        // Use pretty-printed JSON formatting as expected by tests
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
        let mut map = serializer.serialize_map(Some(self.class.len()))?;

        // Serialize with aliased keys
        for (k, v) in self.class.iter() {
            let alias = self.key_to_alias.get(k).unwrap_or(k);
            map.serialize_entry(alias, v)?;
        }

        map.end()
    }
}

impl MinijinjaBamlClass {
    /// Get the aliased keys and values for serialization
    pub fn get_aliased_entries(&self) -> Vec<(String, &minijinja::Value)> {
        self.class
            .iter()
            .map(|(k, v)| {
                let alias = self.key_to_alias.get(k).unwrap_or(k);
                (alias.clone(), v)
            })
            .collect()
    }
}

impl Object for MinijinjaBamlClass {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        let name = key.as_str()?;
        self.class.get(name).cloned()
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        let keys: Vec<minijinja::Value> = self
            .class
            .keys()
            .map(|k| minijinja::Value::from(k.as_str()))
            .collect();
        Enumerator::Values(keys)
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

// List

pub(crate) struct MinijinjaBamlList {
    pub(crate) list: Vec<minijinja::Value>,
}

impl std::fmt::Display for MinijinjaBamlList {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut list = f.debug_list();

        // Replace "none" with "null". See comments in std::fmt::Display impl
        // for MinijinjaBamlClass and see BamlNull.
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

// Null

/// This only exists because [`minijinja`] renders "none" instead of "null".
///
/// Don't use for anything else other than rendering because if we use this
/// instead of `minijinja::Value::from(())` then the `{% if v is none %}`
/// comparison in Jinja will not work.
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
