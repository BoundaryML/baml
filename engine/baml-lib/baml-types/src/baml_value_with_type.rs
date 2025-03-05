use crate::{rpc::upload_baml_src::BamlTypeReference, BamlMedia, BamlValue, FieldType, TypeValue};
use indexmap::IndexSet;
use serde::Serialize;

/// TODO: implement Deserialize
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum BamlValueWithConcreteType {
    Class {
        #[serde(rename = "@type")]
        r#type: BamlTypeReference,
        #[serde(rename = "@data")]
        data: Vec<BamlValueClassEntryWithConcreteType>,
    },
    Enum {
        #[serde(rename = "@type")]
        r#type: BamlTypeReference,
        #[serde(rename = "@data")]
        data: String,
    },
    List {
        #[serde(rename = "@type")]
        r#type: BamlTypeReference,
        #[serde(rename = "@data")]
        data: Vec<BamlValueWithConcreteType>,
    },
    Map {
        #[serde(rename = "@type")]
        r#type: BamlTypeReference,
        #[serde(rename = "@data")]
        data: Vec<BamlValueMapEntryWithConcreteType>,
    },
    Null {
        #[serde(rename = "@type")]
        r#type: BamlTypeReference,
        #[serde(rename = "@data")]
        data: (),
    },
    Bool {
        #[serde(rename = "@type")]
        r#type: BamlTypeReference,
        #[serde(rename = "@data")]
        data: bool,
    },
    String {
        #[serde(rename = "@type")]
        r#type: BamlTypeReference,
        #[serde(rename = "@data")]
        data: String,
    },
    Int {
        #[serde(rename = "@type")]
        r#type: BamlTypeReference,
        #[serde(rename = "@data")]
        data: i64,
    },
    Float {
        #[serde(rename = "@type")]
        r#type: BamlTypeReference,
        #[serde(rename = "@data")]
        data: f64,
    },
    Media {
        #[serde(rename = "@type")]
        r#type: BamlTypeReference,
        // TODO: media type serialization format needs to be decoupled from the runtime-internal repr!
        #[serde(rename = "@data")]
        data: BamlMedia,
    },
    // TODO: literals
}

#[derive(Debug, Clone, Serialize)]
pub struct BamlValueClassEntryWithConcreteType {
    pub field: String,
    pub value: BamlValueWithConcreteType,
}

#[derive(Debug, Clone, Serialize)]
pub struct BamlValueMapEntryWithConcreteType {
    pub key: String,
    pub value: BamlValueWithConcreteType,
}

impl BamlValueWithConcreteType {
    pub fn r#type(&self) -> &BamlTypeReference {
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

    pub fn rewrite_references_to_include_id(
        &mut self,
        id_rewrite: &impl Fn(&mut BamlTypeReference) -> (),
    ) -> &mut Self {
        match self {
            BamlValueWithConcreteType::Class { r#type, data } => {
                id_rewrite(r#type);
                for v in data {
                    v.value.rewrite_references_to_include_id(id_rewrite);
                }
            }
            BamlValueWithConcreteType::Enum { r#type, .. } => {
                id_rewrite(r#type);
            }
            BamlValueWithConcreteType::List { r#type, data } => {
                id_rewrite(r#type);
                for v in data {
                    v.rewrite_references_to_include_id(id_rewrite);
                }
            }
            BamlValueWithConcreteType::Map { r#type, data } => {
                id_rewrite(r#type);
                for v in data {
                    v.value.rewrite_references_to_include_id(id_rewrite);
                }
            }
            BamlValueWithConcreteType::Null { r#type, .. } => {
                id_rewrite(r#type);
            }
            BamlValueWithConcreteType::Bool { r#type, .. } => {
                id_rewrite(r#type);
            }
            BamlValueWithConcreteType::String { r#type, .. } => {
                id_rewrite(r#type);
            }
            BamlValueWithConcreteType::Int { r#type, .. } => {
                id_rewrite(r#type);
            }
            BamlValueWithConcreteType::Float { r#type, .. } => {
                id_rewrite(r#type);
            }
            BamlValueWithConcreteType::Media { r#type, .. } => {
                id_rewrite(r#type);
            }
        }
        self
    }
}
// #[derive(Debug, Clone, Serialize)]
// pub enum BamlLiteralWithConcreteType {
//     String(String),
//     Int(i64),
//     Float(f64),
// }

// mod concrete_type {
//     use serde::ser::SerializeMap;

//     use crate::TypeValue;

//     use super::*;

//     pub fn serialize<S>(from: &FieldType, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: serde::Serializer,
//     {
//         let mut map = serializer.serialize_map(Some(2))?;
//         match from {
//             FieldType::Class(class_name) => {
//                 map.serialize_entry("type", "class")?;
//                 map.serialize_entry("type_id", &format!("{}@@@12345", class_name))?;
//             }
//             FieldType::Enum(enum_name) => {
//                 map.serialize_entry("type", "enum")?;
//                 map.serialize_entry("type_id", &format!("{}@@@12345", enum_name))?;
//             }
//             FieldType::List(inner) => {
//                 // map.serialize_entry("type", "array")?;
//                 // let mut inner_map = serializer.serialize_map(Some(2))?;
//                 // concrete_type::serialize(**inner, &mut inner_map)?;
//                 // map.serialize_entry("items", &inner_map.end()?)?;
//             }
//             FieldType::Map(key, value) => {
//                 // map.serialize_entry("type", "map")?;
//                 // let key_serialized = concrete_type::serialize(key, serializer)?;
//                 // let value_serialized = concrete_type::serialize(value, serializer)?;
//                 // map.serialize_entry("key", &key_serialized)?;
//                 // map.serialize_entry("value", &value_serialized)?;
//             }
//             FieldType::Primitive(type_value) => match type_value {
//                 TypeValue::Null => {
//                     map.serialize_entry("type", "null")?;
//                 }
//                 TypeValue::String => {
//                     map.serialize_entry("type", "string")?;
//                 }
//                 TypeValue::Int => {
//                     map.serialize_entry("type", "int")?;
//                 }
//                 TypeValue::Float => {
//                     map.serialize_entry("type", "float")?;
//                 }
//                 TypeValue::Bool => {
//                     map.serialize_entry("type", "bool")?;
//                 }
//                 TypeValue::Media(_media_type) => {
//                     // TODO: this is a lie, media types are not strings
//                     map.serialize_entry("type", "string")?;
//                 }
//             },
//             FieldType::Literal(literal) => {
//                 map.serialize_entry("type", "literal")?;
//                 map.serialize_entry("value", &literal)?;
//             }
//             FieldType::Union(union) => {
//                 map.serialize_entry("type", "union")?;
//                 map.serialize_entry(
//                     "any_of",
//                     &union.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
//                 )?;
//             }
//             _ => unimplemented!("concrete_type::serialize: {:?}", from),
//         }
//         map.end()
//     }
// }

/// TODO: delete this implementation
/// we need to pipe type information out of arg coercion
impl From<BamlValue> for BamlValueWithConcreteType {
    fn from(value: BamlValue) -> Self {
        match value {
            BamlValue::Null => BamlValueWithConcreteType::Null {
                r#type: FieldType::Primitive(TypeValue::Null).into(),
                data: (),
            },
            BamlValue::String(s) => BamlValueWithConcreteType::String {
                r#type: FieldType::Primitive(TypeValue::String).into(),
                data: s,
            },
            BamlValue::Int(i) => BamlValueWithConcreteType::Int {
                r#type: FieldType::Primitive(TypeValue::Int).into(),
                data: i,
            },
            BamlValue::Float(f) => BamlValueWithConcreteType::Float {
                r#type: FieldType::Primitive(TypeValue::Float).into(),
                data: f,
            },
            BamlValue::Bool(b) => BamlValueWithConcreteType::Bool {
                r#type: FieldType::Primitive(TypeValue::Bool).into(),
                data: b,
            },
            BamlValue::Map(m) => {
                let m = m
                    .into_iter()
                    .map(|(k, v)| BamlValueMapEntryWithConcreteType {
                        key: k,
                        value: v.into(),
                    })
                    .collect::<Vec<_>>();
                // TODO: this should be a union of all key types, but is hardcoded to just being a string right now
                let concrete_value_type = match m
                    .iter()
                    .map(|v: &BamlValueMapEntryWithConcreteType| v.value.r#type().clone())
                    .collect::<TypeReifier2>()
                    .reified_type
                {
                    Some(t) => t,
                    None => BamlTypeReference::Null,
                };
                BamlValueWithConcreteType::Map {
                    r#type: BamlTypeReference::Map {
                        key: Box::new(BamlTypeReference::String),
                        value: Box::new(concrete_value_type),
                    },
                    data: m,
                }
            }
            BamlValue::List(l) => {
                let l = l.into_iter().map(|v| v.into()).collect::<Vec<_>>();
                let concrete_list_type = match l
                    .iter()
                    .map(|v: &BamlValueWithConcreteType| v.r#type().clone())
                    .collect::<TypeReifier2>()
                    .reified_type
                {
                    Some(t) => t,
                    // TODO: dunno what this should be
                    None => BamlTypeReference::Null,
                };
                BamlValueWithConcreteType::List {
                    r#type: BamlTypeReference::Array {
                        items: Box::new(concrete_list_type),
                    },
                    data: l,
                }
            }
            // TODO: we don't have a media
            BamlValue::Media(m) => BamlValueWithConcreteType::Media {
                r#type: FieldType::Primitive(TypeValue::Media(m.media_type)).into(),
                data: m,
            },
            BamlValue::Enum(enum_name, enum_value) => BamlValueWithConcreteType::Enum {
                r#type: FieldType::Enum(enum_name).into(),
                data: enum_value,
            },
            BamlValue::Class(class_name, fields) => {
                let fields_with_concrete_type = fields
                    .into_iter()
                    .map(|(k, v)| BamlValueClassEntryWithConcreteType {
                        field: k,
                        value: v.into(),
                    })
                    .collect::<Vec<_>>();
                BamlValueWithConcreteType::Class {
                    r#type: FieldType::Class(class_name).into(),
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

pub struct TypeReifier2 {
    pub reified_type: Option<BamlTypeReference>,
}

impl FromIterator<BamlTypeReference> for TypeReifier2 {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = BamlTypeReference>,
    {
        let type_set = iter.into_iter().collect::<IndexSet<_>>();
        match type_set.len() {
            0 => TypeReifier2 { reified_type: None },
            1 => TypeReifier2 {
                reified_type: Some(type_set.into_iter().next().unwrap()),
            },
            _ => TypeReifier2 {
                reified_type: Some(BamlTypeReference::Union {
                    any_of: type_set.into_iter().collect(),
                }),
            },
        }
    }
}
