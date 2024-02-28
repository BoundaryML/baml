use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct UpdateTestCase {
    project_id: Option<String>,
    test_cycle_id: String,
    test_dataset_name: String,
    test_case_definition_name: String,
    pub test_case_arg_name: String,
    pub status: TestCaseStatus,
    pub error_data: Option<Value>, // Rust doesn't have a direct equivalent of Python's Any type, so we use serde_json::Value
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LogSchema {
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

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub(crate) struct IO {
    pub(crate) input: Option<IOValue>,
    pub(crate) output: Option<IOValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct IOValue {
    pub(crate) value: ValueType,
    pub(crate) r#override: Option<HashMap<String, Value>>,
    pub(crate) r#type: TypeSchema,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct TypeSchema {
    pub(crate) name: TypeSchemaName,
    pub(crate) fields: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) enum TypeSchemaName {
    #[serde(rename = "single")]
    Single,
    #[serde(rename = "multi")]
    Multi,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum ValueType {
    String(String),
    // For mutli-args, we use a list of strings
    List(Vec<String>),
}

//
// Supporting data structures for the above types
//

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum TestCaseStatus {
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

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub enum EventType {
    #[default]
    #[serde(rename = "log")]
    Log,
    #[serde(rename = "func_llm")]
    FuncLlm,
    #[serde(rename = "func_prob")]
    FuncProb,
    #[serde(rename = "func_code")]
    FuncCode,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub(crate) struct LogSchemaContext {
    pub hostname: String,
    pub process_id: String,
    pub stage: Option<String>,
    pub latency_ms: u64,
    pub start_time: String,
    pub tags: HashMap<String, String>,
    pub event_chain: Vec<EventChain>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct EventChain {
    pub function_name: String,
    pub variant_name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub(crate) struct Error {
    pub code: i32,
    pub message: String,
    pub traceback: Option<String>,
    pub r#override: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub(crate) struct LLMOutputModelMetadata {
    pub logprobs: Option<Value>,
    pub prompt_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub total_tokens: Option<i32>,
    pub finish_reason: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub(crate) struct LLMOutputModel {
    pub raw_text: String,
    pub metadata: LLMOutputModelMetadata,
    pub r#override: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LLMChat {
    pub role: Role,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum Role {
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "system")]
    System,
    Other(String),
}

impl Role {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Role::Assistant => "assistant",
            Role::User => "user",
            Role::System => "system",
            Role::Other(s) => s,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub(crate) struct LLMEventInput {
    pub prompt: LLMEventInputPrompt,
    pub invocation_params: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LLMEventSchema {
    pub model_name: String,
    pub provider: String,
    pub input: LLMEventInput,
    pub output: Option<LLMOutputModel>,
}

type MetadataType = LLMEventSchema;

#[derive(Serialize, Deserialize, Debug, Default)]
pub(crate) struct LLMEventInputPrompt {
    pub template: Template,
    pub template_args: HashMap<String, String>,
    pub r#override: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum Template {
    Single(String),
    Multiple(Vec<LLMChat>),
}

impl Default for Template {
    fn default() -> Self {
        Template::Single("".to_string())
    }
}

impl LogSchema {
    pub fn pretty_string(&self) -> Option<String> {
        match self.event_type {
            EventType::FuncLlm => {
                // Returns a colored string representation of the LogSchema
                todo!();
            }
            _ => None,
        }
    }
}
