use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GoogleResponse {
    pub candidates: Vec<Candidate>,
    pub prompt_feedback: Option<PromptFeedback>,
    pub usage_metadata: UsageMetaData,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub index: Option<i32>,
    pub content: Option<Content>,
    pub finish_reason: Option<String>,
    pub safety_ratings: Option<Vec<SafetyRating>>,
    pub citation_metadata: Option<CitationMetadata>,
    pub grounding_metadata: Option<GroundingMetadata>,
    pub finish_message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub role: Option<String>,
    pub parts: Vec<Part>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Blob {
    pub mime_type: String,
    pub data: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileData {
    pub mime_type: String,
    pub file_uri: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FunctionCall {
    pub name: String,
    pub args: Option<Vec<Struct>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Struct {
    pub fields: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FunctionResponse {
    pub name: String,
    pub response: Option<Struct>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadata {
    pub start_offset: Option<Duration>,
    pub end_offset: Option<Duration>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Duration {
    pub seconds: i64,
    pub nanos: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetaData {
    pub prompt_token_count: Option<u64>,
    pub candidates_token_count: Option<u64>,
    pub total_token_count: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PromptFeedback {
    pub block_reason: BlockReason,
    pub safety_ratings: Vec<SafetyRating>,
    pub block_reason_message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SafetyRating {
    pub category: HarmCategory,
    pub probability: HarmProbability,
    pub probability_score: Option<f64>,
    pub severity: Option<HarmSeverity>,
    pub severity_score: Option<f64>,
    pub blocked: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CitationMetadata {
    pub citations: Option<Vec<Citation>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub start_index: Option<i32>,
    pub end_index: Option<i32>,
    pub uri: Option<String>,
    pub title: Option<String>,
    pub license: Option<String>,
    pub publication_date: Option<Date>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Date {
    pub year: Option<i32>,
    pub month: Option<i32>,
    pub day: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GroundingMetadata {
    pub web_search_queries: Option<Vec<String>>,
    pub search_entry_point: Option<SearchEntryPoint>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchEntryPoint {
    pub rendered_content: Option<String>,
    pub sdk_blob: Option<Vec<u8>>,
}

// Error response types
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct GoogleErrorResponse {
    pub error: GoogleError,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct GoogleError {
    pub code: i32,
    pub message: String,
    pub status: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_google_response() {
        let json = r#"{
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
          }"#;

        let response: GoogleResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.candidates.len(), 1);
        assert_eq!(
            response.candidates[0].finish_reason,
            Some("STOP".to_string())
        );

        let content = response.candidates[0].content.as_ref().unwrap();
        assert_eq!(content.role, Some("model".to_string()));
        assert_eq!(content.parts.len(), 1);
        assert!(content.parts[0].text.contains("Dark fizz, cherry bright"));

        assert_eq!(response.usage_metadata.prompt_token_count, Some(8));
        assert_eq!(response.usage_metadata.candidates_token_count, Some(21));
        assert_eq!(response.usage_metadata.total_token_count, Some(29));
    }

    #[test]
    fn test_deserialize_with_inline_image() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "text": "I see an image"
                    }]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 300,
                "candidatesTokenCount": 4,
                "totalTokenCount": 304
            }
        }"#;

        let response: GoogleResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.candidates[0].content.as_ref().unwrap().parts[0].text,
            "I see an image"
        );
    }

    #[test]
    fn test_deserialize_error_response() {
        let json = r#"{
            "error": {
                "code": 400,
                "message": "API key not valid. Please pass a valid API key.",
                "status": "INVALID_ARGUMENT"
            }
        }"#;

        let response: GoogleErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.error.code, 400);
        assert_eq!(
            response.error.message,
            "API key not valid. Please pass a valid API key."
        );
        assert_eq!(response.error.status, Some("INVALID_ARGUMENT".to_string()));
    }

    #[test]
    fn test_deserialize_with_function_call() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "text": "",
                        "functionCall": {
                            "name": "get_weather",
                            "args": [{
                                "fields": {
                                    "location": {
                                        "STRING_VALUE": "Boston"
                                    }
                                }
                            }]
                        }
                    }]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 50,
                "candidatesTokenCount": 10,
                "totalTokenCount": 60
            }
        }"#;

        let response: GoogleResponse = serde_json::from_str(json).unwrap();
        let part = &response.candidates[0].content.as_ref().unwrap().parts[0];
        assert!(part.function_call.is_some());

        let func_call = part.function_call.as_ref().unwrap();
        assert_eq!(func_call.name, "get_weather");
    }
}
