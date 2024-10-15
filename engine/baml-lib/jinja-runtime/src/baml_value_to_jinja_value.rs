use internal_baml_core::ir::repr::IntermediateRepr;

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
                minijinja::Value::from_iter(map)
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
    class: BamlValue,
}

impl From<BamlValue> for MinijinjaBamlClass {
    fn from(class: BamlValue) -> MinijinjaBamlClass {
        MinijinjaBamlClass { class }
    }
}
