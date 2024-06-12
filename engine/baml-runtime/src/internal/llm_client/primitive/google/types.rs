use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    NullValue,
    #[serde(rename = "NUMBER_VALUE")]
    NumberValue(f64),
    #[serde(rename = "STRING_VALUE")]
    StringValue(String),
    #[serde(rename = "BOOL_VALUE")]
    BoolValue(bool),
    #[serde(rename = "STRUCT_VALUE")]
    StructValue(Struct),
    #[serde(rename = "LIST_VALUE")]
    ListValue(Vec<Value>),
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
    TypeUnspecified,
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
    pub category: HarmCategory,
    pub threshold: HarmBlockThreshold,
    pub method: HarmBlockMethod,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum HarmBlockThreshold {
    #[serde(rename = "HARM_BLOCK_THRESHOLD_UNSPECIFIED")]
    HarmBlockThresholdUnspecified,
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
pub enum HarmBlockMethod {
    #[serde(rename = "harm_block_method_unspecified")]
    HarmBlockMethodUnspecified,
    #[serde(rename = "severity")]
    Severity,
    #[serde(rename = "probability")]
    Probability,
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
    pub probability_score: f64,
    pub severity: HarmSeverity,
    pub severity_score: f64,
    pub blocked: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, strum_macros::Display)]
pub enum HarmCategory {
    #[serde(rename = "HARM_CATEGORY_UNSPECIFIED")]
    HarmCategoryUnspecified,
    #[serde(rename = "HARM_CATEGORY_HATE_SPEECH")]
    HarmCategoryHateSpeech,
    #[serde(rename = "HARM_CATEGORY_DANGEROUS_CONTENT")]
    HarmCategoryDangerousContent,
    #[serde(rename = "HARM_CATEGORY_HARASSMENT")]
    HarmCategoryHarassment,
    #[serde(rename = "HARM_CATEGORY_SEXUALLY_EXPLICIT")]
    HarmCategorySexuallyExplicit,
}

#[derive(Serialize, Deserialize, Debug, strum_macros::Display)]
pub enum HarmProbability {
    #[serde(rename = "HARM_PROBABILITY_UNSPECIFIED")]
    HarmProbabilityUnspecified,
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
    HarmSeverityUnspecified,
    #[serde(rename = "HARM_SEVERITY_NEGLIGIBLE")]
    HarmSeverityNegligible,
    #[serde(rename = "HARM_SEVERITY_LOW")]
    HarmSeverityLow,
    #[serde(rename = "HARM_SEVERITY_MEDIUM")]
    HarmSeverityMedium,
    #[serde(rename = "HARM_SEVERITY_HIGH")]
    HarmSeverityHigh,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub index: Option<i32>,
    pub content: Content,
    pub finish_reason: Option<FinishReason>,
    pub safety_ratings: Vec<SafetyRating>,
    pub citation_metadata: Option<CitationMetadata>,
    pub grounding_metadata: Option<GroundingMetadata>,
    pub finish_message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub role: String,
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

#[derive(Serialize, Deserialize, Debug, strum_macros::Display)]
pub enum FinishReason {
    #[serde(rename = "FINISH_REASON_UNSPECIFIED")]
    FinishReasonUnspecified,
    #[serde(rename = "STOP")]
    Stop,
    #[serde(rename = "MAX_TOKENS")]
    MaxTokens,
    #[serde(rename = "SAFETY")]
    Safety,
    #[serde(rename = "RECITATION")]
    Recitation,
    #[serde(rename = "OTHER")]
    Other,
    #[serde(rename = "BLOCKLIST")]
    Blocklist,
    #[serde(rename = "PROHIBITED_CONTENT")]
    ProhibitedContent,
    #[serde(rename = "SPII")]
    Spii,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CitationMetadata {
    pub citations: Vec<Citation>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub start_index: i32,
    pub end_index: i32,
    pub uri: String,
    pub title: String,
    pub license: String,
    pub publication_date: Date,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Date {
    pub year: i32,
    pub month: i32,
    pub day: i32,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GroundingMetadata {
    pub web_search_queries: Vec<String>,
    pub search_entry_point: SearchEntryPoint,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchEntryPoint {
    pub rendered_content: String,
    pub sdk_blob: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetaData {
    pub prompt_token_count: u64,
    pub candidates_token_count: u64,
    pub total_token_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Error;

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
            Ok(response) => println!("Parsed successfully: {:?}", response),
            Err(e) => {
                println!("Failed to parse: {}", e);
                println!("Error line: {}", e.line());
                println!("Error column: {}", e.column());
                println!("Error cause: {:?}", e.classify());
                assert!(false, "Deserialization failed");
            }
        }
    }
}
