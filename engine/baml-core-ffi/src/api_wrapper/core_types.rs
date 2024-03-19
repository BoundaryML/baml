use colored::*;
use std::{collections::HashMap, str::FromStr};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct UpdateTestCase {
  pub project_id: Option<String>,
  pub test_cycle_id: String,
  pub test_dataset_name: String,
  pub test_case_definition_name: String,
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[napi_derive::napi]
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
  pub latency_ms: i128,
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
pub enum Template {
  Single(String),
  Multiple(Vec<LLMChat>),
}

impl Default for Template {
  fn default() -> Self {
    Template::Single("".to_string())
  }
}

impl LLMEventInputPrompt {
  fn pretty_print(&self) -> String {
    match &self.template {
      Template::Single(s) => s.clone(),
      Template::Multiple(chats) => chats
        .iter()
        .map(|chat| {
          format!(
            "{} {}\n{}",
            "Role:".yellow(),
            chat.role.as_str().yellow(),
            chat.content
          )
        })
        .collect::<Vec<String>>()
        .join("\n"),
    }
  }
}

impl LogSchema {
  pub fn pretty_string(&self) -> Option<String> {
    match self.event_type {
      EventType::FuncLlm => {
        let log = self;

        let (llm_prompt, llm_raw_output) = if let Some(meta) = log.metadata.as_ref() {
          // TODO: Swap out template vars
          let input = match &meta.input.prompt.template {
            Template::Single(o) => o.clone(),
            Template::Multiple(chats) => chats
              .iter()
              .map(|c| {
                format!(
                  "{}:\n{}",
                  c.role.as_str().yellow().bold(),
                  c.content.white()
                )
              })
              .collect::<Vec<_>>()
              .join("\n"),
          };

          let mut colored_input = input.clone();
          meta.input.prompt.template_args.iter().for_each(|(k, v)| {
            let replacement = format!("{}", v.blue()); // Colorize the replacement text in magenta
            colored_input = colored_input.replace(k, &replacement);
          });

          let raw_output = meta.output.as_ref().map(|output| output.raw_text.clone());

          (Some(colored_input), raw_output)
        } else {
          (None, None)
        };

        let err = log.error.as_ref().map(|error| match &error.traceback {
          Some(traceback) => format!("{}\n{}", error.message, traceback),
          None => error.message.clone(),
        });

        let parsed_output = if let Some(output) = log.io.output.as_ref() {
          let r#type = match &output.r#type.name {
            TypeSchemaName::Single => {
              let fields = output
                .r#type
                .fields
                .iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join(", ");
              fields.to_string()
            }
            TypeSchemaName::Multi => {
              let fields = output
                .r#type
                .fields
                .iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join(", ");
              fields.to_string()
            }
          };
          let output = match &output.value {
            ValueType::String(s) => serde_json::Value::from_str(s),
            ValueType::List(l) => l
              .iter()
              .map(|v| serde_json::Value::from_str(v))
              .collect::<Result<Vec<_>, _>>()
              .map(serde_json::Value::Array),
          }
          .map(|v| {
            serde_json::to_string_pretty(&v)
              .unwrap_or_else(|_| format!("Failed to serialize output: {:?}", v))
          })
          .ok();
          match output {
            Some(output) => Some((output, r#type)),
            None => None,
          }
        } else {
          None
        };

        let res = match (llm_prompt, llm_raw_output, err, parsed_output) {
          (Some(llm_prompt), Some(llm_raw_output), Some(err), _) => vec![
            format!("\n{}", "---- Prompt ---------".dimmed()),
            format!("{}", llm_prompt),
            format!("\n{}", "---- Raw Response ---".dimmed()),
            format!("{}", llm_raw_output.white()),
            format!("{}", "----- Error -----".dimmed()),
            format!("{}", err.red()),
          ],
          (Some(llm_prompt), None, Some(err), _) => vec![
            format!("\n{}", "---- Prompt ---------".dimmed()),
            format!("{}", llm_prompt),
            format!("{}", "----- Error -----".dimmed()),
            format!("{}", err.red()),
          ],
          (Some(llm_prompt), Some(llm_raw_output), None, Some((output, output_type))) => {
            vec![
              format!("\n{}", "------- Prompt ------".yellow()),
              format!("{}", llm_prompt),
              format!("\n{}", "---- Raw Response ---".dimmed()),
              format!("{}", llm_raw_output.dimmed()),
              format!(
                "\n{}{}{}",
                "----- Parsed Response (".green(),
                output_type.green(),
                ") -----".green()
              ),
              format!("{}", output.green()),
            ]
          }
          _ => vec![],
        }
        .join("\n");

        Some(res)
      }
      _ => None,
    }
  }
}
