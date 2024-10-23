use std::{collections::HashMap, path::PathBuf};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Debug)]
#[allow(dead_code)]
pub(crate) struct UpdateTestCase {
    pub project_id: Option<String>,
    pub test_cycle_id: String,
    pub test_dataset_name: String,
    pub test_case_definition_name: String,
    pub test_case_arg_name: String,
    pub status: TestCaseStatus,
    pub error_data: Option<Value>, // Rust doesn't have a direct equivalent of Python's Any type, so we use serde_json::Value
}

#[derive(Serialize, Debug, Clone)]
pub struct LogSchema {
    pub project_id: Option<String>,
    pub event_type: EventType,
    pub root_event_id: String,
    pub event_id: String,
    pub parent_event_id: Option<String>,
    pub context: LogSchemaContext,
    pub io: IO,
    pub error: Option<Error>,
    pub metadata: Option<MetadataType>,
}

#[derive(Serialize, Debug, Clone)]
pub struct IO {
    pub(crate) input: Option<IOValue>,
    pub(crate) output: Option<IOValue>,
}

#[derive(Serialize, Debug, Clone)]
pub struct IOValue {
    pub(crate) value: ValueType,
    pub(crate) r#override: Option<HashMap<String, Value>>,
    pub(crate) r#type: TypeSchema,
}

#[derive(Serialize, Debug, Clone)]
pub struct TypeSchema {
    pub(crate) name: TypeSchemaName,
    pub(crate) fields: IndexMap<String, String>,
}

#[derive(Serialize, Debug, Clone)]
pub enum TypeSchemaName {
    #[serde(rename = "single")]
    Single,
    #[serde(rename = "multi")]
    Multi,
}

#[derive(Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum ValueType {
    String(String),
    // For multi-args, we use a list of strings
    List(Vec<String>),
}

//
// Supporting data structures for the above types
//

#[derive(Serialize, Debug, PartialEq, Eq, Clone, Copy)]
#[allow(dead_code)]
pub enum TestCaseStatus {
    #[serde(rename = "QUEUED")]
    Queued,
    #[serde(rename = "RUNNING")]
    Running,
    #[serde(rename = "PASSED")]
    Passed,
    #[serde(rename = "FAILED")]
    Failed,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "EXPECTED_FAILURE")]
    ExpectedFailure,
}

#[derive(Serialize, Debug, Clone)]
#[allow(dead_code)]
pub enum EventType {
    #[serde(rename = "log")]
    Log,
    #[serde(rename = "func_llm")]
    FuncLlm,
    #[serde(rename = "func_prob")]
    FuncProb,
    #[serde(rename = "func_code")]
    FuncCode,
}

#[derive(Serialize, Debug, Clone)]
pub struct LogSchemaContext {
    pub hostname: String,
    pub process_id: String,
    pub stage: Option<String>,
    pub latency_ms: i128,
    pub start_time: String,
    pub tags: HashMap<String, String>,
    pub event_chain: Vec<EventChain>,
}

#[derive(Serialize, Debug, Clone)]
pub struct EventChain {
    pub function_name: String,
    pub variant_name: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub(crate) struct Error {
    pub code: i32,
    pub message: String,
    pub traceback: Option<String>,
    pub r#override: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Debug, Deserialize, Default, Clone)]
pub struct LLMOutputModelMetadata {
    pub logprobs: Option<Value>,
    pub prompt_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub finish_reason: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct LLMOutputModel {
    pub raw_text: String,
    pub metadata: LLMOutputModelMetadata,
    pub r#override: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Debug, Clone)]
pub(crate) struct LLMChat {
    pub role: Role,
    pub content: Vec<ContentPart>,
}

#[derive(Serialize, Debug, Clone)]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text(String),
    #[serde(rename = "file_image")]
    FileImage(String, String),
    #[serde(rename = "url_image")]
    UrlImage(String),
    #[serde(rename = "b64_image")]
    B64Image(String),
    #[serde(rename = "file_audio")]
    FileAudio(String, String),
    #[serde(rename = "url_audio")]
    UrlAudio(String),
    #[serde(rename = "b64_audio")]
    B64Audio(String),
    #[serde(rename = "with_meta")]
    WithMeta(Box<ContentPart>, HashMap<String, Value>),
}

#[derive(Serialize, Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum Role {
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "system")]
    System,
    Other(String),
}

#[derive(Serialize, Debug, Clone)]
pub(crate) struct LLMEventInput {
    pub prompt: LLMEventInputPrompt,
    #[serde(rename = "invocation_params")]
    pub request_options: HashMap<String, Value>,
}

#[derive(Serialize, Debug, Clone)]
pub struct LLMEventSchema {
    pub model_name: String,
    pub provider: String,
    pub input: LLMEventInput,
    pub output: Option<LLMOutputModel>,
    pub error: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum MetadataType {
    #[allow(dead_code)]
    Single(LLMEventSchema),
    Multi(Vec<LLMEventSchema>),
}
#[derive(Serialize, Debug, Clone)]
pub struct LLMEventInputPrompt {
    pub template: Template,
    pub template_args: HashMap<String, String>,
    pub r#override: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(untagged)]
#[allow(dead_code)]
pub enum Template {
    Single(String),
    Multiple(Vec<LLMChat>),
}
