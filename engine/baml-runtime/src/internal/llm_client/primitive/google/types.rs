use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GoogleRequestBody {
    pub contents: Vec<Content>,
    pub tools: Option<Vec<Tool>>,
    pub safety_settings: Option<SafetySetting>,
    pub generation_config: Option<GenerationConfig>,
    pub system_instruction: Option<Content>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub function_declarations: Option<Vec<FunctionDeclaration>>,
    pub retrieval: Option<Retrieval>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<Schema>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    pub schema_type: Type,
    pub format: String,
    pub title: String,
    pub description: String,
    pub nullable: bool,
    pub default: Option<Value>,
    pub items: Option<Box<Schema>>,
    pub min_items: Option<usize>,
    pub max_items: Option<usize>,
    pub enum_values: Option<Vec<String>>,
    pub properties: Option<HashMap<String, Schema>>,
    pub required: Option<Vec<String>>,
    pub min_properties: Option<usize>,
    pub max_properties: Option<usize>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub pattern: Option<String>,
    pub example: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Value {
    #[serde(rename = "NULL_VALUE")]
    Null,
    #[serde(rename = "NUMBER_VALUE")]
    Number(f64),
    #[serde(rename = "STRING_VALUE")]
    String(String),
    #[serde(rename = "BOOL_VALUE")]
    Bool(bool),
    #[serde(rename = "STRUCT_VALUE")]
    Struct(Struct),
    #[serde(rename = "LIST_VALUE")]
    List(Vec<Value>),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Type {
    #[serde(rename = "STRING")]
    String,
    #[serde(rename = "NUMBER")]
    Number,
    #[serde(rename = "INTEGER")]
    Integer,
    #[serde(rename = "BOOLEAN")]
    Boolean,
    #[serde(rename = "OBJECT")]
    Object,
    #[serde(rename = "ARRAY")]
    Array,
    #[serde(rename = "TYPE_UNSPECIFIED")]
    Unspecified,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Retrieval {
    pub disable_attribution: bool,
    pub vertex_ai_search: VertexAiSearch,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VertexAiSearch {
    pub datastore: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SafetySetting {
    pub category: Option<HarmCategory>,
    pub threshold: Option<HarmBlockThreshold>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum HarmBlockThreshold {
    #[serde(rename = "HARM_BLOCK_THRESHOLD_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "BLOCK_LOW_AND_ABOVE")]
    BlockLowAndAbove,
    #[serde(rename = "BLOCK_MEDIUM_AND_ABOVE")]
    BlockMediumAndAbove,
    #[serde(rename = "BLOCK_ONLY_HIGH")]
    BlockOnlyHigh,
    #[serde(rename = "BLOCK_NONE")]
    BlockNone,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    pub stop_sequences: Option<Vec<String>>,
    pub response_mime_type: Option<String>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<i32>,
    pub candidate_count: Option<i32>,
    pub max_output_tokens: Option<i32>,
    pub presence_penalty: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub response_schema: Option<Schema>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GoogleResponse {
    pub candidates: Vec<Candidate>,
    pub prompt_feedback: Option<PromptFeedback>,
    pub usage_metadata: UsageMetaData,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PromptFeedback {
    pub block_reason: BlockReason,
    pub safety_ratings: Vec<SafetyRating>,
    pub block_reason_message: String,
}

#[derive(Serialize, Deserialize, Debug, strum_macros::Display)]
pub enum BlockReason {
    #[serde(rename = "BLOCKED_REASON_UNSPECIFIED")]
    BlockedReasonUnspecified,
    #[serde(rename = "SAFETY")]
    Safety,
    #[serde(rename = "OTHER")]
    Other,
    #[serde(rename = "BLOCKLIST")]
    Blocklist,
    #[serde(rename = "PROHIBITED_CONTENT")]
    ProhibitedContent,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SafetyRating {
    pub category: HarmCategory,
    pub probability: HarmProbability,
    pub probability_score: Option<f64>,
    pub severity: Option<HarmSeverity>,
    pub severity_score: Option<f64>,
    pub blocked: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, strum_macros::Display)]
pub enum HarmCategory {
    #[serde(rename = "HARM_CATEGORY_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "HARM_CATEGORY_HATE_SPEECH")]
    HateSpeech,
    #[serde(rename = "HARM_CATEGORY_DANGEROUS_CONTENT")]
    DangerousContent,
    #[serde(rename = "HARM_CATEGORY_HARASSMENT")]
    Harassment,
    #[serde(rename = "HARM_CATEGORY_SEXUALLY_EXPLICIT")]
    SexuallyExplicit,
}

#[derive(Serialize, Deserialize, Debug, strum_macros::Display)]
pub enum HarmProbability {
    #[serde(rename = "HARM_PROBABILITY_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "NEGLIGIBLE")]
    Negligible,
    #[serde(rename = "LOW")]
    Low,
    #[serde(rename = "MEDIUM")]
    Medium,
    #[serde(rename = "HIGH")]
    High,
}

#[derive(Serialize, Deserialize, Debug, strum_macros::Display)]
pub enum HarmSeverity {
    #[serde(rename = "HARM_SEVERITY_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "HARM_SEVERITY_NEGLIGIBLE")]
    Negligible,
    #[serde(rename = "HARM_SEVERITY_LOW")]
    Low,
    #[serde(rename = "HARM_SEVERITY_MEDIUM")]
    Medium,
    #[serde(rename = "HARM_SEVERITY_HIGH")]
    High,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub index: Option<i32>,
    pub content: Option<Content>,
    pub finish_reason: Option<String>,
    pub safety_ratings: Option<Vec<SafetyRating>>,
    // pub citation_metadata: Option<CitationMetadata>,
    pub grounding_metadata: Option<GroundingMetadata>,
    pub finish_message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub role: Option<String>,
    #[serde(default)]
    pub parts: Vec<Part>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub text: String,
    pub inline_data: Option<Blob>,
    pub file_data: Option<FileData>,
    pub function_call: Option<FunctionCall>,
    pub function_response: Option<FunctionResponse>,
    pub video_metadata: Option<VideoMetadata>,
    pub thought: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Blob {
    pub mime_type: String,
    pub data: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileData {
    pub mime_type: String,
    pub file_uri: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FunctionCall {
    pub name: String,
    pub args: Option<Vec<Struct>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Struct {
    pub fields: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FunctionResponse {
    pub name: String,
    pub response: Option<Struct>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadata {
    pub start_offset: Option<Duration>,
    pub end_offset: Option<Duration>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Duration {
    pub seconds: i64,
    pub nanos: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CitationMetadata {
    // docs are wrong, this seems to be citationSources
    pub citations: Option<Vec<Citation>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub start_index: Option<i32>,
    pub end_index: Option<i32>,
    pub uri: Option<String>,
    pub title: Option<String>,
    pub license: Option<String>,
    pub publication_date: Option<Date>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Date {
    pub year: Option<i32>,
    pub month: Option<i32>,
    pub day: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GroundingMetadata {
    pub web_search_queries: Option<Vec<String>>,
    pub search_entry_point: Option<SearchEntryPoint>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchEntryPoint {
    pub rendered_content: Option<String>,
    pub sdk_blob: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetaData {
    pub prompt_token_count: Option<u64>,
    pub candidates_token_count: Option<u64>,
    pub total_token_count: Option<u64>,
    pub cached_content_token_count: Option<u64>,
}

#[cfg(test)]
mod tests {
    use serde_json::Error;

    use super::*;

    #[test]
    fn test_deserialization() {
        let data = r#"
        {
            "candidates": [
              {
                "content": {
                  "role": "model",
                  "parts": [
                    {
                      "text": "Dark fizz, cherry bright,\nTwenty-three flavors dance light,\nA Texan delight. \n"
                    }
                  ]
                },
                "finishReason": "STOP",
                "safetyRatings": [
                  {
                    "category": "HARM_CATEGORY_HATE_SPEECH",
                    "probability": "NEGLIGIBLE",
                    "probabilityScore": 0.04977345,
                    "severity": "HARM_SEVERITY_NEGLIGIBLE",
                    "severityScore": 0.06359858
                  },
                  {
                    "category": "HARM_CATEGORY_DANGEROUS_CONTENT",
                    "probability": "NEGLIGIBLE",
                    "probabilityScore": 0.06632687,
                    "severity": "HARM_SEVERITY_NEGLIGIBLE",
                    "severityScore": 0.103205055
                  },
                  {
                    "category": "HARM_CATEGORY_HARASSMENT",
                    "probability": "NEGLIGIBLE",
                    "probabilityScore": 0.06979492,
                    "severity": "HARM_SEVERITY_NEGLIGIBLE",
                    "severityScore": 0.058131594
                  },
                  {
                    "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT",
                    "probability": "NEGLIGIBLE",
                    "probabilityScore": 0.09285216,
                    "severity": "HARM_SEVERITY_NEGLIGIBLE",
                    "severityScore": 0.0992954
                  }
                ]
              }
            ],
            "usageMetadata": {
              "promptTokenCount": 8,
              "candidatesTokenCount": 21,
              "totalTokenCount": 29
            }
          }
        "#;

        let parsed: Result<GoogleResponse, Error> = serde_json::from_str(data);

        match parsed {
            Ok(response) => println!("Parsed successfully: {response:?}"),
            Err(e) => {
                println!("Failed to parse: {e}");
                println!("Error line: {}", e.line());
                println!("Error column: {}", e.column());
                println!("Error cause: {:?}", e.classify());
                panic!("Deserialization failed");
            }
        }
    }
}
