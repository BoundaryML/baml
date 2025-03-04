use indexmap::{IndexMap, IndexSet};

use crate::{BamlMediaType, BamlValue, BamlValueWithMeta, FieldType, TypeValue};

pub type BamlValueWithConcreteType = BamlValueWithMeta<FieldType>;

impl From<BamlValue> for BamlValueWithConcreteType {
    fn from(value: BamlValue) -> Self {
        match value {
            BamlValue::Null => {
                BamlValueWithConcreteType::Null(FieldType::Primitive(TypeValue::Null))
            }
            BamlValue::String(s) => {
                BamlValueWithConcreteType::String(s, FieldType::Primitive(TypeValue::String))
            }
            BamlValue::Int(i) => {
                BamlValueWithConcreteType::Int(i, FieldType::Primitive(TypeValue::Int))
            }
            BamlValue::Float(f) => {
                BamlValueWithConcreteType::Float(f, FieldType::Primitive(TypeValue::Float))
            }
            BamlValue::Bool(b) => {
                BamlValueWithConcreteType::Bool(b, FieldType::Primitive(TypeValue::Bool))
            }
            BamlValue::Map(m) => {
                let m_with_concrete_type = m
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<IndexMap<_, _>>();
                // TODO: this should be a union of all key types, but is hardcoded to just being a string right now
                let concrete_key_type = FieldType::string();
                let value_types = m_with_concrete_type
                    .values()
                    .map(|v: &BamlValueWithMeta<FieldType>| v.meta().clone())
                    .collect::<IndexSet<_>>();
                let concrete_value_type = match value_types.len() {
                    0 => FieldType::null(),
                    1 => value_types[0].clone(),
                    _ => FieldType::Union(value_types.into_iter().collect()),
                };
                BamlValueWithConcreteType::Map(
                    m_with_concrete_type,
                    FieldType::Map(Box::new(concrete_key_type), Box::new(concrete_value_type)),
                )
            }
            BamlValue::List(l) => {
                let l_with_concrete_type = l.into_iter().map(|v| v.into()).collect::<Vec<_>>();
                let value_types = l_with_concrete_type
                    .iter()
                    .map(|v: &BamlValueWithMeta<FieldType>| v.meta().clone())
                    .collect::<IndexSet<_>>();
                let concrete_list_type = match value_types.len() {
                    0 => FieldType::null(),
                    1 => value_types[0].clone(),
                    _ => FieldType::Union(value_types.into_iter().collect()),
                };
                BamlValueWithConcreteType::List(
                    l_with_concrete_type,
                    FieldType::List(Box::new(concrete_list_type)),
                )
            }
            // TODO: we don't have a media
            BamlValue::Media(m) => {
                let concrete_type = match m.media_type {
                    BamlMediaType::Image => {
                        FieldType::Primitive(TypeValue::Media(BamlMediaType::Image))
                    }
                    BamlMediaType::Audio => {
                        FieldType::Primitive(TypeValue::Media(BamlMediaType::Audio))
                    }
                };
                BamlValueWithConcreteType::Media(m, concrete_type)
            }
            BamlValue::Enum(enum_name, enum_value) => BamlValueWithConcreteType::Enum(
                enum_name.clone(),
                enum_value.clone(),
                FieldType::Enum(enum_name),
            ),
            BamlValue::Class(class_name, fields) => {
                let fields_with_concrete_type = fields
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<IndexMap<_, _>>();
                BamlValueWithConcreteType::Class(
                    class_name.clone(),
                    fields_with_concrete_type,
                    FieldType::Class(class_name),
                )
            }
        }
    }
}

// mod baml_value_with_concrete_type_serde {
//     use super::*;

//     pub fn serialize<S>(from: &BamlValueWithConcreteType, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: serde::Serializer,
//     {
//         match from {
//             BamlValueWithConcreteType::Null(_) => {
//                 let map = serializer.serialize_map(None)?;
//                 map.end()
//             }
//             BamlValueWithConcreteType::String(s, _) => serializer.serialize_str(s),
//             BamlValueWithConcreteType::Int(i, _) => serializer.serialize_i64(*i),
//             BamlValueWithConcreteType::Float(f, _) => serializer.serialize_f64(*f),
//             BamlValueWithConcreteType::Bool(b, _) => serializer.serialize_bool(*b),
//             BamlValueWithConcreteType::Map(m, _) => {
//                 // comment
//                 todo!()
//                 // serializer.serialize_map(m.iter())
//             }
//             BamlValueWithConcreteType::List(l, _) => serializer.serialize_seq(l.iter()),
//             // TODO: serialize media types
//             BamlValueWithConcreteType::Media(m, _) => serializer.serialize_unit(),
//             BamlValueWithConcreteType::Enum(e, v, _) => {
//                 serializer.serialize_str(&format!("{}.{}", e, v))
//             }
//             BamlValueWithConcreteType::Class(c, f, _) => {
//                 let s = serializer.serialize_struct(c, f.len())?;
//                 for (k, v) in f {
//                     s.serialize_field(k, v)?;
//                 }
//                 s.end();
//                 todo!()
//             }
//         }
//     }
// }

// mod concrete_type_serde {
//     use super::*;

//     pub fn serialize<S>(from: &FieldType, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: serde::Serializer,
//     {
//         match from {
//             FieldType::Null => {
//                 let map = serializer.serialize_map(Some(2))?;
//                 map.serialize_entry("type", "null")?;
//                 map.serialize_entry("value", "null")?;
//                 map.end()
//             }
//             FieldType::Primitive(TypeValue::String) => {
//         }
//     }
// }
