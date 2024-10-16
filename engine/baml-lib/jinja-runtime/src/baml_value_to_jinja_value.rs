use std::collections::HashMap;

use indexmap::IndexMap;
use internal_baml_core::ir::repr::IntermediateRepr;
use minijinja::Value;

use crate::{BamlMedia, BamlValue};
// use internal_baml_core::ir::repr::IntermediateRepr;

// impl From<BamlValue> for minijinja::Value {
//     fn from(arg: BamlValue) -> minijinja::Value {
//         match arg {
//             BamlValue::String(s) => minijinja::Value::from(s),
//             BamlValue::Int(n) => minijinja::Value::from(n),
//             BamlValue::Float(n) => minijinja::Value::from(n),
//             BamlValue::Bool(b) => minijinja::Value::from(b),
//             BamlValue::Map(m) => {
//                 let map = m.into_iter().map(|(k, v)| (k, minijinja::Value::from(v)));
//                 minijinja::Value::from_iter(map)
//             }
//             BamlValue::List(l) => {
//                 let list: Vec<minijinja::Value> = l.into_iter().map(|v| v.into()).collect();
//                 minijinja::Value::from(list)
//             }
//             BamlValue::Media(i) => i.into(),
//             BamlValue::Enum(_, v) => minijinja::Value::from(v),
//             BamlValue::Class(_, m) => {
//                 let map = m.into_iter().map(|(k, v)| (k, minijinja::Value::from(v)));
//                 minijinja::Value::from_iter(map)
//             }
//             BamlValue::Null => minijinja::Value::from(()),
//         }
//     }
// }

pub trait IntoMiniJinjaValue {
    fn into_minijinja_value(&self, ir: &IntermediateRepr) -> minijinja::Value;
}

impl IntoMiniJinjaValue for BamlValue {
    fn into_minijinja_value(&self, ir: &IntermediateRepr) -> minijinja::Value {
        println!("BamlValue::into_minijinja_value");
        match self {
            BamlValue::String(s) => minijinja::Value::from(s.clone()),
            BamlValue::Int(n) => minijinja::Value::from(n.clone()),
            BamlValue::Float(n) => minijinja::Value::from(n.clone()),
            BamlValue::Bool(b) => minijinja::Value::from(b.clone()),
            BamlValue::Map(m) => {
                let map = m
                    .into_iter()
                    .map(|(k, v)| (k.as_str(), v.into_minijinja_value(ir)));
                minijinja::Value::from_iter(map)
            }
            BamlValue::List(l) => {
                let list: Vec<minijinja::Value> =
                    l.into_iter().map(|v| v.into_minijinja_value(ir)).collect();
                minijinja::Value::from(list)
            }
            BamlValue::Media(i) => i.into_minijinja_value(ir),
            BamlValue::Enum(_, v) => minijinja::Value::from(v.clone()),
            BamlValue::Class(_, m) => {
                let map = m
                    .into_iter()
                    .map(|(k, v)| (k.as_str(), v.into_minijinja_value(ir)));
                // minijinja::Value::from_iter(map)
                // minijinja::Value::from_object(MinijinjaBamlClass::from((
                //     m,
                //    &IndexMap::from([("prop1".to_string(), "key1".to_string())]),
                //)))
                minijinja::Value::from_iter(MinijinjaBamlClass {
                    class: map.map(|(k, v)| (k.to_string(), v)).collect(),
                    key_to_alias: IndexMap::from(
                        [("prop1".to_string(), "key1".to_string())].clone(),
                    ),
                    index: 0,
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
    fn into_minijinja_value(&self, ir: &IntermediateRepr) -> minijinja::Value {
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

struct MinijinjaBamlClass {
    class: IndexMap<String, minijinja::Value>,
    key_to_alias: IndexMap<String, String>,
    index: usize,
    // ir: &'a IntermediateRepr,
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

// impl From<(&IndexMap<String, BamlValue>, &IndexMap<String, String>)> for MinijinjaBamlClass {
//     fn from(
//         (class_map, key_to_alias): (&IndexMap<String, BamlValue>, &IndexMap<String, String>),
//     ) -> MinijinjaBamlClass {
//         MinijinjaBamlClass {
//             class: class_map.clone(),
//             key_to_alias: key_to_alias.clone(),
//             index: 0,
//         }
//     }
// }

impl std::fmt::Display for MinijinjaBamlClass {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        println!("MinijinjaBamlClass::fmt");
        let mut map = IndexMap::new();
        // replace the keys with the aliases
        for (k, v) in self.class.iter() {
            let alias = self.key_to_alias.get(k).unwrap_or(k);
            map.insert(alias.to_string(), v.clone());
        }
        write!(f, "{:#?}", map)
    }
}

// impl std::fmt::Display for IndexMap<String, BamlValue> {
//     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//         write!(f, "{:#?}", self)
//     }
// }

impl std::fmt::Debug for MinijinjaBamlClass {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        println!("MinijinjaBamlClass::Debug::fmt");
        std::fmt::Display::fmt(self, f)
    }
}

impl minijinja::value::IteratorObject for MinijinjaBamlClass {
    fn next_value(&self) -> Option<minijinja::Value> {
        println!("next_value");
        let key = self.key_to_alias.get_index(self.index);
        // self.index += 1;
        Some(
            key.map(|(k, v)| Value::from_serializable(&(k.clone(), v.clone())))
                .unwrap(),
        )
        // match &self.class {

        //     // BamlValue::Class(_, m) => {
        //     //     let key = self.key_to_alias.get_index(self.index);
        //     //     // self.index += 1;

        //     //     Some(
        //     //         key.map(|(k, v)| Value::from_serializable(&(k.clone(), v.clone())))
        //     //             .unwrap(),
        //     //     )
        //     // }
        //     _ => None,
        // }
    }

    fn iterator_len(&self) -> Option<usize> {
        // match &self.class {
        //     BamlValue::Class(_, m) => Some(m.len()),
        //     _ => None,
        // }
        Some(self.key_to_alias.len())
    }
}

// impl minijinja::value::StructObject for MinijinjaBamlClass {
//     fn get_field(&self, key: &str) -> Option<minijinja::value::Value> {
//         match &self.class {
//             BamlValue::Class(_, m) => Some(m.get(key).unwrap().into_minijinja_value()),
//             _ => None,
//         }
//     }
// }
