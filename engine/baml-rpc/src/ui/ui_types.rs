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

    pub fn from_string(s: String) -> Self {
        UiFunctionIdString(s)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
#[ts(export)]
#[serde(tag = "type")]
pub enum UiBamlFunctionCallError {
    #[serde(rename = "BamlExternalException")]
    ExternalException { message: String },
    #[serde(rename = "BamlInternalException")]
    InternalException { message: String },
    #[serde(rename = "BamlError")]
    Base { message: String },
    #[serde(rename = "BamlInvalidArgumentError")]
    InvalidArgument { message: String },
    #[serde(rename = "BamlClientError")]
    Client { message: String },
    #[serde(rename = "BamlClientHttpError")]
    ClientHttp { message: String, status_code: i32 },
    #[serde(rename = "BamlClientFinishReasonError")]
    ClientFinishReason {
        finish_reason: String,
        message: String,
        prompt: String,
        raw_output: String,
    },
    #[serde(rename = "BamlValidationError")]
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

    #[ts(type = "Record<string, unknown>")]
    pub tags: serde_json::Map<String, serde_json::Value>,

    #[serde(rename = "start_epoch_ms")]
    #[ts(type = "number")]
    pub start_time: EpochMsTimestamp,
    #[serde(rename = "end_epoch_ms")]
    #[ts(type = "number | null")]
    pub end_time: Option<EpochMsTimestamp>,
    pub status: String,

    #[ts(type = "unknown")]
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
    #[ts(optional)]
    pub llm_request: Option<UiLlmRequest>,
    #[ts(optional)]
    pub llm_response: Option<UiLlmResponse>,
    #[ts(optional)]
    pub http_metadata_summary: Option<Vec<UiHttpMetadataSummary>>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UiLlmRequest {
    pub client_name: String,
    pub client_provider: String,
    // TODO: type this out properly.
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
    #[ts(type = "unknown")]
    pub prompt: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UiLlmResponse {
    pub client_stack: Vec<String>,
    pub model: Option<String>,
    pub finish_reason: Option<String>,
    pub usage: Option<UiUsageEstimate>,
    pub raw_text_output: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UiUsageEstimate {
    // TODO: these estimates are pulled straight from LLMResponse and
    // does not reflect the cost of failed retries.
    #[ts(type = "number | null")]
    pub input_tokens: Option<u64>,
    #[ts(type = "number | null")]
    pub output_tokens: Option<u64>,
    // Cost estimates calculated from provider-specific pricing
    #[ts(type = "number | null")]
    pub input_cost: Option<f64>,
    #[ts(type = "number | null")]
    pub output_cost: Option<f64>,
    #[ts(type = "number | null")]
    pub total_cost: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UiFunctionCallDetails {
    pub http_calls: Vec<UiHttpCall>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
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
    #[ts(type = "Record<string, string> | undefined")]
    pub headers: Option<HashMap<String, String>>,
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UiHttpMetadataSummary {
    pub client_name: String,
    pub client_provider: String,
    pub model: Option<String>,
    pub status: u16,
}

/// A single chunk from an SSE stream response, with its timestamp for ordering
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UiStreamChunk {
    /// Epoch milliseconds when this chunk was received
    #[serde(rename = "timestamp_epoch_ms")]
    #[ts(type = "number")]
    pub timestamp: EpochMsTimestamp,
    /// The raw SSE event data (e.g., `{"type":"content_block_delta",...}`)
    pub raw: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UiHttpResponse {
    #[serde(rename = "end_epoch_ms")]
    #[ts(type = "number")]
    pub end_time: EpochMsTimestamp,
    pub status_code: u16,
    #[ts(type = "Record<string, unknown>")]
    pub headers: HashMap<String, serde_json::Value>,
    /// For non-streaming responses, this contains the full body.
    /// For streaming responses, this is the concatenated stream chunks (for backwards compatibility).
    pub body: String,
    /// For streaming responses, ordered list of individual chunks with timestamps.
    /// This allows the UI to display events in the correct chronological order.
    #[ts(optional)]
    pub stream_chunks: Option<Vec<UiStreamChunk>>,
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
