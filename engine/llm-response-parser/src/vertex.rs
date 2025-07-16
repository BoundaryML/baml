use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Vertex AI response format (very similar to Google AI but with some differences)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VertexResponse {
    pub candidates: Vec<Candidate>,
    pub prompt_feedback: Option<PromptFeedback>,
    pub usage_metadata: Option<UsageMetaData>,
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
    pub category: Option<HarmCategory>,
    pub probability: Option<HarmProbability>,
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

// Vertex AI error response
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct VertexErrorResponse {
    pub error: VertexError,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct VertexError {
    pub code: i32,
    pub message: String,
    pub status: Option<String>,
    pub details: Option<Vec<serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_vertex_response() {
        let json = r#"{
          "candidates": [
            {
              "content": {
                "role": "model",
                "parts": [
                  {
                    "text": "The air in Donkey Kong's treehouse was thick with frustration."
                  }
                ]
              },
              "finishReason": "STOP",
              "safetyRatings": [
                {
                  "category": "HARM_CATEGORY_HATE_SPEECH",
                  "probability": "NEGLIGIBLE",
                  "probabilityScore": 0.12085322,
                  "severity": "HARM_SEVERITY_NEGLIGIBLE",
                  "severityScore": 0.11616109
                },
                {
                  "category": "HARM_CATEGORY_DANGEROUS_CONTENT",
                  "probability": "NEGLIGIBLE",
                  "probabilityScore": 0.07356305,
                  "severity": "HARM_SEVERITY_NEGLIGIBLE",
                  "severityScore": 0.037750278
                },
                {
                  "category": "HARM_CATEGORY_HARASSMENT",
                  "probability": "NEGLIGIBLE",
                  "probabilityScore": 0.24926445,
                  "severity": "HARM_SEVERITY_NEGLIGIBLE",
                  "severityScore": 0.108566426
                },
                {
                  "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT",
                  "probability": "NEGLIGIBLE",
                  "probabilityScore": 0.08137363,
                  "severity": "HARM_SEVERITY_NEGLIGIBLE",
                  "severityScore": 0.1301748
                }
              ]
            }
          ],
          "usageMetadata": {
            "promptTokenCount": 11,
            "candidatesTokenCount": 433,
            "totalTokenCount": 444
          }
        }"#;

        let response: VertexResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.candidates.len(), 1);
        assert_eq!(
            response.candidates[0].finish_reason,
            Some("STOP".to_string())
        );

        let content = response.candidates[0].content.as_ref().unwrap();
        assert_eq!(content.role, Some("model".to_string()));
        assert!(content.parts[0].text.contains("Donkey Kong"));

        let usage = response.usage_metadata.as_ref().unwrap();
        assert_eq!(usage.prompt_token_count, Some(11));
        assert_eq!(usage.candidates_token_count, Some(433));
        assert_eq!(usage.total_token_count, Some(444));

        // Check safety ratings
        let safety_ratings = response.candidates[0].safety_ratings.as_ref().unwrap();
        assert_eq!(safety_ratings.len(), 4);
        assert_eq!(safety_ratings[0].category, Some(HarmCategory::HateSpeech));
        assert_eq!(
            safety_ratings[0].probability,
            Some(HarmProbability::Negligible)
        );
    }

    #[test]
    fn test_deserialize_with_citation() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "text": "Response with citation"
                    }]
                },
                "finishReason": "STOP",
                "citationMetadata": {
                    "citations": [{
                        "startIndex": 0,
                        "endIndex": 22,
                        "uri": "https://example.com",
                        "title": "Example Source"
                    }]
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 5,
                "candidatesTokenCount": 3,
                "totalTokenCount": 8
            }
        }"#;

        let response: VertexResponse = serde_json::from_str(json).unwrap();
        let citation_metadata = response.candidates[0].citation_metadata.as_ref().unwrap();
        let citations = citation_metadata.citations.as_ref().unwrap();
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].uri, Some("https://example.com".to_string()));
        assert_eq!(citations[0].title, Some("Example Source".to_string()));
    }

    #[test]
    fn test_deserialize_error_response() {
        let json = r#"{
            "error": {
                "code": 403,
                "message": "Permission denied",
                "status": "PERMISSION_DENIED",
                "details": []
            }
        }"#;

        let response: VertexErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.error.code, 403);
        assert_eq!(response.error.message, "Permission denied");
        assert_eq!(response.error.status, Some("PERMISSION_DENIED".to_string()));
    }

    #[test]
    fn test_vertex_optional_usage_metadata() {
        // Vertex sometimes doesn't include usage metadata
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "text": "No usage metadata"
                    }]
                },
                "finishReason": "STOP"
            }]
        }"#;

        let response: VertexResponse = serde_json::from_str(json).unwrap();
        assert!(response.usage_metadata.is_none());
    }

    #[test]
    fn test_vertex_safety_rating_optional_fields() {
        // Vertex safety ratings have optional category field
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "text": "Test"
                    }]
                },
                "finishReason": "STOP",
                "safetyRatings": [{
                    "probability": "NEGLIGIBLE",
                    "probabilityScore": 0.1
                }]
            }]
        }"#;

        let response: VertexResponse = serde_json::from_str(json).unwrap();
        let safety_rating = &response.candidates[0].safety_ratings.as_ref().unwrap()[0];
        assert!(safety_rating.category.is_none());
        assert_eq!(safety_rating.probability, Some(HarmProbability::Negligible));
    }
}
