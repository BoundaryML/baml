use baml_ids::FunctionCallId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{base::EpochMsTimestamp, BamlFunctionId, BamlTypeId};

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TypeId(#[ts(type = "`${string}##${string}##${string}##${string}`")] BamlTypeId);

impl From<BamlTypeId> for TypeId {
    fn from(value: BamlTypeId) -> Self {
        TypeId(value)
    }
}

impl From<&BamlTypeId> for TypeId {
    fn from(value: &BamlTypeId) -> Self {
        TypeId(value.clone())
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FunctionId(#[ts(type = "`${string}##${string}##${string}##${string}`")] BamlFunctionId);

impl From<BamlFunctionId> for FunctionId {
    fn from(value: BamlFunctionId) -> Self {
        FunctionId(value)
    }
}

impl From<&BamlFunctionId> for FunctionId {
    fn from(value: &BamlFunctionId) -> Self {
        FunctionId(value.clone())
    }
}

impl FunctionId {
    pub fn inner(&self) -> &BamlFunctionId {
        &self.0
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FunctionCall {
    #[ts(type = "string")]
    pub function_call_id: FunctionCallId,
    pub function_name: String,
    pub source: String,
    #[ts(optional)]
    pub function_id: Option<FunctionId>,
    #[serde(rename = "start_epoch_ms")]
    #[ts(type = "number | null")]
    pub start_time: Option<EpochMsTimestamp>,
    #[serde(rename = "end_epoch_ms")]
    #[ts(type = "number | null")]
    pub end_time: Option<EpochMsTimestamp>,
    #[ts(type = "any")]
    pub baml_options: serde_json::Value,
    pub inputs: Vec<FunctionInput>,
    #[ts(type = "any")]
    pub output: serde_json::Value,
    pub status: String,
    #[ts(type = "any", optional)]
    pub error: Option<serde_json::Value>,
    #[ts(optional)]
    #[ts(type = "any | null")]
    pub tags: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct FunctionInput {
    pub field: String,
    #[ts(type = "any")]
    pub value: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct FunctionDefinition {
    pub function_id: FunctionId,
    pub inputs: Vec<NameTypeField>,
    pub output: TypeReference,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct TypeDefinition {
    pub r#type: String,
    pub type_id: TypeId,
    #[ts(optional)]
    pub fields: Option<Vec<NameTypeField>>,
    #[ts(optional)]
    pub values: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct NameTypeField {
    pub name: String,
    pub r#type: TypeReference,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TypeReference {
    Null,
    String,
    Int,
    Float,
    Bool,
    Media,
    Class {
        type_id: TypeId,
    },
    Enum {
        type_id: TypeId,
    },
    TypeAlias {
        type_id: TypeId,
    },
    Array {
        items: Box<TypeReference>,
    },
    Map {
        key: Box<TypeReference>,
        value: Box<TypeReference>,
    },
    Union {
        any_of: Vec<TypeReference>,
    },
    Literal(LiteralType),
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
#[serde(tag = "literal_type", content = "literal", rename_all = "snake_case")]
pub enum LiteralType {
    String(String),
    Int(i64),
    Bool(bool),
}
