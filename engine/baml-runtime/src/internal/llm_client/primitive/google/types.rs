use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct GoogleRequestBody {
    pub contents: Vec<Content>,
    pub tools: Option<Vec<Tool>>,
    pub safety_settings: Option<SafetySetting>,
    pub generation_config: Option<GenerationConfig>,
    pub system_instruction: Option<Content>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct Tool {
    pub function_declarations: Option<Vec<FunctionDeclaration>>,
    pub retrieval: Option<Retrieval>,
}

#[derive(Serialize, Deserialize)]

pub struct FunctionDeclaration {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<Schema>,
}

#[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    NullValue,
    NumberValue(f64),
    StringValue(String),
    BoolValue(bool),
    StructValue(Struct),
    ListValue(Vec<Value>),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub enum Type {
    String,
    Number,
    Integer,
    Boolean,
    Object,
    Array,
    TypeUnspecified,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct Retrieval {
    pub disable_attribution: bool,
    pub vertex_ai_search: VertexAiSearch,
}

#[derive(Serialize, Deserialize)]
pub struct VertexAiSearch {
    pub datastore: String,
}

#[derive(Serialize, Deserialize)]
pub struct SafetySetting {
    pub category: HarmCategory,
    pub threshold: HarmBlockThreshold,
    pub method: HarmBlockMethod,
}

#[derive(Serialize, Deserialize)]
pub enum HarmBlockThreshold {
    HarmBlockThresholdUnspecified,
    BlockLowAndAbove,
    BlockMediumAndAbove,
    BlockOnlyHigh,
    BlockNone,
}

#[derive(Serialize, Deserialize)]
pub enum HarmBlockMethod {
    HarmBlockMethodUnspecified,
    Severity,
    Probability,
}

#[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
pub struct GoogleResponse {
    pub candidates: Vec<Candidate>,
    pub prompt_feedback: Option<PromptFeedback>,
    pub usage_meta_data: UsageMetaData,
}

#[derive(Serialize, Deserialize)]
pub struct PromptFeedback {
    pub block_reason: BlockReason,
    pub safety_ratings: Vec<SafetyRating>,
    pub block_reason_message: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub enum BlockReason {
    BlockedReasonUnspecified,
    Safety,
    Other,
    Blocklist,
    ProhibitedContent,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct SafetyRating {
    pub category: HarmCategory,
    pub probability: HarmProbability,
    pub probability_score: i32,
    pub severity: HarmSeverity,
    pub severity_score: i32,
    pub blocked: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub enum HarmCategory {
    HarmCategoryUnspecified,
    HarmCategoryHateSpeech,
    HarmCategoryDangerousContent,
    HarmCategoryHarassment,
    HarmCategorySexuallyExplicit,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub enum HarmProbability {
    HarmProbabilityUnspecified,
    Negligible,
    Low,
    Medium,
    High,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub enum HarmSeverity {
    HarmSeverityUnspecified,
    HarmSeverityNegligible,
    HarmSeverityLow,
    HarmSeverityMedium,
    HarmSeverityHigh,
}

#[derive(Serialize, Deserialize)]
pub struct Candidate {
    pub index: i32,
    pub content: Content,
    pub finish_reason: Option<FinishReason>,
    pub safety_ratings: Vec<SafetyRating>,
    pub citation_metadata: CitationMetadata,
    pub grounding_metadata: GroundingMetadata,
    pub finish_message: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Content {
    pub role: String,
    pub parts: Vec<Part>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Part {
    pub text: String,
    pub inline_data: Option<Blob>,
    pub file_data: Option<FileData>,
    pub function_call: Option<FunctionCall>,
    pub function_response: Option<FunctionResponse>,
    pub video_metadata: Option<VideoMetadata>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct Blob {
    pub mime_type: String,
    pub data: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileData {
    pub mime_type: String,
    pub file_uri: String,
}

#[derive(Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub args: Option<Vec<Struct>>,
}

#[derive(Serialize, Deserialize)]
pub struct Struct {
    pub fields: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
pub struct FunctionResponse {
    pub name: String,
    pub response: Option<Struct>,
}

#[derive(Serialize, Deserialize)]
pub struct VideoMetadata {
    pub start_offset: Option<Duration>,
    pub end_offset: Option<Duration>,
}

#[derive(Serialize, Deserialize)]
pub struct Duration {
    pub seconds: i64,
    pub nanos: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinishReason {
    FinishReasonUnspecified,
    Stop,
    MaxTokens,
    Safety,
    Recitation,
    Other,
    Blocklist,
    ProhibitedContent,
    Spii,
}
#[derive(Serialize, Deserialize)]
pub struct CitationMetadata {
    pub citations: Vec<Citation>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct Citation {
    pub start_index: i32,
    pub end_index: i32,
    pub uri: String,
    pub title: String,
    pub license: String,
    pub publication_date: Date,
}
#[derive(Serialize, Deserialize)]
pub struct Date {
    pub year: i32,
    pub month: i32,
    pub day: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct GroundingMetadata {
    pub web_search_queries: Vec<String>,
    pub search_entry_point: SearchEntryPoint,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchEntryPoint {
    pub rendered_content: String,
    pub sdk_blob: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UsageMetaData {
    pub prompt_token_count: i32,
    pub candidates_token_count: i32,
    pub total_token_count: i32,
}
