use crate::{BamlValue, FieldType, TypeValue};
use indexmap::IndexSet;
use serde::Serialize;

/// TODO: implement Deserialize
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum BamlValueWithConcreteType {
    Class {
        #[serde(rename = "@type", with = "concrete_type")]
        r#type: FieldType,
        #[serde(rename = "@data")]
        data: Vec<(String, BamlValueWithConcreteType)>,
    },
    Enum {
        #[serde(rename = "@type", with = "concrete_type")]
        r#type: FieldType,
        #[serde(rename = "@data")]
        data: String,
    },
    List {
        #[serde(rename = "@type", with = "concrete_type")]
        r#type: FieldType,
        #[serde(rename = "@data")]
        data: Vec<BamlValueWithConcreteType>,
    },
    Map {
        #[serde(rename = "@type", with = "concrete_type")]
        r#type: FieldType,
        #[serde(rename = "@data")]
        data: Vec<(String, BamlValueWithConcreteType)>,
    },
    Null {
        #[serde(rename = "@type", with = "concrete_type")]
        r#type: FieldType,
        #[serde(rename = "@data")]
        data: (),
    },
    Bool {
        #[serde(rename = "@type", with = "concrete_type")]
        r#type: FieldType,
        #[serde(rename = "@data")]
        data: bool,
    },
    String {
        #[serde(rename = "@type", with = "concrete_type")]
        r#type: FieldType,
        #[serde(rename = "@data")]
        data: String,
    },
    Int {
        #[serde(rename = "@type", with = "concrete_type")]
        r#type: FieldType,
        #[serde(rename = "@data")]
        data: i64,
    },
    Float {
        #[serde(rename = "@type", with = "concrete_type")]
        r#type: FieldType,
        #[serde(rename = "@data")]
        data: f64,
    },
    Media {
        #[serde(rename = "@type", with = "concrete_type")]
        r#type: FieldType,
        #[serde(rename = "@data")]
        data: String,
    },
    // TODO: literals
}

impl BamlValueWithConcreteType {
    pub fn r#type(&self) -> &FieldType {
        match self {
            BamlValueWithConcreteType::Class { r#type, .. } => r#type,
            BamlValueWithConcreteType::Enum { r#type, .. } => r#type,
            BamlValueWithConcreteType::List { r#type, .. } => r#type,
            BamlValueWithConcreteType::Map { r#type, .. } => r#type,
            BamlValueWithConcreteType::Null { r#type, .. } => r#type,
            BamlValueWithConcreteType::Bool { r#type, .. } => r#type,
            BamlValueWithConcreteType::String { r#type, .. } => r#type,
            BamlValueWithConcreteType::Int { r#type, .. } => r#type,
            BamlValueWithConcreteType::Float { r#type, .. } => r#type,
            BamlValueWithConcreteType::Media { r#type, .. } => r#type,
        }
    }
}
// #[derive(Debug, Clone, Serialize)]
// pub enum BamlLiteralWithConcreteType {
//     String(String),
//     Int(i64),
//     Float(f64),
// }

mod concrete_type {
    use serde::ser::SerializeMap;

    use crate::TypeValue;

    use super::*;

    pub fn serialize<S>(from: &FieldType, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        match from {
            FieldType::Primitive(type_value) => match type_value {
                TypeValue::Null => {
                    map.serialize_entry("type", "null")?;
                }
                TypeValue::String => {
                    map.serialize_entry("type", "string")?;
                }
                TypeValue::Int => {
                    map.serialize_entry("type", "int")?;
                }
                TypeValue::Float => {
                    map.serialize_entry("type", "float")?;
                }
                TypeValue::Bool => {
                    map.serialize_entry("type", "bool")?;
                }
                TypeValue::Media(_media_type) => {
                    // TODO: this is a lie, media types are not strings
                    map.serialize_entry("type", "string")?;
                }
            },
            _ => todo!(),
        }
        map.end()
    }
}

/// TODO: delete this implementation
/// we need to pipe type information out of arg coercion
impl From<BamlValue> for BamlValueWithConcreteType {
    fn from(value: BamlValue) -> Self {
        match value {
            BamlValue::Null => BamlValueWithConcreteType::Null {
                r#type: FieldType::Primitive(TypeValue::Null),
                data: (),
            },
            BamlValue::String(s) => BamlValueWithConcreteType::String {
                r#type: FieldType::Primitive(TypeValue::String),
                data: s,
            },
            BamlValue::Int(i) => BamlValueWithConcreteType::Int {
                r#type: FieldType::Primitive(TypeValue::Int),
                data: i,
            },
            BamlValue::Float(f) => BamlValueWithConcreteType::Float {
                r#type: FieldType::Primitive(TypeValue::Float),
                data: f,
            },
            BamlValue::Bool(b) => BamlValueWithConcreteType::Bool {
                r#type: FieldType::Primitive(TypeValue::Bool),
                data: b,
            },
            BamlValue::Map(m) => {
                let m = m
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<Vec<_>>();
                // TODO: this should be a union of all key types, but is hardcoded to just being a string right now
                let concrete_key_type = FieldType::string();
                let concrete_value_type = match m
                    .iter()
                    .map(|(_, v): &(String, BamlValueWithConcreteType)| v.r#type().clone())
                    .collect::<TypeReifier>()
                    .reified_type
                {
                    Some(t) => t,
                    None => FieldType::null(),
                };
                BamlValueWithConcreteType::Map {
                    r#type: FieldType::Map(
                        Box::new(concrete_key_type),
                        Box::new(concrete_value_type),
                    ),
                    data: m,
                }
            }
            BamlValue::List(l) => {
                let l = l.into_iter().map(|v| v.into()).collect::<Vec<_>>();
                let concrete_list_type = match l
                    .iter()
                    .map(|v: &BamlValueWithConcreteType| v.r#type().clone())
                    .collect::<TypeReifier>()
                    .reified_type
                {
                    Some(t) => t,
                    // TODO: dunno what this should be
                    None => FieldType::null(),
                }
                .as_list();
                BamlValueWithConcreteType::List {
                    r#type: concrete_list_type,
                    data: l,
                }
            }
            // TODO: we don't have a media
            BamlValue::Media(m) => BamlValueWithConcreteType::Media {
                r#type: FieldType::Primitive(TypeValue::Media(m.media_type)),
                data: "media-placeholder".to_string(),
            },
            BamlValue::Enum(enum_name, enum_value) => BamlValueWithConcreteType::Enum {
                r#type: FieldType::Enum(enum_name),
                data: enum_value,
            },
            BamlValue::Class(class_name, fields) => {
                let fields_with_concrete_type = fields
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<Vec<_>>();
                BamlValueWithConcreteType::Class {
                    r#type: FieldType::Class(class_name),
                    data: fields_with_concrete_type,
                }
            }
        }
    }
}

pub struct TypeReifier {
    pub reified_type: Option<FieldType>,
}

impl FromIterator<FieldType> for TypeReifier {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = FieldType>,
    {
        let type_set = iter.into_iter().collect::<IndexSet<_>>();
        match type_set.len() {
            0 => TypeReifier { reified_type: None },
            1 => TypeReifier {
                reified_type: Some(type_set.into_iter().next().unwrap()),
            },
            _ => TypeReifier {
                reified_type: Some(FieldType::Union(type_set.into_iter().collect())),
            },
        }
    }
}
