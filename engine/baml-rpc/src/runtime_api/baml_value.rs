use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ast::type_reference::TypeReference;

// Export this to TS since we don't yet decouple this into a DB specific type or anything. What the runtime exports is what the frontend reads as far as BamlValue is concerned. If you want to decouple it, create a UIBamlValue type and do a conversion from this to the UI type.
// TODO: aaron: seems redudant that we have the 'type' information in both the type_ref and the value.type.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub struct BamlValue<'a> {
    pub type_ref: TypeReference,
    pub value: ValueContent<'a>,
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
    Map(Vec<(String, BamlValue<'a>)>),
    Class {
        fields: Vec<(String, BamlValue<'a>)>,
    },
    Enum {
        value: String,
    },
    Media(Media<'a>),
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
}
