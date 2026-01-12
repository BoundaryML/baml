use std::{
    collections::{HashSet, VecDeque},
    fmt,
};

use anyhow::Result;
use indexmap::IndexMap;
use pretty::RcDoc;
use serde::{de::Visitor, ser::SerializeMap, Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    ir_type::{
        TypeGeneric, TypeIR, TypeNonStreaming, TypeStreaming, UnionConstructor,
        UnionTypeViewGeneric,
    },
    media::BamlMediaType,
    type_meta, BamlMap, BamlMedia, HasType, LiteralValue, ResponseCheck, TypeValue,
};

#[derive(Clone, Debug, PartialEq)]
pub enum BamlValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Map(BamlMap<String, BamlValue>),
    List(Vec<BamlValue>),
    Media(BamlMedia),
    Enum(String, String),
    Class(String, BamlMap<String, BamlValue>),
    Null,
}

impl TryFrom<serde_json::Value> for BamlValue {
    type Error = anyhow::Error;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        Ok(serde_json::from_value(value)?)
    }
}

impl serde::Serialize for BamlValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
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

impl BamlValue {
    pub fn r#type(&self) -> String {
        match self {
            BamlValue::String(_) => "string".into(),
            BamlValue::Int(_) => "int".into(),
            BamlValue::Float(_) => "float".into(),
            BamlValue::Bool(_) => "bool".into(),
            BamlValue::Map(kv) => {
                let value_types = kv
                    .values()
                    .map(|v| v.r#type())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(" | ");
                if value_types.is_empty() {
                    "map<string, ?>".into()
                } else {
                    format!("map<string, {value_types}>")
                }
            }
            BamlValue::List(k) => {
                let value_type = k
                    .iter()
                    .map(|v| v.r#type())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(" | ");
                if value_type.is_empty() {
                    "list<?>".into()
                } else {
                    format!("list<{value_type}>")
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

    /// Convert this BamlValue to a pretty printing document
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        match self {
            BamlValue::Null => RcDoc::text("null"),
            BamlValue::Bool(b) => RcDoc::text(b.to_string()),
            BamlValue::Int(i) => RcDoc::text(i.to_string()),
            BamlValue::Float(f) => RcDoc::text(f.to_string()),
            BamlValue::String(s) => RcDoc::text(format!("\"{}\"", escape_string(s))),
            BamlValue::List(items) => {
                if items.is_empty() {
                    RcDoc::text("[]")
                } else {
                    RcDoc::text("[")
                        .append(RcDoc::softline())
                        .append(
                            RcDoc::intersperse(
                                items.iter().map(|item| item.to_doc()),
                                RcDoc::text(",").append(RcDoc::line()),
                            )
                            .nest(2),
                        )
                        .append(RcDoc::softline())
                        .append(RcDoc::text("]"))
                }
            }
            BamlValue::Map(map) => {
                if map.is_empty() {
                    RcDoc::text("{}")
                } else {
                    RcDoc::text("{")
                        .append(RcDoc::softline())
                        .append(
                            RcDoc::intersperse(
                                map.iter().map(|(k, v)| {
                                    RcDoc::text(format!("\"{}\"", escape_string(k)))
                                        .append(RcDoc::text(":"))
                                        .append(RcDoc::space())
                                        .append(v.to_doc())
                                }),
                                RcDoc::text(",").append(RcDoc::line()),
                            )
                            .nest(2),
                        )
                        .append(RcDoc::softline())
                        .append(RcDoc::text("}"))
                }
            }
            BamlValue::Media(media) => format_media(media),
            BamlValue::Enum(enum_name, variant) => RcDoc::text(format!("{enum_name}::{variant}")),
            BamlValue::Class(class_name, fields) => {
                if fields.is_empty() {
                    RcDoc::text(format!("{class_name} {{}}"))
                } else {
                    RcDoc::text(format!("{class_name} {{"))
                        .append(RcDoc::softline())
                        .append(
                            RcDoc::intersperse(
                                fields.iter().map(|(k, v)| {
                                    RcDoc::text(k.to_string())
                                        .append(RcDoc::text(":"))
                                        .append(RcDoc::space())
                                        .append(v.to_doc())
                                }),
                                RcDoc::text(",").append(RcDoc::line()),
                            )
                            .nest(2),
                        )
                        .append(RcDoc::softline())
                        .append(RcDoc::text("}"))
                }
            }
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

    pub fn as_str(&self) -> Option<&str> {
        match self {
            BamlValue::String(s) => Some(s),
            _ => None,
        }
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

    pub fn as_list_owned(self) -> Option<Vec<BamlValue>> {
        match self {
            BamlValue::List(vals) => Some(vals),
            _ => None,
        }
    }
}

impl std::fmt::Display for BamlValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#?}", serde_json::json!(self))
    }
}

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

    fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Int(v as i64))
    }

    fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Int(v as i64))
    }

    fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Int(v as i64))
    }

    fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Int(v as i64))
    }

    fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Int(v as i64))
    }

    fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Int(v as i64))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Int(v as i64))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Int(value))
    }

    fn visit_f32<E>(self, v: f32) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Float(v as f64))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Float(value))
    }

    fn visit_char<E>(self, v: char) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::String(v.to_string()))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::String(v))
    }

    fn visit_bytes<E>(self, _: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Err(serde::de::Error::custom("bytes are not supported by BAML"))
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::String(v.to_owned()))
    }

    fn visit_i128<E>(self, _: i128) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Err(serde::de::Error::custom("i128 is not supported by BAML"))
    }

    fn visit_u128<E>(self, _: u128) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Err(serde::de::Error::custom("u128 is not supported by BAML"))
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

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        BamlValue::deserialize(deserializer)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BamlValue::Null)
    }

    fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
    where
        V: serde::de::SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element()? {
            values.push(value);
        }
        Ok(BamlValue::List(values))
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: serde::de::MapAccess<'de>,
    {
        let mut values = BamlMap::new();
        while let Some((key, value)) = map.next_entry()? {
            values.insert(key, value);
        }
        Ok(BamlValue::Map(values))
    }
}

/// A BamlValue with associated metadata.
/// This type is used to flexibly carry additional information.
/// It is used as a base type for situations where we want to represent
/// a BamlValue with additional information per node, such as a score,
/// or a constraint result.
#[derive(Clone, Debug, PartialEq)]
pub enum BamlValueWithMeta<T> {
    String(String, T),
    Int(i64, T),
    Float(f64, T),
    Bool(bool, T),
    Map(BamlMap<String, BamlValueWithMeta<T>>, T),
    List(Vec<BamlValueWithMeta<T>>, T),
    Media(BamlMedia, T),
    Enum(String, String, T),
    Class(String, BamlMap<String, BamlValueWithMeta<T>>, T),
    Null(T),
}

pub trait TypeLookups: Sized {
    fn expand_recursive_type(&self, type_alias: &str) -> anyhow::Result<&TypeIR>;
}

pub trait TypeLookupsMeta<T>: TypeLookups {
    fn expand_recursive_type(&self, type_alias: &str) -> anyhow::Result<TypeGeneric<T>>;
}

impl<Base: TypeLookups> TypeLookupsMeta<type_meta::IR> for Base {
    fn expand_recursive_type(&self, type_alias: &str) -> anyhow::Result<TypeIR> {
        match self.expand_recursive_type(type_alias) {
            Ok(t) => Ok(t.clone()),
            Err(e) => Err(e),
        }
    }
}

impl<Base: TypeLookups> TypeLookupsMeta<type_meta::NonStreaming> for Base {
    fn expand_recursive_type(&self, type_alias: &str) -> anyhow::Result<TypeNonStreaming> {
        match self.expand_recursive_type(type_alias) {
            Ok(t) => Ok(t.to_non_streaming_type(self)),
            Err(e) => Err(e),
        }
    }
}

impl<Base: TypeLookups> TypeLookupsMeta<type_meta::Streaming> for Base {
    fn expand_recursive_type(&self, type_alias: &str) -> anyhow::Result<TypeStreaming> {
        match self.expand_recursive_type(type_alias) {
            Ok(t) => Ok(t.to_streaming_type(self)),
            Err(e) => Err(e),
        }
    }
}

pub trait TypeQuery<T> {
    fn real_type(&self, lookup: &impl TypeLookups) -> TypeGeneric<T>;
    fn is_type(&self, field_type: &TypeGeneric<T>, lookup: &impl TypeLookups) -> bool;
}

impl<T: HasType<type_meta::IR>> TypeQuery<type_meta::IR> for BamlValueWithMeta<T> {
    /// Given a value and a union, picks the actual type of the value.
    ///
    /// For example, if the value is a union of `int` and `string`, and the value is an `int`,
    /// this will return `int`.
    ///
    /// If the value is a union of `int` and `string`, and the value is a `string`,
    /// this will return `string`.
    fn real_type(&self, lookup: &impl TypeLookups) -> TypeGeneric<type_meta::IR> {
        let field_type = self.field_type();

        let mut field_type = field_type;
        while let TypeGeneric::RecursiveTypeAlias { name, .. } = field_type {
            field_type = lookup
                .expand_recursive_type(name)
                .expect("Recursive type alias not found");
        }
        let field_type = field_type;

        if let TypeIR::Union(options, _) = field_type {
            let field_type = match options.view() {
                UnionTypeViewGeneric::Null => TypeGeneric::null(),
                UnionTypeViewGeneric::Optional(field_type) => {
                    if self.is_type(field_type, lookup) {
                        field_type.clone()
                    } else {
                        TypeGeneric::null()
                    }
                }
                UnionTypeViewGeneric::OneOf(field_types) => field_types
                    .into_iter()
                    .find(|t| self.is_type(t, lookup))
                    .expect("At least one type must be supported")
                    .clone(),
                UnionTypeViewGeneric::OneOfOptional(field_types) => field_types
                    .into_iter()
                    .find(|t| self.is_type(t, lookup))
                    .map_or_else(TypeIR::null, |t| t.clone()),
            };
            if &field_type == self.field_type() {
                return field_type.clone();
            }
            if let TypeGeneric::RecursiveTypeAlias { name, .. } = field_type {
                let expanded_type = lookup
                    .expand_recursive_type(name.as_str())
                    .expect("Recursive type alias not found");
                return expanded_type.clone();
            }
            return field_type;
        }
        field_type.clone()
    }

    // TODO: This will fail for type aliases?
    fn is_type(&self, field_type: &TypeIR, lookup: &impl TypeLookups) -> bool {
        let field_type = match field_type {
            TypeGeneric::RecursiveTypeAlias { name, .. } => lookup
                .expand_recursive_type(name)
                .expect("Recursive type alias not found"),
            _ => field_type,
        };

        let handle_composite = |field_type: &TypeIR| match field_type {
            TypeGeneric::Union(options, _) => match options.view() {
                UnionTypeViewGeneric::Null => self.is_type(&TypeIR::null(), lookup),
                UnionTypeViewGeneric::Optional(field_type) => {
                    self.is_type(field_type, lookup) || self.is_type(&TypeIR::null(), lookup)
                }
                UnionTypeViewGeneric::OneOf(field_types) => {
                    field_types.iter().any(|t| self.is_type(t, lookup))
                }
                UnionTypeViewGeneric::OneOfOptional(field_types) => {
                    field_types.iter().any(|t| self.is_type(t, lookup))
                        || self.is_type(&TypeIR::null(), lookup)
                }
            },
            _ => false,
        };

        match self {
            BamlValueWithMeta::String(val, _) => match field_type {
                TypeGeneric::Literal(LiteralValue::String(lit), _) => val.as_str() == lit,
                TypeGeneric::Primitive(TypeValue::String, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Int(val, _) => match field_type {
                TypeGeneric::Literal(LiteralValue::Int(lit), _) => val == lit,
                TypeGeneric::Primitive(TypeValue::Int, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Float(_, _) => match field_type {
                TypeGeneric::Primitive(TypeValue::Float, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Bool(val, _) => match field_type {
                TypeGeneric::Literal(LiteralValue::Bool(lit), _) => val == lit,
                TypeGeneric::Primitive(TypeValue::Bool, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Map(index_map, _) => match field_type {
                TypeGeneric::Map(_, value_type, _) => {
                    // TODO: Check key type
                    index_map.iter().all(|(_, v)| v.is_type(value_type, lookup))
                }
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::List(baml_value_with_metas, _) => match field_type {
                TypeGeneric::List(item_type, _) => baml_value_with_metas
                    .iter()
                    .all(|v| v.is_type(item_type, lookup)),
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Media(baml_media, _) => match field_type {
                TypeGeneric::Primitive(TypeValue::Media(media_type), _) => {
                    &baml_media.media_type == media_type
                }
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Enum(enum_name, _, _) => match field_type {
                TypeGeneric::Enum { name: enm, .. } => enum_name.as_str() == enm,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Class(cls_name, _cls_fields, _) => match field_type {
                TypeGeneric::Class { name: cls, .. } => cls_name.as_str() == cls,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Null(_) => match field_type {
                TypeGeneric::Primitive(TypeValue::Null, _) => true,
                _ => handle_composite(field_type),
            },
        }
    }
}

impl<T: HasType<type_meta::NonStreaming>> TypeQuery<type_meta::NonStreaming>
    for BamlValueWithMeta<T>
{
    /// Given a value and a union, picks the actual type of the value.
    ///
    /// For example, if the value is a union of `int` and `string`, and the value is an `int`,
    /// this will return `int`.
    ///
    /// If the value is a union of `int` and `string`, and the value is a `string`,
    /// this will return `string`.
    fn real_type(&self, lookup: &impl TypeLookups) -> TypeNonStreaming {
        let field_type = self.field_type();

        let mut field_type = field_type.clone();
        while let TypeGeneric::RecursiveTypeAlias { name, .. } = field_type {
            field_type = lookup
                .expand_recursive_type(name.as_str())
                .expect("Recursive type alias not found")
                .to_non_streaming_type(lookup);
        }
        let field_type = field_type;

        if let TypeGeneric::Union(options, _) = field_type {
            let field_type = match options.view() {
                UnionTypeViewGeneric::Null => TypeGeneric::null(),
                UnionTypeViewGeneric::Optional(field_type) => {
                    if self.is_type(field_type, lookup) {
                        field_type.clone()
                    } else {
                        TypeGeneric::null()
                    }
                }
                UnionTypeViewGeneric::OneOf(field_types) => field_types
                    .into_iter()
                    .find(|t| self.is_type(t, lookup))
                    .expect("At least one type must be supported")
                    .clone(),
                UnionTypeViewGeneric::OneOfOptional(field_types) => field_types
                    .into_iter()
                    .find(|t| self.is_type(t, lookup))
                    .map_or_else(TypeGeneric::null, |t| t.clone()),
            };
            if &field_type == self.field_type() {
                return field_type.clone();
            }
            if let TypeGeneric::RecursiveTypeAlias { name, .. } = field_type {
                let expanded_type = lookup
                    .expand_recursive_type(name.as_str())
                    .expect("Recursive type alias not found");
                return expanded_type.to_non_streaming_type(lookup);
            }
            return field_type;
        }
        field_type.clone()
    }

    // TODO: This will fail for type aliases?
    fn is_type(&self, field_type: &TypeNonStreaming, lookup: &impl TypeLookups) -> bool {
        let field_type = match field_type {
            TypeGeneric::RecursiveTypeAlias { name, .. } => &lookup
                .expand_recursive_type(name)
                .expect("Recursive type alias not found")
                .to_non_streaming_type(lookup),
            _ => field_type,
        };

        let handle_composite = |field_type: &TypeNonStreaming| match field_type {
            TypeGeneric::Union(options, _) => match options.view() {
                UnionTypeViewGeneric::Null => self.is_type(&TypeNonStreaming::null(), lookup),
                UnionTypeViewGeneric::Optional(field_type) => {
                    self.is_type(field_type, lookup)
                        || self.is_type(&TypeNonStreaming::null(), lookup)
                }
                UnionTypeViewGeneric::OneOf(field_types) => {
                    field_types.iter().any(|t| self.is_type(t, lookup))
                }
                UnionTypeViewGeneric::OneOfOptional(field_types) => {
                    field_types.iter().any(|t| self.is_type(t, lookup))
                        || self.is_type(&TypeNonStreaming::null(), lookup)
                }
            },
            _ => false,
        };

        match self {
            BamlValueWithMeta::String(val, _) => match field_type {
                TypeGeneric::Literal(LiteralValue::String(lit), _) => val.as_str() == lit,
                TypeGeneric::Primitive(TypeValue::String, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Int(val, _) => match field_type {
                TypeGeneric::Literal(LiteralValue::Int(lit), _) => val == lit,
                TypeGeneric::Primitive(TypeValue::Int, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Float(_, _) => match field_type {
                TypeGeneric::Primitive(TypeValue::Float, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Bool(val, _) => match field_type {
                TypeGeneric::Literal(LiteralValue::Bool(lit), _) => val == lit,
                TypeGeneric::Primitive(TypeValue::Bool, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Map(index_map, _) => match field_type {
                TypeGeneric::Map(_, value_type, _) => {
                    // TODO: Check key type
                    index_map.iter().all(|(_, v)| v.is_type(value_type, lookup))
                }
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::List(baml_value_with_metas, _) => match field_type {
                TypeGeneric::List(item_type, _) => baml_value_with_metas
                    .iter()
                    .all(|v| v.is_type(item_type, lookup)),
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Media(baml_media, _) => match field_type {
                TypeGeneric::Primitive(TypeValue::Media(media_type), _) => {
                    &baml_media.media_type == media_type
                }
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Enum(enum_name, _, _) => match field_type {
                TypeGeneric::Enum { name: enm, .. } => enum_name.as_str() == enm,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Class(cls_name, _cls_fields, _) => match field_type {
                TypeGeneric::Class { name: cls, .. } => cls_name.as_str() == cls,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Null(_) => match field_type {
                TypeGeneric::Primitive(TypeValue::Null, _) => true,
                _ => handle_composite(field_type),
            },
        }
    }
}

impl<T: HasType<type_meta::Streaming>> TypeQuery<type_meta::Streaming> for BamlValueWithMeta<T> {
    /// Given a value and a union, picks the actual type of the value.
    ///
    /// For example, if the value is a union of `int` and `string`, and the value is an `int`,
    /// this will return `int`.
    ///
    /// If the value is a union of `int` and `string`, and the value is a `string`,
    /// this will return `string`.
    fn real_type(&self, lookup: &impl TypeLookups) -> TypeStreaming {
        let field_type = self.field_type();

        let mut field_type = field_type.clone();
        while let TypeGeneric::RecursiveTypeAlias { name, .. } = field_type {
            field_type = lookup
                .expand_recursive_type(name.as_str())
                .expect("Recursive type alias not found")
                .to_streaming_type(lookup);
        }
        let field_type = field_type;

        if let TypeGeneric::Union(options, _) = field_type {
            let field_type = match options.view() {
                UnionTypeViewGeneric::Null => TypeGeneric::null(),
                UnionTypeViewGeneric::Optional(field_type) => {
                    if self.is_type(field_type, lookup) {
                        field_type.clone()
                    } else {
                        TypeGeneric::null()
                    }
                }
                UnionTypeViewGeneric::OneOf(field_types) => field_types
                    .into_iter()
                    .find(|t| self.is_type(t, lookup))
                    .expect("At least one type must be supported")
                    .clone(),
                UnionTypeViewGeneric::OneOfOptional(field_types) => field_types
                    .into_iter()
                    .find(|t| self.is_type(t, lookup))
                    .map_or_else(TypeGeneric::null, |t| t.clone()),
            };
            if &field_type == self.field_type() {
                return field_type.clone();
            }
            if let TypeGeneric::RecursiveTypeAlias { name, .. } = field_type {
                let expanded_type = lookup
                    .expand_recursive_type(name.as_str())
                    .expect("Recursive type alias not found");
                return expanded_type.to_streaming_type(lookup);
            }
            return field_type;
        }
        field_type.clone()
    }

    // TODO: This will fail for type aliases?
    fn is_type(&self, field_type: &TypeStreaming, lookup: &impl TypeLookups) -> bool {
        let field_type = match field_type {
            TypeGeneric::RecursiveTypeAlias { name, .. } => &lookup
                .expand_recursive_type(name)
                .expect("Recursive type alias not found")
                .to_streaming_type(lookup),
            _ => field_type,
        };

        let handle_composite = |field_type: &TypeStreaming| match field_type {
            TypeGeneric::Union(options, _) => match options.view() {
                UnionTypeViewGeneric::Null => self.is_type(&TypeStreaming::null(), lookup),
                UnionTypeViewGeneric::Optional(field_type) => {
                    self.is_type(field_type, lookup) || self.is_type(&TypeStreaming::null(), lookup)
                }
                UnionTypeViewGeneric::OneOf(field_types) => {
                    field_types.iter().any(|t| self.is_type(t, lookup))
                }
                UnionTypeViewGeneric::OneOfOptional(field_types) => {
                    field_types.iter().any(|t| self.is_type(t, lookup))
                        || self.is_type(&TypeStreaming::null(), lookup)
                }
            },
            _ => false,
        };

        match self {
            BamlValueWithMeta::String(val, _) => match field_type {
                TypeGeneric::Literal(LiteralValue::String(lit), _) => val.as_str() == lit,
                TypeGeneric::Primitive(TypeValue::String, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Int(val, _) => match field_type {
                TypeGeneric::Literal(LiteralValue::Int(lit), _) => val == lit,
                TypeGeneric::Primitive(TypeValue::Int, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Float(_, _) => match field_type {
                TypeGeneric::Primitive(TypeValue::Float, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Bool(val, _) => match field_type {
                TypeGeneric::Literal(LiteralValue::Bool(lit), _) => val == lit,
                TypeGeneric::Primitive(TypeValue::Bool, _) => true,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Map(index_map, _) => match field_type {
                TypeGeneric::Map(_, value_type, _) => {
                    // TODO: Check key type
                    index_map.iter().all(|(_, v)| v.is_type(value_type, lookup))
                }
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::List(baml_value_with_metas, _) => match field_type {
                TypeGeneric::List(item_type, _) => baml_value_with_metas
                    .iter()
                    .all(|v| v.is_type(item_type, lookup)),
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Media(baml_media, _) => match field_type {
                TypeGeneric::Primitive(TypeValue::Media(media_type), _) => {
                    &baml_media.media_type == media_type
                }
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Enum(enum_name, _, _) => match field_type {
                TypeGeneric::Enum { name: enm, .. } => enum_name.as_str() == enm,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Class(cls_name, _cls_fields, _) => match field_type {
                TypeGeneric::Class { name: cls, .. } => cls_name.as_str() == cls,
                _ => handle_composite(field_type),
            },
            BamlValueWithMeta::Null(_) => match field_type {
                TypeGeneric::Primitive(TypeValue::Null, _) => true,
                _ => handle_composite(field_type),
            },
        }
    }
}

impl<T, Meta> crate::HasType<Meta> for BamlValueWithMeta<T>
where
    T: HasType<Meta>,
{
    fn field_type(&self) -> &TypeGeneric<Meta> {
        self.meta().field_type()
    }
}

impl<T> BamlValueWithMeta<T> {
    pub fn r#type(&self) -> String {
        let plain_value: BamlValue = self.into();
        plain_value.r#type()
    }

    /// Convert this BamlValueWithMeta to a pretty printing document
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        match self {
            BamlValueWithMeta::Null(_) => RcDoc::text("null"),
            BamlValueWithMeta::Bool(b, _) => RcDoc::text(b.to_string()),
            BamlValueWithMeta::Int(i, _) => RcDoc::text(i.to_string()),
            BamlValueWithMeta::Float(f, _) => RcDoc::text(f.to_string()),
            BamlValueWithMeta::String(s, _) => RcDoc::text(format!("\"{}\"", escape_string(s))),
            BamlValueWithMeta::List(items, _) => {
                if items.is_empty() {
                    RcDoc::text("[]")
                } else {
                    RcDoc::text("[")
                        .append(RcDoc::softline())
                        .append(
                            RcDoc::intersperse(
                                items.iter().map(|item| item.to_doc()),
                                RcDoc::text(",").append(RcDoc::line()),
                            )
                            .nest(2),
                        )
                        .append(RcDoc::softline())
                        .append(RcDoc::text("]"))
                }
            }
            BamlValueWithMeta::Map(map, _) => {
                if map.is_empty() {
                    RcDoc::text("{}")
                } else {
                    RcDoc::text("{")
                        .append(RcDoc::softline())
                        .append(
                            RcDoc::intersperse(
                                map.iter().map(|(k, v)| {
                                    RcDoc::text(format!("\"{}\"", escape_string(k)))
                                        .append(RcDoc::text(":"))
                                        .append(RcDoc::space())
                                        .append(v.to_doc())
                                }),
                                RcDoc::text(",").append(RcDoc::line()),
                            )
                            .nest(2),
                        )
                        .append(RcDoc::softline())
                        .append(RcDoc::text("}"))
                }
            }
            BamlValueWithMeta::Media(media, _) => format_media(media),
            BamlValueWithMeta::Enum(enum_name, variant, _) => {
                RcDoc::text(format!("{enum_name}::{variant}"))
            }
            BamlValueWithMeta::Class(class_name, fields, _) => {
                if fields.is_empty() {
                    RcDoc::text(format!("{class_name} {{}}"))
                } else {
                    RcDoc::text(format!("{class_name} {{"))
                        .append(RcDoc::softline())
                        .append(
                            RcDoc::intersperse(
                                fields.iter().map(|(k, v)| {
                                    RcDoc::text(k.to_string())
                                        .append(RcDoc::text(":"))
                                        .append(RcDoc::space())
                                        .append(v.to_doc())
                                }),
                                RcDoc::text(",").append(RcDoc::line()),
                            )
                            .nest(2),
                        )
                        .append(RcDoc::softline())
                        .append(RcDoc::text("}"))
                }
            }
        }
    }

    /// Iterating over a `BamlValueWithMeta` produces a depth-first traversal
    /// of the value and all its children.
    pub fn iter(&self) -> BamlValueWithMetaIterator<'_, T> {
        BamlValueWithMetaIterator::new(self)
    }

    pub fn value(self) -> BamlValue {
        match self {
            BamlValueWithMeta::String(v, _) => BamlValue::String(v),
            BamlValueWithMeta::Int(v, _) => BamlValue::Int(v),
            BamlValueWithMeta::Float(v, _) => BamlValue::Float(v),
            BamlValueWithMeta::Bool(v, _) => BamlValue::Bool(v),
            BamlValueWithMeta::Map(v, _) => {
                BamlValue::Map(v.into_iter().map(|(k, v)| (k, v.value())).collect())
            }
            BamlValueWithMeta::List(v, _) => {
                BamlValue::List(v.into_iter().map(|v| v.value()).collect())
            }
            BamlValueWithMeta::Media(v, _) => BamlValue::Media(v),
            BamlValueWithMeta::Enum(v, w, _) => BamlValue::Enum(v, w),
            BamlValueWithMeta::Class(n, fs, _) => {
                BamlValue::Class(n, fs.into_iter().map(|(k, v)| (k, v.value())).collect())
            }
            BamlValueWithMeta::Null(_) => BamlValue::Null,
        }
    }

    pub fn value_clone(&self) -> BamlValue {
        match self {
            BamlValueWithMeta::String(v, _) => BamlValue::String(v.clone()),
            BamlValueWithMeta::Int(v, _) => BamlValue::Int(*v),
            BamlValueWithMeta::Float(v, _) => BamlValue::Float(*v),
            BamlValueWithMeta::Bool(v, _) => BamlValue::Bool(*v),
            BamlValueWithMeta::Map(v, _) => BamlValue::Map(
                v.into_iter()
                    .map(|(k, v)| (k.clone(), v.value_clone()))
                    .collect(),
            ),
            BamlValueWithMeta::List(v, _) => {
                BamlValue::List(v.iter().map(|v| v.value_clone()).collect())
            }
            BamlValueWithMeta::Media(v, _) => BamlValue::Media(v.clone()),
            BamlValueWithMeta::Enum(v, w, _) => BamlValue::Enum(v.clone(), w.clone()),
            BamlValueWithMeta::Class(n, fs, _) => BamlValue::Class(
                n.clone(),
                fs.into_iter()
                    .map(|(k, v)| (k.clone(), v.value_clone()))
                    .collect(),
            ),
            BamlValueWithMeta::Null(_) => BamlValue::Null,
        }
    }

    pub fn meta(&self) -> &T {
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

    pub fn meta_mut(&mut self) -> &mut T {
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

    pub fn with_default_meta(value: &BamlValue) -> BamlValueWithMeta<T>
    where
        T: From<TypeIR> + HasType<type_meta::IR>,
    {
        use BamlValueWithMeta::*;
        match value {
            BamlValue::String(s) => String(s.clone(), T::from(TypeIR::string())),
            BamlValue::Int(i) => Int(*i, T::from(TypeIR::int())),
            BamlValue::Float(f) => Float(*f, T::from(TypeIR::float())),
            BamlValue::Bool(b) => Bool(*b, T::from(TypeIR::bool())),
            BamlValue::Map(entries) => {
                let entries: BamlMap<std::string::String, BamlValueWithMeta<T>> = entries
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::with_default_meta(v)))
                    .collect();
                let value_types = entries.values().map(|v| v.field_type()).collect::<Vec<_>>();
                let field_type =
                    TypeIR::union(value_types.into_iter().map(|v| v.to_owned()).collect());

                Map(entries, T::from(field_type.simplify()))
            }
            BamlValue::List(items) => {
                let items: Vec<BamlValueWithMeta<T>> =
                    items.iter().map(|i| Self::with_default_meta(i)).collect();
                let items_types = items.iter().map(|i| i.field_type()).collect::<Vec<_>>();
                let field_type =
                    TypeIR::union(items_types.into_iter().map(|v| v.to_owned()).collect());
                List(items, T::from(field_type.simplify()))
            }
            BamlValue::Media(m) => Media(
                m.clone(),
                T::from(match m.media_type {
                    BamlMediaType::Image => TypeIR::image(),
                    BamlMediaType::Audio => TypeIR::audio(),
                    BamlMediaType::Pdf => TypeIR::pdf(),
                    BamlMediaType::Video => TypeIR::video(),
                }),
            ),
            BamlValue::Enum(n, v) => Enum(n.clone(), v.clone(), T::from(TypeIR::r#enum(n))),
            BamlValue::Class(name, items) => {
                let items: BamlMap<std::string::String, BamlValueWithMeta<T>> = items
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::with_default_meta(v)))
                    .collect();

                Class(name.clone(), items, T::from(TypeIR::class(name)))
            }
            BamlValue::Null => Null(T::from(TypeIR::null())),
        }
    }

    /// Apply the same meta value to every node throughout a BamlValue.
    pub fn with_same_meta_at_all_nodes(value: &BamlValue, meta: T) -> BamlValueWithMeta<T>
    where
        T: Clone,
    {
        use BamlValueWithMeta::*;
        match value {
            BamlValue::String(s) => String(s.clone(), meta),
            BamlValue::Int(i) => Int(*i, meta),
            BamlValue::Float(f) => Float(*f, meta),
            BamlValue::Bool(b) => Bool(*b, meta),
            BamlValue::Map(entries) => BamlValueWithMeta::Map(
                entries
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            Self::with_same_meta_at_all_nodes(v, meta.clone()),
                        )
                    })
                    .collect(),
                meta,
            ),
            BamlValue::List(items) => List(
                items
                    .iter()
                    .map(|i| Self::with_same_meta_at_all_nodes(i, meta.clone()))
                    .collect(),
                meta,
            ),
            BamlValue::Media(m) => Media(m.clone(), meta),
            BamlValue::Enum(n, v) => Enum(n.clone(), v.clone(), meta),
            BamlValue::Class(class_name, items) => BamlValueWithMeta::Class(
                class_name.clone(),
                items
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            Self::with_same_meta_at_all_nodes(v, meta.clone()),
                        )
                    })
                    .collect(),
                meta,
            ),
            BamlValue::Null => Null(meta),
        }
    }

    pub fn map_meta<'a, F, U>(&'a self, f: F) -> BamlValueWithMeta<U>
    where
        F: Fn(&'a T) -> U + Copy,
    {
        match self {
            BamlValueWithMeta::String(v, m) => BamlValueWithMeta::String(v.clone(), f(m)),
            BamlValueWithMeta::Int(v, m) => BamlValueWithMeta::Int(*v, f(m)),
            BamlValueWithMeta::Float(v, m) => BamlValueWithMeta::Float(*v, f(m)),
            BamlValueWithMeta::Bool(v, m) => BamlValueWithMeta::Bool(*v, f(m)),
            BamlValueWithMeta::Map(v, m) => BamlValueWithMeta::Map(
                v.iter().map(|(k, v)| (k.clone(), v.map_meta(f))).collect(),
                f(m),
            ),
            BamlValueWithMeta::List(v, m) => {
                BamlValueWithMeta::List(v.iter().map(|v| v.map_meta(f)).collect(), f(m))
            }
            BamlValueWithMeta::Media(v, m) => BamlValueWithMeta::Media(v.clone(), f(m)),
            BamlValueWithMeta::Enum(v, e, m) => BamlValueWithMeta::Enum(v.clone(), e.clone(), f(m)),
            BamlValueWithMeta::Class(n, fs, m) => BamlValueWithMeta::Class(
                n.clone(),
                fs.into_iter()
                    .map(|(k, v)| (k.clone(), v.map_meta(f)))
                    .collect(),
                f(m),
            ),
            BamlValueWithMeta::Null(m) => BamlValueWithMeta::Null(f(m)),
        }
    }

    pub fn map_meta_owned<F, U>(self, f: F) -> BamlValueWithMeta<U>
    where
        F: Fn(T) -> U + Copy,
    {
        match self {
            BamlValueWithMeta::String(v, m) => BamlValueWithMeta::String(v, f(m)),
            BamlValueWithMeta::Int(v, m) => BamlValueWithMeta::Int(v, f(m)),
            BamlValueWithMeta::Float(v, m) => BamlValueWithMeta::Float(v, f(m)),
            BamlValueWithMeta::Bool(v, m) => BamlValueWithMeta::Bool(v, f(m)),
            BamlValueWithMeta::Map(v, m) => BamlValueWithMeta::Map(
                v.into_iter()
                    .map(|(k, v)| (k, v.map_meta_owned(f)))
                    .collect(),
                f(m),
            ),
            BamlValueWithMeta::List(v, m) => {
                BamlValueWithMeta::List(v.into_iter().map(|v| v.map_meta_owned(f)).collect(), f(m))
            }
            BamlValueWithMeta::Media(v, m) => BamlValueWithMeta::Media(v, f(m)),
            BamlValueWithMeta::Enum(v, e, m) => BamlValueWithMeta::Enum(v, e, f(m)),
            BamlValueWithMeta::Class(n, fs, m) => BamlValueWithMeta::Class(
                n,
                fs.into_iter()
                    .map(|(k, v)| (k, v.map_meta_owned(f)))
                    .collect(),
                f(m),
            ),
            BamlValueWithMeta::Null(m) => BamlValueWithMeta::Null(f(m)),
        }
    }

    /// Combine two similar shaped baml values by tupling their metadata
    /// on a node-by-node basis.
    ///
    /// The baml value calling `zip_meta` is the "primary" one, whose value
    /// data will live on in the returned baml value.
    pub fn zip_meta<U: Clone + std::fmt::Debug>(
        self,
        other: &BamlValueWithMeta<U>,
    ) -> Result<BamlValueWithMeta<(T, U)>>
    where
        T: std::fmt::Debug,
    {
        let other_meta: U = other.meta().clone();
        let error_msg = String::new();
        let ret = match (self, other) {
            (BamlValueWithMeta::Null(meta1), _) => {
                Result::<_, _>::Ok(BamlValueWithMeta::Null((meta1, other_meta)))
            }
            (BamlValueWithMeta::String(s1, meta1), BamlValueWithMeta::String(_s2, _)) if true => {
                Ok(BamlValueWithMeta::String(s1, (meta1, other_meta)))
            }
            (BamlValueWithMeta::String(_, _), _) => anyhow::bail!("Unification error"),
            (BamlValueWithMeta::Int(s1, meta1), BamlValueWithMeta::Int(_s2, _)) if true => {
                Ok(BamlValueWithMeta::Int(s1, (meta1, other_meta)))
            }
            (BamlValueWithMeta::Int(_, _), _) => anyhow::bail!("Unification error"),
            (BamlValueWithMeta::Float(s1, meta1), BamlValueWithMeta::Float(_s2, _)) if true => {
                Ok(BamlValueWithMeta::Float(s1, (meta1, other_meta)))
            }
            (BamlValueWithMeta::Float(_, _), _) => anyhow::bail!("Unification error"),
            (BamlValueWithMeta::Bool(s1, meta1), BamlValueWithMeta::Bool(_s2, _)) if true => {
                Ok(BamlValueWithMeta::Bool(s1, (meta1, other_meta)))
            }
            (BamlValueWithMeta::Bool(_, _), _) => anyhow::bail!("Unification error"),
            (BamlValueWithMeta::Map(s1, meta1), BamlValueWithMeta::Map(s2, _)) => {
                let map_result = s1
                    .into_iter()
                    .zip(s2)
                    .map(|((k1, v1), (_k2, v2))| v1.zip_meta(v2).map(|res| (k1, res)))
                    .collect::<Result<IndexMap<_, _>>>()?;
                Ok(BamlValueWithMeta::Map(map_result, (meta1, other_meta)))
            }
            (BamlValueWithMeta::Map(_, _), _) => anyhow::bail!("Unification error"),
            (BamlValueWithMeta::List(l1, meta1), BamlValueWithMeta::List(l2, _)) => {
                let list_result = l1
                    .into_iter()
                    .zip(l2)
                    .map(|(item1, item2)| item1.zip_meta(item2))
                    .collect::<Result<Vec<_>>>()?;
                Ok(BamlValueWithMeta::List(list_result, (meta1, other_meta)))
            }
            (BamlValueWithMeta::List(_, _), _) => anyhow::bail!("Unification error"),
            (BamlValueWithMeta::Media(m1, meta1), BamlValueWithMeta::Media(_m2, _)) if true => {
                Ok(BamlValueWithMeta::Media(m1, (meta1, other_meta)))
            }
            (BamlValueWithMeta::Media(_, _), _) => anyhow::bail!("Unification error"),
            (BamlValueWithMeta::Enum(x1, y1, meta1), BamlValueWithMeta::Enum(_x2, _y2, _))
                if true =>
            {
                Ok(BamlValueWithMeta::Enum(x1, y1, (meta1, other_meta)))
            }
            (BamlValueWithMeta::Enum(_, _, _), _) => anyhow::bail!("Unification error"),
            (
                BamlValueWithMeta::Class(name1, fields1, meta1),
                BamlValueWithMeta::Class(_name2, fields2, _),
            ) if true => {
                // TODO: We can remove a `clone` by checking that the fields
                // are ordered the same way between the two classes, then consuming
                // both classs' fields in parallel.
                // let map_result = fields1.into_iter().zip(fields2).map(|((k1,v1),(_k2,v2))| {
                //     v1.zip_meta(v2).map(|r| (k1, r))
                // }).collect::<Result<IndexMap<_,_>>>()?;

                // Only zip fields that exist in both. Fields that exist only in fields1
                // (e.g., @skip fields added as null fillers by semantic streaming) are
                // dropped because they don't have corresponding metadata in fields2.
                // These fields will be re-added with proper null values when converting
                // to the final response type.
                let map_result = fields1
                    .into_iter()
                    .filter_map(|(k1, v1)| {
                        // Skip fields only in fields1 (e.g., @skip fields)
                        fields2.get(&k1).map(|v2| v1.zip_meta(v2).map(|r| (k1, r)))
                    })
                    .collect::<Result<IndexMap<_, _>>>()?;
                Ok(BamlValueWithMeta::Class(
                    name1,
                    map_result,
                    (meta1, other_meta),
                ))
            }
            (BamlValueWithMeta::Class(_, _, _), _) => anyhow::bail!("Unification error"),
        };
        ret.map_err(|_: anyhow::Error| anyhow::anyhow!(error_msg))
    }
}

/// An iterator over a BamlValue and all of its sub-values.
/// It yields entries in depth-first order.
pub struct BamlValueWithMetaIterator<'a, T> {
    stack: VecDeque<&'a BamlValueWithMeta<T>>,
}

impl<'a, T> BamlValueWithMetaIterator<'a, T> {
    /// Construct a new iterator. Users should do this via
    /// `.iter()` on a `BamlValueWithMeta` value.
    fn new(root: &'a BamlValueWithMeta<T>) -> Self {
        let mut stack = VecDeque::new();
        stack.push_back(root);
        BamlValueWithMetaIterator { stack }
    }
}

impl<'a, T: 'a> Iterator for BamlValueWithMetaIterator<'a, T> {
    type Item = &'a BamlValueWithMeta<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(value) = self.stack.pop_back() {
            // Get all the children and push them onto the stack.
            match value {
                BamlValueWithMeta::List(items, _) => {
                    self.stack.extend(items);
                }
                BamlValueWithMeta::Map(fields, _) => {
                    for (_, v) in fields.iter() {
                        self.stack.push_back(v);
                    }
                }
                BamlValueWithMeta::Class(_, fields, _) => {
                    for (_, v) in fields.iter() {
                        self.stack.push_back(v);
                    }
                }
                // These items have to children.
                BamlValueWithMeta::String(..)
                | BamlValueWithMeta::Int(..)
                | BamlValueWithMeta::Float(..)
                | BamlValueWithMeta::Bool(..)
                | BamlValueWithMeta::Media(..)
                | BamlValueWithMeta::Enum(..)
                | BamlValueWithMeta::Null(..) => {}
            }
            Some(value)
        } else {
            None
        }
    }
}

// Boilerplate.
impl<'a, T: 'a> IntoIterator for &'a BamlValueWithMeta<T> {
    type Item = &'a BamlValueWithMeta<T>;
    type IntoIter = BamlValueWithMetaIterator<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T> From<&BamlValueWithMeta<T>> for BamlValue {
    fn from(baml_value: &BamlValueWithMeta<T>) -> BamlValue {
        use BamlValueWithMeta::*;
        match baml_value {
            String(v, _) => BamlValue::String(v.clone()),
            Int(v, _) => BamlValue::Int(*v),
            Float(v, _) => BamlValue::Float(*v),
            Bool(v, _) => BamlValue::Bool(*v),
            Map(v, _) => {
                BamlValue::Map(v.into_iter().map(|(k, v)| (k.clone(), v.into())).collect())
            }
            List(v, _) => BamlValue::List(v.iter().map(|v| v.into()).collect()),
            Media(v, _) => BamlValue::Media(v.clone()),
            Enum(enum_name, v, _) => BamlValue::Enum(enum_name.clone(), v.clone()),
            Class(class_name, v, _) => BamlValue::Class(
                class_name.clone(),
                v.into_iter().map(|(k, v)| (k.clone(), v.into())).collect(),
            ),
            Null(_) => BamlValue::Null,
        }
    }
}

impl<T> From<BamlValueWithMeta<T>> for BamlValue {
    fn from(baml_value: BamlValueWithMeta<T>) -> BamlValue {
        use BamlValueWithMeta::*;
        match baml_value {
            String(v, _) => BamlValue::String(v),
            Int(v, _) => BamlValue::Int(v),
            Float(v, _) => BamlValue::Float(v),
            Bool(v, _) => BamlValue::Bool(v),
            Map(v, _) => BamlValue::Map(v.into_iter().map(|(k, v)| (k, v.into())).collect()),
            List(v, _) => BamlValue::List(v.into_iter().map(|v| v.into()).collect()),
            Media(v, _) => BamlValue::Media(v),
            Enum(enum_name, v, _) => BamlValue::Enum(enum_name, v),
            Class(class_name, v, _) => BamlValue::Class(
                class_name,
                v.into_iter().map(|(k, v)| (k, v.into())).collect(),
            ),
            Null(_) => BamlValue::Null,
        }
    }
}

/// This special-purpose serializer is used for jinja.
/// When we want to extend the orchestrator with BamlValues packing more
/// metadata than just a `Vec<ResponseCheck>`, `
impl Serialize for BamlValueWithMeta<Vec<ResponseCheck>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            BamlValueWithMeta::String(v, cr) => serialize_with_checks(v, cr, serializer),
            BamlValueWithMeta::Int(v, cr) => serialize_with_checks(v, cr, serializer),
            BamlValueWithMeta::Float(v, cr) => serialize_with_checks(v, cr, serializer),
            BamlValueWithMeta::Bool(v, cr) => serialize_with_checks(v, cr, serializer),
            BamlValueWithMeta::Map(v, cr) => {
                let mut map = serializer.serialize_map(None)?;
                for (key, value) in v {
                    map.serialize_entry(key, value)?;
                }
                add_checks(&mut map, cr)?;
                map.end()
            }
            BamlValueWithMeta::List(v, cr) => serialize_with_checks(v, cr, serializer),
            BamlValueWithMeta::Media(v, cr) => serialize_with_checks(v, cr, serializer),
            BamlValueWithMeta::Enum(_enum_name, v, cr) => serialize_with_checks(v, cr, serializer),
            BamlValueWithMeta::Class(_class_name, v, cr) => {
                if cr.is_empty() {
                    let mut map = serializer.serialize_map(None)?;
                    for (key, value) in v {
                        map.serialize_entry(key, value)?;
                    }
                    add_checks(&mut map, cr)?;
                    map.end()
                } else {
                    let mut checked_value = serializer.serialize_map(Some(2))?;
                    checked_value.serialize_entry("value", &v)?;
                    add_checks(&mut checked_value, cr)?;
                    checked_value.end()
                }
            }
            BamlValueWithMeta::Null(cr) => serialize_with_checks(&(), cr, serializer),
        }
    }
}

fn serialize_with_checks<S, T: Serialize>(
    value: &T,
    checks: &[ResponseCheck],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if !checks.is_empty() {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("value", value)?;
        add_checks(&mut map, checks)?;
        map.end()
    } else {
        value.serialize(serializer)
    }
}

fn add_checks<'a, S: SerializeMap>(
    map: &'a mut S,
    checks: &'a [ResponseCheck],
) -> Result<(), S::Error> {
    if !checks.is_empty() {
        let checks_map: BamlMap<_, _> = checks
            .iter()
            .map(|check| (check.name.clone(), check))
            .collect();
        map.serialize_entry("checks", &checks_map)?;
    }
    Ok(())
}

fn format_media(media: &BamlMedia) -> RcDoc<'static, ()> {
    match &media.content {
        crate::BamlMediaContent::Url(url) => {
            RcDoc::text(format!("<media url {}: {}>", media.media_type, url.url))
        }
        crate::BamlMediaContent::Base64(base64) => {
            let preview = if base64.base64.len() > 50 {
                format!("{}...", &base64.base64[..50])
            } else {
                base64.base64.clone()
            };
            RcDoc::text(format!("<media base64 {}: {}>", media.media_type, preview))
        }
        crate::BamlMediaContent::File(file) => match file.path() {
            Ok(path) => RcDoc::text(format!(
                "<media file {}: {}>",
                media.media_type,
                path.display()
            )),
            Err(_) => RcDoc::text(format!(
                "<media file {}: {}>",
                media.media_type,
                file.relpath.display()
            )),
        },
    }
}

fn escape_string(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '"' => "\\\"".to_string(),
            '\\' => "\\\\".to_string(),
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            c if c.is_control() => format!("\\u{:04x}", c as u32),
            c => c.to_string(),
        })
        .collect()
}

/// This type is used in `BamlResponseValue` to summarize data about the
/// completion state and completion behavior of a BamlValueWithMeta node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Completion {
    pub state: CompletionState,
    pub display: bool,
    pub required_done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, strum::Display)]
pub enum CompletionState {
    Pending,
    Incomplete,
    Complete,
}

impl Default for Completion {
    fn default() -> Self {
        Completion {
            state: CompletionState::Complete,
            display: false,
            required_done: false,
        }
    }
}
