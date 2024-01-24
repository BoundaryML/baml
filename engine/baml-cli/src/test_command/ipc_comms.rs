use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub(crate) fn handle_message(message: &str) -> Option<MessageData> {
    if message.is_empty() {
        return None;
    }
    // Parse the message as json
    match serde_json::from_str::<MessageData>(message) {
        Ok(data) => Some(data),
        Err(e) => {
            log::error!("Failed to parse message: {}\n{}", e, message);
            None
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "name", content = "data")]
pub(crate) enum MessageData {
    #[serde(rename = "test_url")]
    TestRunMeta(TestRunMeta),
    #[serde(rename = "update_test_case")]
    UpdateTestCase(UpdateTestCase),
    #[serde(rename = "log")]
    Log(LogSchema),
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct TestRunMeta {
    pub dashboard_url: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct UpdateTestCase {
    project_id: String,
    test_cycle_id: String,
    test_dataset_name: String,
    test_case_definition_name: String,
    pub test_case_arg_name: String,
    pub status: TestCaseStatus,
    pub error_data: Option<Value>, // Rust doesn't have a direct equivalent of Python's Any type, so we use serde_json::Value
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LogSchema {
    project_id: String,
    event_type: EventType,
    root_event_id: String,
    event_id: String,
    parent_event_id: Option<String>,
    pub context: LogSchemaContext,
    pub io: IO,
    pub error: Option<Error>,
    pub metadata: Option<MetadataType>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct IO {
    pub(crate) input: Option<IOValue>,
    pub(crate) output: Option<IOValue>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct IOValue {
    pub(crate) value: ValueType,
    r#override: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Serialize, Deserialize, Debug)]
enum EventType {
    #[serde(rename = "log")]
    Log,
    #[serde(rename = "func_llm")]
    FuncLlm,
    #[serde(rename = "func_prob")]
    FuncProb,
    #[serde(rename = "func_code")]
    FuncCode,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LogSchemaContext {
    hostname: String,
    process_id: String,
    stage: Option<String>,
    pub latency_ms: i32,
    start_time: String,
    pub tags: HashMap<String, String>,
    pub event_chain: Vec<EventChain>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct EventChain {
    pub function_name: String,
    pub variant_name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Error {
    code: i32,
    pub message: String,
    pub traceback: Option<String>,
    r#override: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct LLMOutputModelMetadata {
    logprobs: Option<Value>,
    prompt_tokens: Option<i32>,
    output_tokens: Option<i32>,
    total_tokens: Option<i32>,
    finish_reason: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LLMOutputModel {
    pub raw_text: String,
    metadata: LLMOutputModelMetadata,
    r#override: Option<HashMap<String, Value>>,
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

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LLMEventInput {
    pub prompt: LLMEventInputPrompt,
    invocation_params: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LLMEventSchema {
    pub model_name: String,
    provider: String,
    pub input: LLMEventInput,
    pub output: Option<LLMOutputModel>,
}

type MetadataType = LLMEventSchema;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct LLMEventInputPrompt {
    pub template: Template,
    pub template_args: HashMap<String, String>,
    r#override: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum Template {
    Single(String),
    Multiple(Vec<LLMChat>),
}
