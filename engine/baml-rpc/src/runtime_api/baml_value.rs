use std::borrow::Cow;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ast::type_reference::TypeReference;

#[derive(Debug, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum TypeIndex {
    #[default]
    NotUnion,
    Null,         // the type is a union but the value is null
    Index(usize), // the type is a union and this index points to the actual type
    NotFound,
}

fn is_default_type_index(t: &TypeIndex) -> bool {
    matches!(t, TypeIndex::NotUnion)
}

// Export this to TS since we don't yet decouple this into a DB specific type or anything. What the runtime exports is what the frontend reads as far as BamlValue is concerned. If you want to decouple it, create a UIBamlValue type and do a conversion from this to the UI type.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub struct BamlValue<'a> {
    pub metadata: ValueMetadata,
    pub value: ValueContent<'a>,
}

impl<'a> std::fmt::Display for BamlValue<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.value {
            ValueContent::Null => write!(f, "null"),
            ValueContent::String(s) => write!(f, "{s}"),
            ValueContent::Float(flt) => write!(f, "{flt}"),
            ValueContent::Int(i) => write!(f, "{i}"),
            ValueContent::Boolean(b) => write!(f, "{b}"),
            ValueContent::List(l) => write!(
                f,
                "[{}]",
                l.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ValueContent::Map(m) => write!(
                f,
                "{{{}}}",
                m.iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ValueContent::Class { fields } => write!(
                f,
                "{} {{{}}}",
                self.metadata.type_ref,
                fields
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ValueContent::Enum { value } => write!(f, "{} {}", value, self.metadata.type_ref),
            ValueContent::Media(_) => write!(f, "<media placeholder>"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CheckValue {
    Bool(bool),
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub struct ValueMetadata {
    pub type_ref: TypeReference,
    // if the value is a union, this index indicates which variant of the union it is
    // None -> Not a union
    // Some(None) -> Null
    // Some(Some(i)) -> i
    #[serde(default, skip_serializing_if = "is_default_type_index")]
    pub type_index: TypeIndex,

    // check_name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_results: Option<IndexMap<String, CheckValue>>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case", tag = "type", content = "data")]
pub enum ValueContent<'a> {
    Null,
    String(Cow<'a, str>),
    Float(f64),
    Int(i64),
    Boolean(bool),
    List(Vec<BamlValue<'a>>),
    Map(IndexMap<String, BamlValue<'a>>),
    // It's easier to query class with the classname.property1.property2.property3 in the DB
    // so we use a flattened map here.
    Class {
        #[serde(flatten)]
        fields: IndexMap<String, BamlValue<'a>>,
    },
    Enum {
        value: String,
    },
    Media(Media<'a>),
}

impl<'a> std::fmt::Display for ValueContent<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueContent::Null => write!(f, "null"),
            ValueContent::String(s) => write!(f, "{s}"),
            ValueContent::Float(flt) => write!(f, "{flt}"),
            ValueContent::Int(i) => write!(f, "{i}"),
            ValueContent::Boolean(b) => write!(f, "{b}"),
            ValueContent::List(l) => write!(
                f,
                "[{}]",
                l.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ValueContent::Map(m) => write!(
                f,
                "{{{}}}",
                m.iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ValueContent::Class { fields } => write!(
                f,
                "class {{{}}}",
                fields
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ValueContent::Enum { value } => write!(f, "enum {value}"),
            ValueContent::Media(_) => write!(f, "<media placeholder>"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub struct Media<'a> {
    pub mime_type: Option<String>,
    pub value: MediaValue<'a>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case", tag = "media_source", content = "data")]
pub enum MediaValue<'a> {
    Url(Cow<'a, str>),
    Base64(Cow<'a, str>),
    FilePath(Cow<'a, str>),
    BlobRef(Cow<'a, str>), // Hash reference to a blob stored in S3
}
