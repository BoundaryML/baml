use std::collections::HashMap;

use baml_ids::FunctionCallId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    base::EpochMsTimestamp, AstNodeId, BamlFunctionId, BamlTypeId, BamlValue, FunctionDefinition,
    FunctionType, NamedType, TypeDefinition, TypeDefinitionSource, TypeReference,
};

// READ
// THE GIST OF THESE TYPES IS THAT WE SIMPLIFY THE BAMLTYPEID to a string.
// But we actually reuse all the same AST structures from the runtime.
// So we don't have "UI*" equivalent types for all runtime types. We just annotate the actual runtimet ypes with the (TS) annotation to export those.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UiTypeIdString(#[ts(type = "`${string}##${string}##${string}##${string}`")] String);

impl From<BamlTypeId> for UiTypeIdString {
    fn from(value: BamlTypeId) -> Self {
        UiTypeIdString(value.0.to_string())
    }
}

impl From<&BamlTypeId> for UiTypeIdString {
    fn from(value: &BamlTypeId) -> Self {
        UiTypeIdString(value.0.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
// TODO: aaron, make this string
pub struct UiFunctionIdString(#[ts(type = "`${string}##${string}##${string}##${string}`")] String);

impl From<BamlFunctionId> for UiFunctionIdString {
    fn from(value: BamlFunctionId) -> Self {
        UiFunctionIdString(value.0.to_string())
    }
}

impl From<&BamlFunctionId> for UiFunctionIdString {
    fn from(value: &BamlFunctionId) -> Self {
        UiFunctionIdString(value.0.to_string())
    }
}

impl UiFunctionIdString {
    pub fn inner(&self) -> &String {
        &self.0
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiBamlFunctionCallError {
    ExternalException {
        message: String,
    },
    InternalException {
        message: String,
    },
    Base {
        message: String,
    },
    InvalidArgument {
        message: String,
    },
    Client {
        message: String,
    },
    ClientHttp {
        message: String,
        status_code: i32,
    },
    ClientFinishReason {
        finish_reason: String,
        message: String,
        prompt: String,
        raw_output: String,
    },
    Validation {
        raw_output: String,
        message: String,
        prompt: String,
    },
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UiFunctionCall {
    #[ts(type = "string")]
    pub function_call_id: FunctionCallId,
    pub function_name: String,
    pub function_type: FunctionType,
    pub is_stream: Option<bool>,
    #[ts(optional)]
    pub function_id: Option<UiFunctionIdString>,

    #[ts(type = "Record<string, any>")]
    pub tags: serde_json::Map<String, serde_json::Value>,

    #[serde(rename = "start_epoch_ms")]
    #[ts(type = "number | null")]
    pub start_time: Option<EpochMsTimestamp>,
    #[serde(rename = "end_epoch_ms")]
    #[ts(type = "number | null")]
    pub end_time: Option<EpochMsTimestamp>,
    pub status: String,

    #[ts(type = "any")]
    pub baml_options: serde_json::Value,
    pub inputs: Vec<UiFunctionInput>,
    #[ts(as = "Option<BamlValue>")]
    pub output: serde_json::Value,
    pub error: Option<UiBamlFunctionCallError>,

    pub is_root: bool,
    #[ts(type = "string | null")]
    pub root_function_call_id: Option<FunctionCallId>,
    pub usage_estimate: UiUsageEstimate,
    #[ts(optional)]
    pub details: Option<UiFunctionCallDetails>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UiUsageEstimate {
    // TODO: these estimates are pulled straight from LLMResponse and
    // does not reflect the cost of failed retries.
    #[ts(type = "number | null")]
    pub input_tokens: Option<u64>,
    #[ts(type = "number | null")]
    pub output_tokens: Option<u64>,
    // TODO: add cost estimate data here
    // This is tricky to do because we need provider & model to effectively
    // resolve the token costs. Even restricting to just openai is non-straightforward,
    // and frankly I'm skeptical that restricting to just openai is a sufficiently common
    // implementation use case.
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UiFunctionCallDetails {
    pub http_calls: Vec<UiHttpCall>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UiHttpCall {
    pub http_request: UiHttpRequest,
    pub http_response: Option<UiHttpResponse>,

    pub is_stream: bool,
    pub is_selected: bool,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UiHttpRequest {
    #[serde(rename = "start_epoch_ms")]
    #[ts(type = "number")]
    pub start_time: EpochMsTimestamp,
    pub url: String,
    pub method: String,
    #[ts(type = "Record<string, any>")]
    pub headers: HashMap<String, String>,
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UiHttpResponse {
    #[serde(rename = "end_epoch_ms")]
    #[ts(type = "number")]
    pub end_time: EpochMsTimestamp,
    pub status_code: u16,
    #[ts(type = "Record<string, any>")]
    pub headers: HashMap<String, serde_json::Value>,
    pub body: String,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct UiFunctionInput {
    pub field: String,
    // TODO: this is of type baml-rpc/src/runtime_api/baml_value.rs::BamlValue IIRC.
    // The reason why we dont yet add it in directly is because of the lifetime issues.
    #[ts(as = "BamlValue")]
    pub baml_value: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct UiFunctionDefinition {
    pub function_id: UiFunctionIdString,
    pub inputs: Vec<NamedType>,
    pub output: TypeReference,
}

// Matches the runtime TypeDefinition but replaces ids with strings instead of a struct.
#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct UiTypeDefinition {
    pub type_id: UiTypeIdString,
    #[serde(flatten)]
    pub definition: UiTypeDefinitionData,
}

// Nearly the same as baml-rpc/src/ast/type_definition.rs::TypeDefinition but replaces ids with strings instead of a struct, and moves the Id to the top level.
// These are the user-defined types in a baml_src.
// If you want to decouple some more you can add more UI* equivalent types here with different structure than the runtime. But you will need to do the translation work (and regenerate the ui types using cargo test)
#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", content = "data")]
#[ts(export)]
pub enum UiTypeDefinitionData {
    // User-defined types
    Enum {
        // Order matters!
        values: Vec<String>,
        source: TypeDefinitionSource,
        dependencies: Vec<AstNodeId>,
    },
    Class {
        // Order matters!
        fields: Vec<NamedType>,
        source: TypeDefinitionSource,
        dependencies: Vec<AstNodeId>,
    },
    Alias {
        rhs: TypeReference,
    },
}

// pub enum UiTypeDefinitionSource {
//     CompileTime,
//     Buildable,
//     PureBuildable,
// }

// #[derive(Debug, Deserialize, Serialize, TS)]
// #[ts(export)]
// pub struct UiNameTypeField {
//     pub name: String,
//     pub r#type: TypeReference,
// }

// #[derive(Debug, Deserialize, Serialize, TS)]
// #[ts(export)]
// #[serde(tag = "type", rename_all = "snake_case")]
// pub enum UiTypeReference {
//     Null,
//     String,
//     Int,
//     Float,
//     Bool,
//     Media,
//     Class {
//         type_id: UiTypeId,
//     },
//     Enum {
//         type_id: UiTypeId,
//     },
//     TypeAlias {
//         type_id: UiTypeId,
//     },
//     Array {
//         items: Box<UiTypeReference>,
//     },
//     Map {
//         key: Box<UiTypeReference>,
//         value: Box<UiTypeReference>,
//     },
//     Union {
//         any_of: Vec<UiTypeReference>,
//     },
//     Literal(LiteralType),
// }

// #[derive(Debug, Deserialize, Serialize, TS)]
// #[ts(export)]
// #[serde(tag = "literal_type", content = "literal", rename_all = "snake_case")]
// pub enum LiteralType {
//     String(String),
//     Int(i64),
//     Bool(bool),
// }

// Mappers

// from FunctionDefinition to UiFunctionDefinition
impl From<FunctionDefinition> for UiFunctionDefinition {
    fn from(value: FunctionDefinition) -> Self {
        UiFunctionDefinition {
            function_id: UiFunctionIdString(value.function_id.0.to_string()),
            inputs: value
                .inputs
                .into_iter()
                .map(|input| NamedType {
                    name: input.name,
                    type_ref: input.type_ref,
                })
                .collect(),
            output: value.output,
        }
    }
}

// from TypeDefinition to UiTypeDefinition
impl From<TypeDefinition> for UiTypeDefinition {
    fn from(value: TypeDefinition) -> Self {
        match value {
            TypeDefinition::Enum {
                type_id,
                values,
                source,
                dependencies,
            } => UiTypeDefinition {
                type_id: UiTypeIdString(type_id.0.to_string()),
                definition: UiTypeDefinitionData::Enum {
                    values,
                    source,
                    dependencies,
                },
            },
            TypeDefinition::Class {
                type_id,
                fields,
                source,
                dependencies,
            } => UiTypeDefinition {
                type_id: UiTypeIdString(type_id.0.to_string()),
                definition: UiTypeDefinitionData::Class {
                    fields,
                    source,
                    dependencies,
                },
            },
            TypeDefinition::Alias { type_id, rhs } => UiTypeDefinition {
                type_id: UiTypeIdString(type_id.0.to_string()),
                definition: UiTypeDefinitionData::Alias { rhs },
            },
        }
    }
}
