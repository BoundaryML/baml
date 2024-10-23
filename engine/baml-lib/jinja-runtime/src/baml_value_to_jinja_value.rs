use std::collections::HashMap;

use indexmap::IndexMap;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_core::ir::IRHelper;

use crate::{BamlMedia, BamlValue};

pub trait IntoMiniJinjaValue {
    fn into_minijinja_value(
        &self,
        ir: &IntermediateRepr,
        env_vars: &HashMap<String, String>,
    ) -> minijinja::Value;
}

impl IntoMiniJinjaValue for BamlValue {
    fn into_minijinja_value(
        &self,
        ir: &IntermediateRepr,
        env_vars: &HashMap<String, String>,
    ) -> minijinja::Value {
        match self {
            BamlValue::String(s) => minijinja::Value::from(s.clone()),
            BamlValue::Int(n) => minijinja::Value::from(n.clone()),
            BamlValue::Float(n) => minijinja::Value::from(n.clone()),
            BamlValue::Bool(b) => minijinja::Value::from(b.clone()),
            BamlValue::Map(m) => {
                let map = m
                    .into_iter()
                    .map(|(k, v)| (k.as_str(), v.into_minijinja_value(ir, env_vars)));
                minijinja::Value::from_iter(map)
            }
            BamlValue::List(l) => {
                let list: Vec<minijinja::Value> = l
                    .into_iter()
                    .map(|v| v.into_minijinja_value(ir, env_vars))
                    .collect();
                minijinja::Value::from(list)
            }
            BamlValue::Media(i) => i.into_minijinja_value(ir, env_vars),
            // For enums and classes we compute the aliases from the IR, and generate custom jinja structs that print out the alias if stringified.
            BamlValue::Enum(name, value) => {
                minijinja::Value::from(value.clone())
                // Until we can fix the broken test, just return the normal value. For now we wont support enum alias rendering.
                // let mut alias: Option<String> = None;
                // if let Ok(e) = ir.find_enum(name) {
                //     if let Some(enum_value) = e
                //         .walk_values()
                //         .find(|ir_enum_value| ir_enum_value.item.elem.0 == *value)
                //     {
                //         alias = enum_value.alias(env_vars).ok().and_then(|a| a);
                //     }
                // }
                // minijinja::Value::from_object(MinijinjaBamlEnum {
                //     value: value.clone(),
                //     alias,
                // })
            }
            BamlValue::Class(name, m) => {
                let map = m
                    .into_iter()
                    .map(|(k, v)| (k.as_str(), v.into_minijinja_value(ir, env_vars)));

                let mut key_to_alias = IndexMap::new();
                match ir.find_class(name) {
                    Ok(c) => {
                        for field in c.walk_fields() {
                            let key = field
                                .alias(&env_vars)
                                .ok()
                                .and_then(|a| a)
                                .unwrap_or_else(|| field.name().to_string());
                            key_to_alias.insert(field.name().to_string(), key);
                        }
                    }
                    Err(_) => (),
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

struct MinijinjaBamlMedia {
    media: BamlMedia,
}

impl From<BamlMedia> for MinijinjaBamlMedia {
    fn from(media: BamlMedia) -> MinijinjaBamlMedia {
        MinijinjaBamlMedia { media }
    }
}

impl IntoMiniJinjaValue for BamlMedia {
    fn into_minijinja_value(
        &self,
        ir: &IntermediateRepr,
        env_vars: &HashMap<String, String>,
    ) -> minijinja::Value {
        minijinja::Value::from_object(MinijinjaBamlMedia::from(self.clone()))
    }
}

const MAGIC_MEDIA_DELIMITER: &'static str = "BAML_MEDIA_MAGIC_STRING_DELIMITER";

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
        &self,
        _state: &minijinja::State<'_, '_>,
        args: &[minijinja::value::Value],
    ) -> Result<minijinja::value::Value, minijinja::Error> {
        Err(minijinja::Error::new(
            minijinja::ErrorKind::UnknownMethod,
            format!("BamlImage has no callable attribute '{:#?}'", args),
        ))
    }
}

// Enums

struct MinijinjaBamlEnum {
    value: String,
    alias: Option<String>,
}

impl std::fmt::Display for MinijinjaBamlEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.alias.as_ref().unwrap_or(&self.value))
    }
}

impl std::fmt::Debug for MinijinjaBamlEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl minijinja::value::Object for MinijinjaBamlEnum {
    fn kind(&self) -> minijinja::value::ObjectKind<'_> {
        minijinja::value::ObjectKind::Struct(self)
    }
}

impl minijinja::value::StructObject for MinijinjaBamlEnum {
    fn get_field(&self, name: &str) -> Option<minijinja::Value> {
        return None;
    }

    fn static_fields(&self) -> Option<&'static [&'static str]> {
        None
    }
}

impl PartialEq for MinijinjaBamlEnum {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

// Classes

impl minijinja::value::Object for MinijinjaBamlClass {
    fn kind(&self) -> minijinja::value::ObjectKind<'_> {
        minijinja::value::ObjectKind::Struct(self)
    }
}

impl minijinja::value::StructObject for MinijinjaBamlClass {
    fn get_field(&self, name: &str) -> Option<minijinja::Value> {
        self.class.get(name).cloned()
    }

    fn static_fields(&self) -> Option<&'static [&'static str]> {
        None
    }
}

struct MinijinjaBamlClass {
    class: IndexMap<String, minijinja::Value>,
    key_to_alias: IndexMap<String, String>,
}

impl IntoIterator for MinijinjaBamlClass {
    type Item = (String, minijinja::Value);
    type IntoIter = std::collections::hash_map::IntoIter<String, minijinja::Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.class
            .into_iter()
            .collect::<HashMap<String, minijinja::Value>>()
            .into_iter()
    }
}

impl std::fmt::Display for MinijinjaBamlClass {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut map = IndexMap::new();
        // replace the keys with the aliases
        for (k, v) in self.class.iter() {
            let alias = self.key_to_alias.get(k).unwrap_or(k);
            map.insert(alias.to_string(), v.clone());
        }
        write!(f, "{:#?}", map)
    }
}

impl std::fmt::Debug for MinijinjaBamlClass {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
