use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct GoogleRequestBody {
    pub contents: Vec<Content>,
    pub tools: Option<Vec<Tool>>,
    pub safetySettings: Option<SafetySetting>,
    pub generationConfig: Option<GenerationConfig>,
    pub systemInstruction: Option<Content>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct Tool {
    pub functionDeclarations: Option<Vec<FunctionDeclaration>>,
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
    STRING,
    NUMBER,
    INTEGER,
    BOOLEAN,
    OBJECT,
    ARRAY,
    TYPE_UNSPECIFIED,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct Retrieval {
    pub disableAttribution: bool,
    pub vertexAiSearch: VertexAiSearch,
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
    HARM_BLOCK_THRESHOLD_UNSPECIFIED,
    BLOCK_LOW_AND_ABOVE,
    BLOCK_MEDIUM_AND_ABOVE,
    BLOCK_ONLY_HIGH,
    BLOCK_NONE,
}

#[derive(Serialize, Deserialize)]
pub enum HarmBlockMethod {
    HARM_BLOCK_METHOD_UNSPECIFIED,
    SEVERITY,
    PROBABILITY,
}

#[derive(Serialize, Deserialize)]
pub struct GenerationConfig {
    pub stopSequences: Option<Vec<String>>,
    pub responseMimeType: Option<String>,
    pub temperature: Option<f64>,
    pub topP: Option<f64>,
    pub topK: Option<i32>,
    pub candidateCount: Option<i32>,
    pub maxOutputTokens: Option<i32>,
    pub presencePenalty: Option<f64>,
    pub frequencyPenalty: Option<f64>,
    pub responseSchema: Option<Schema>,
}

#[derive(Serialize, Deserialize)]
pub struct GoogleResponse {
    pub candidates: Vec<Candidate>,
    pub promptFeedback: Option<PromptFeedback>,
    pub usageMetaData: UsageMetaData,
}

#[derive(Serialize, Deserialize)]
pub struct PromptFeedback {
    pub blockReason: BlockReason,
    pub safetyRatings: Vec<SafetyRating>,
    pub blockReasonMessage: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub enum BlockReason {
    BLOCKED_REASON_UNSPECIFIED,
    SAFETY,
    OTHER,
    BLOCKLIST,
    PROHIBITED_CONTENT,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct SafetyRating {
    pub category: HarmCategory,
    pub probability: HarmProbability,
    pub probabilityScore: i32,
    pub severity: HarmSeverity,
    pub severityScore: i32,
    pub blocked: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub enum HarmCategory {
    HARM_CATEGORY_UNSPECIFIED,
    HARM_CATEGORY_HATE_SPEECH,
    HARM_CATEGORY_DANGEROUS_CONTENT,
    HARM_CATEGORY_HARASSMENT,
    HARM_CATEGORY_SEXUALLY_EXPLICIT,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub enum HarmProbability {
    HARM_PROBABILITY_UNSPECIFIED,
    NEGLIGIBLE,
    LOW,
    MEDIUM,
    HIGH,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub enum HarmSeverity {
    HARM_SEVERITY_UNSPECIFIED,
    HARM_SEVERITY_NEGLIGIBLE,
    HARM_SEVERITY_LOW,
    HARM_SEVERITY_MEDIUM,
    HARM_SEVERITY_HIGH,
}

#[derive(Serialize, Deserialize)]
pub struct Candidate {
    pub index: i32,
    pub content: Content,
    pub finishReason: Option<FinishReason>,
    pub safetyRatings: Vec<SafetyRating>,
    pub citationMetadata: CitationMetadata,
    pub groundingMetadata: GroundingMetadata,
    pub finishMessage: Option<String>,
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
    pub inlineData: Option<Blob>,
    pub fileData: Option<FileData>,
    pub functionCall: Option<FunctionCall>,
    pub functionResponse: Option<FunctionResponse>,
    pub videoMetadata: Option<VideoMetadata>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct Blob {
    pub mimeType: String,
    pub data: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileData {
    pub mimeType: String,
    pub fileUri: String,
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
    pub startOffset: Option<Duration>,
    pub endOffset: Option<Duration>,
}

#[derive(Serialize, Deserialize)]
pub struct Duration {
    pub seconds: i64,
    pub nanos: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinishReason {
    FINISH_REASON_UNSPECIFIED,
    STOP,
    MAX_TOKENS,
    SAFETY,
    RECITATION,
    OTHER,
    BLOCKLIST,
    PROHIBITED_CONTENT,
    SPII,
}

#[derive(Serialize, Deserialize)]
pub struct CitationMetadata {
    pub citations: Vec<Citation>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]

pub struct Citation {
    pub startIndex: i32,
    pub endIndex: i32,
    pub uri: String,
    pub title: String,
    pub license: String,
    pub publicationDate: Date,
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
    pub webSearchQueries: Vec<String>,
    pub searchEntryPoint: SearchEntryPoint,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchEntryPoint {
    pub renderedContent: String,
    pub sdkBlob: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UsageMetaData {
    pub promptTokenCount: i32,
    pub candidatesTokenCount: i32,
    pub totalTokenCount: i32,
}
