use anyhow::{bail, Context, Result};
use baml_types::{BamlMap, BamlMedia, BamlMediaContent, BamlMediaType};
use base64::{prelude::BASE64_STANDARD, Engine};
use internal_baml_jinja::{ChatMessagePart, RenderedChatMessage};
use serde::{de::Deserializer, Deserialize, Serialize};
use serde_json::Value;

pub type CompletionResponse = ChatCompletionGeneric<CompletionChoice>;
pub type ChatCompletionResponse = ChatCompletionGeneric<ChatCompletionChoice>;

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptionParts {
    pub file_bytes: Vec<u8>,
    pub filename: String,
    pub mime: String,
    pub fields: BamlMap<String, String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TranscriptionResponse {
    pub text: String,
    pub usage: Option<TranscriptionUsage>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum TranscriptionUsage {
    Tokens {
        input_tokens: u64,
        output_tokens: u64,
        total_tokens: u64,
        input_token_details: Option<Value>,
    },
    Duration {
        seconds: f64,
    },
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum TranscriptionStreamEvent {
    #[serde(rename = "transcript.text.delta")]
    Delta { delta: String },
    #[serde(rename = "transcript.text.done")]
    Done {
        text: String,
        usage: Option<TranscriptionUsage>,
    },
    #[serde(rename = "transcript.text.segment")]
    Segment,
    #[serde(other)]
    Unknown,
}

pub fn build_transcription_parts(
    properties: &BamlMap<String, Value>,
    prompt: either::Either<&String, &[RenderedChatMessage]>,
) -> Result<TranscriptionParts> {
    let messages = match prompt {
        either::Either::Right(messages) => messages,
        either::Either::Left(_) => {
            bail!("OpenAI transcriptions require chat messages with exactly one audio media part")
        }
    };

    reject_reserved_request_fields(properties)?;

    let mut audio_parts = Vec::new();
    let mut text_parts = Vec::new();
    for message in messages {
        for part in &message.parts {
            collect_transcription_prompt_parts(part, &mut audio_parts, &mut text_parts)?;
        }
    }

    if audio_parts.len() != 1 {
        bail!(
            "OpenAI transcriptions require exactly one audio media part, got {}",
            audio_parts.len()
        );
    }

    let audio = audio_parts
        .pop()
        .expect("audio_parts length was already validated");
    let mime = audio.mime_type_as_ok()?;
    let file_bytes = match &audio.content {
        BamlMediaContent::Base64(media_b64) => BASE64_STANDARD
            .decode(&media_b64.base64)
            .context("Failed to decode transcription audio as base64")?,
        BamlMediaContent::Url(_) | BamlMediaContent::File(_) => {
            bail!("OpenAI transcription audio must be resolved to base64 before request building")
        }
    };

    let mut fields = BamlMap::new();
    let model = required_string_field(properties, "model")?;
    fields.insert("model".to_string(), model.clone());

    let property_prompt = optional_string_field(properties, "prompt")?;
    if property_prompt.is_some() && !text_parts.is_empty() {
        bail!("OpenAI transcription prompt is ambiguous: both properties.prompt and rendered text were provided");
    }
    let rendered_prompt = (!text_parts.is_empty()).then(|| text_parts.join("\n"));
    if let Some(prompt) = property_prompt.or(rendered_prompt) {
        fields.insert("prompt".to_string(), prompt);
    }

    if let Some(language) = optional_string_field(properties, "language")? {
        fields.insert("language".to_string(), language);
    }
    if let Some(response_format) = optional_response_format(properties, &model)? {
        fields.insert("response_format".to_string(), response_format);
    }
    if let Some(temperature) = optional_temperature(properties)? {
        fields.insert("temperature".to_string(), temperature);
    }

    Ok(TranscriptionParts {
        file_bytes,
        filename: filename_for_mime(&mime),
        mime,
        fields,
    })
}

fn collect_transcription_prompt_parts(
    part: &ChatMessagePart,
    audio_parts: &mut Vec<BamlMedia>,
    text_parts: &mut Vec<String>,
) -> Result<()> {
    match part {
        ChatMessagePart::Text(text) => {
            let text = text.trim();
            if !text.is_empty() {
                text_parts.push(text.to_string());
            }
        }
        ChatMessagePart::Media(media) => {
            if media.media_type != BamlMediaType::Audio {
                bail!("OpenAI transcriptions only support audio media parts")
            }
            audio_parts.push(media.clone());
        }
        ChatMessagePart::WithMeta(inner, _) => {
            collect_transcription_prompt_parts(inner, audio_parts, text_parts)?;
        }
    }
    Ok(())
}

fn reject_reserved_request_fields(properties: &BamlMap<String, Value>) -> Result<()> {
    for key in ["messages", "stream"] {
        if properties.contains_key(key) {
            bail!("OpenAI transcriptions do not support reserved request field `{key}`")
        }
    }
    Ok(())
}

fn required_string_field(properties: &BamlMap<String, Value>, field: &str) -> Result<String> {
    let value = properties
        .get(field)
        .with_context(|| format!("OpenAI transcriptions require string field `{field}`"))?;

    match value {
        Value::String(value) => Ok(value.clone()),
        _ => bail!("OpenAI transcription field `{field}` must be a string"),
    }
}

fn optional_string_field(
    properties: &BamlMap<String, Value>,
    field: &str,
) -> Result<Option<String>> {
    match properties.get(field) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("OpenAI transcription field `{field}` must be a string"),
        None => Ok(None),
    }
}

fn optional_response_format(
    properties: &BamlMap<String, Value>,
    model: &str,
) -> Result<Option<String>> {
    match properties.get("response_format") {
        Some(Value::String(value))
            if value == "json"
                || (value == "verbose_json"
                    && !matches!(
                        model,
                        "gpt-4o-transcribe"
                            | "gpt-4o-mini-transcribe"
                            | "gpt-4o-mini-transcribe-2025-12-15"
                            | "gpt-4o-transcribe-diarize"
                    )) => Ok(Some(value.clone())),
        Some(Value::String(value)) => bail!(
            "OpenAI transcription response_format `{value}` is not supported by model `{model}` in BAML; use `json`"
        ),
        Some(_) => bail!("OpenAI transcription field `response_format` must be a string"),
        None => Ok(None),
    }
}

fn optional_temperature(properties: &BamlMap<String, Value>) -> Result<Option<String>> {
    match properties.get("temperature") {
        Some(Value::Number(value)) => Ok(Some(value.to_string())),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("OpenAI transcription field `temperature` must be a number or string"),
        None => Ok(None),
    }
}

fn filename_for_mime(mime: &str) -> String {
    let subtype = mime
        .split('/')
        .next_back()
        .unwrap_or("mpeg")
        .split(';')
        .next()
        .unwrap_or("mpeg");
    let extension = match subtype {
        "mpeg" => "mp3",
        other => other,
    };
    format!("audio.{extension}")
}

/// OpenAI Responses API response structure
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ResponsesApiResponse {
    pub id: String,
    pub object: String,
    #[serde(default, deserialize_with = "deserialize_float_to_u32")]
    pub created_at: Option<u32>,
    pub status: String,
    pub model: String,
    pub output: Vec<ResponseOutput>,
    pub usage: Option<CompletionUsage>,
    pub error: Option<serde_json::Value>,
    pub incomplete_details: Option<IncompleteDetails>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseOutputType {
    Message,
    WebSearchCall,
    FileSearchCall,
    FunctionCall,
    Reasoning,
    ComputerCall,
    McpListTools,
    McpCall,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ResponseOutput {
    #[serde(rename = "type")]
    pub output_type: ResponseOutputType,
    pub id: Option<String>,
    pub status: Option<String>,
    pub role: Option<String>,
    #[serde(default, deserialize_with = "deserialize_maybe_list_to_vec")]
    pub content: Vec<ResponseContent>,
    // For web search calls
    pub action: Option<WebSearchAction>,
    // For file search calls
    pub queries: Option<Vec<String>>,
    pub results: Option<serde_json::Value>,
    // For function calls
    pub call_id: Option<String>,
    pub name: Option<String>,
    pub arguments: Option<String>,
    // For reasoning outputs
    pub summary: Option<Vec<serde_json::Value>>,
    // For MCP outputs
    pub server_label: Option<String>,
    pub tools: Option<Vec<McpToolDescriptor>>, // mcp_list_tools
    pub approval_request_id: Option<String>,   // mcp_call
    pub output: Option<String>,                // mcp_call output text
    pub error: Option<serde_json::Value>,      // mcp_call error
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct McpToolDescriptor {
    pub annotations: Option<serde_json::Value>,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct IncompleteDetails {
    pub reason: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct WebSearchAction {
    #[serde(rename = "type")]
    pub action_type: String,
    pub query: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ResponseContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
    pub annotations: Option<Vec<serde_json::Value>>,
    pub logprobs: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum ResponsesApiStreamEvent {
    #[serde(rename = "response.created")]
    ResponseCreated {
        response: ResponsesApiStreamResponse,
        sequence_number: u32,
    },
    #[serde(rename = "response.in_progress")]
    ResponseInProgress {
        response: ResponsesApiStreamResponse,
        sequence_number: u32,
    },
    #[serde(rename = "response.completed")]
    ResponseCompleted {
        response: ResponsesApiStreamResponse,
        sequence_number: u32,
    },
    #[serde(rename = "response.failed")]
    ResponseFailed {
        response: ResponsesApiStreamResponse,
        sequence_number: u32,
    },
    #[serde(rename = "response.incomplete")]
    ResponseIncomplete {
        response: ResponsesApiStreamResponse,
        sequence_number: u32,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        item_id: String,
        output_index: u32,
        content_index: u32,
        delta: String,
        sequence_number: u32,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        item_id: String,
        output_index: u32,
        content_index: u32,
        text: String,
        sequence_number: u32,
    },
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: ContentPart,
        sequence_number: u32,
    },
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: ContentPart,
        sequence_number: u32,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: String,
    pub annotations: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ResponsesApiStreamResponse {
    pub id: String,
    pub object: String,
    #[serde(default, deserialize_with = "deserialize_float_to_u32")]
    pub created_at: Option<u32>,
    pub status: String,
    pub model: String,
    pub output: Vec<ResponseOutput>,
    pub usage: Option<CompletionUsage>,
    pub error: Option<serde_json::Value>,
    pub incomplete_details: Option<IncompleteDetails>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct StreamOutputItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub status: Option<String>,
    pub role: Option<String>,
    pub content: Option<Vec<ResponseContent>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ContentDelta {
    pub index: u32,
    pub delta: DeltaContent,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct DeltaContent {
    #[serde(rename = "type")]
    pub delta_type: String,
    pub text: Option<String>,
}

pub type ChatCompletionResponseDelta = ChatCompletionGeneric<ChatCompletionChoiceDelta>;

/// Represents a chat completion response returned by model, based on the provided input.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatCompletionGeneric<C> {
    /// A unique identifier for the chat completion.
    pub id: Option<String>,
    /// A list of chat completion choices. Can be more than one if `n` is greater than 1.s
    pub choices: Vec<C>,
    /// The Unix timestamp (in seconds) of when the chat completion was created.
    #[serde(default, deserialize_with = "deserialize_float_to_u32")]
    pub created: Option<u32>,
    /// The model used for the chat completion.
    pub model: String,
    /// This fingerprint represents the backend configuration that the model runs with.
    ///
    /// Can be used in conjunction with the `seed` request parameter to understand when backend changes have been made that might impact determinism.
    pub system_fingerprint: Option<String>,

    /// The object type, which is `chat.completion` for non-streaming chat completion, `chat.completion.chunk` for streaming chat completion.
    pub object: Option<String>,
    pub usage: Option<CompletionUsage>,
}

fn deserialize_maybe_list_to_vec<'de, D, I>(deserializer: D) -> Result<Vec<I>, D::Error>
where
    D: Deserializer<'de>,
    I: Deserialize<'de>,
{
    match Option::<Vec<I>>::deserialize(deserializer)? {
        Some(inner) => Ok(inner),
        None => Ok(vec![]),
    }
}

fn deserialize_float_to_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum FloatOrInt {
        Int(u32),
        Float(f64),
    }

    match Option::<FloatOrInt>::deserialize(deserializer)? {
        Some(FloatOrInt::Int(i)) => Ok(Some(i)),
        Some(FloatOrInt::Float(f)) => Ok(Some(f.floor() as u32)),
        None => Ok(None),
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct CompletionChoice {
    pub finish_reason: Option<String>,
    pub index: u32,
    pub text: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatCompletionChoice {
    /// The index of the choice in the list of choices.
    pub index: u32,
    pub message: ChatCompletionResponseMessage,
    /// The reason the model stopped generating tokens. This will be `stop` if the model hit a natural stop point or a provided stop sequence,
    /// `length` if the maximum number of tokens specified in the request was reached,
    /// `content_filter` if content was omitted due to a flag from our content filters,
    /// `tool_calls` if the model called a tool, or `function_call` (deprecated) if the model called a function.
    pub finish_reason: Option<String>,
    /// Log probability information for the choice.
    pub logprobs: Option<ChatChoiceLogprobs>,
}

/// Usage statistics for the completion request.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct CompletionUsage {
    /// Number of tokens in the prompt.
    #[serde(alias = "input_tokens")]
    pub prompt_tokens: u64,
    /// Number of tokens in the generated completion.
    #[serde(alias = "output_tokens")]
    pub completion_tokens: u64,
    /// Total number of tokens used in the request (prompt + completion).
    pub total_tokens: u64,
    /// Additional fields that may be present in responses API
    #[serde(alias = "prompt_tokens_details")]
    pub input_tokens_details: Option<serde_json::Value>,
    #[serde(alias = "completion_tokens_details")]
    pub output_tokens_details: Option<serde_json::Value>,
}

/// A chat completion message generated by the model.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatCompletionResponseMessage {
    /// The contents of the message.
    pub content: Option<String>,

    /// The tool calls generated by the model, such as function calls.
    // pub tool_calls: Option<Vec<ChatCompletionMessageToolCall>>,

    /// The role of the author of this message.
    pub role: ChatCompletionMessageRole,
    // Deprecated and replaced by `tool_calls`.
    // The name and arguments of a function that should be called, as generated by the model.
    // #[deprecated]
    // pub function_call: Option<FunctionCall>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ChatCompletionChoiceDelta {
    pub index: u64,
    pub finish_reason: Option<String>,
    pub delta: ChatCompletionMessageDelta,
}

/// Same as ChatCompletionMessage, but received during a response stream.
#[derive(Deserialize, Clone, Debug)]
pub struct ChatCompletionMessageDelta {
    /// The role of the author of this message.
    pub role: Option<ChatCompletionMessageRole>,
    /// The contents of the message
    pub content: Option<String>,
    // The name of the user in a multi-user chat
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub name: Option<String>,
    // The function that ChatGPT called
    //
    // [API Reference](https://platform.openai.com/docs/api-reference/chat/create#chat/create-function_call)
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub function_call: Option<ChatCompletionFunctionCallDelta>,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChatCompletionMessageRole {
    System,
    #[default]
    User,
    Assistant,
    Tool,
    Function,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatChoiceLogprobs {
    /// A list of message content tokens with log probability information.
    pub content: Option<Vec<ChatCompletionTokenLogprob>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatCompletionTokenLogprob {
    /// The token.
    pub token: String,
    /// The log probability of this token.
    pub logprob: f32,
    /// A list of integers representing the UTF-8 bytes representation of the token. Useful in instances where characters are represented by multiple tokens and their byte representations must be combined to generate the correct text representation. Can be `null` if there is no bytes representation for the token.
    pub bytes: Option<Vec<u8>>,
    ///  List of the most likely tokens and their log probability, at this token position. In rare cases, there may be fewer than the number of requested `top_logprobs` returned.
    pub top_logprobs: Vec<TopLogprobs>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TopLogprobs {
    /// The token.
    pub token: String,
    /// The log probability of this token.
    pub logprob: f32,
    /// A list of integers representing the UTF-8 bytes representation of the token. Useful in instances where characters are represented by multiple tokens and their byte representations must be combined to generate the correct text representation. Can be `null` if there is no bytes representation for the token.
    pub bytes: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAIErrorResponse {
    pub error: OpenAIError,
}

#[derive(Debug, Deserialize)]
pub struct OpenAIError {
    pub message: String,
    pub r#type: String,
    pub code: Option<String>,
}

#[cfg(test)]
mod transcription_parts_tests {
    use std::path::PathBuf;

    use baml_types::{BamlMap, BamlMedia, BamlMediaType};
    use base64::{prelude::BASE64_STANDARD, Engine};
    use internal_baml_jinja::{ChatMessagePart, RenderedChatMessage};
    use serde_json::json;

    use super::build_transcription_parts;

    fn audio_message(base64: String, mime: &str) -> RenderedChatMessage {
        RenderedChatMessage {
            role: "user".to_string(),
            allow_duplicate_role: false,
            parts: vec![ChatMessagePart::Media(BamlMedia::base64(
                BamlMediaType::Audio,
                base64,
                Some(mime.to_string()),
            ))],
        }
    }

    fn text_message(text: &str) -> RenderedChatMessage {
        RenderedChatMessage {
            role: "user".to_string(),
            allow_duplicate_role: false,
            parts: vec![ChatMessagePart::Text(text.to_string())],
        }
    }

    fn props(entries: &[(&str, serde_json::Value)]) -> BamlMap<String, serde_json::Value> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn transcription_parts_happy_path_decodes_audio_and_fields() {
        let audio_bytes = b"fake mp3 bytes".to_vec();
        let audio_b64 = BASE64_STANDARD.encode(&audio_bytes);
        let properties = props(&[
            ("model", json!("whisper-1")),
            ("language", json!("en")),
            ("response_format", json!("verbose_json")),
            ("temperature", json!(0.25)),
        ]);
        let messages = vec![audio_message(audio_b64, "audio/mpeg")];

        let parts = build_transcription_parts(&properties, either::Right(messages.as_slice()))
            .expect("valid transcriptions prompt should build multipart parts");

        assert_eq!(parts.file_bytes, audio_bytes);
        assert_eq!(parts.filename, "audio.mp3");
        assert_eq!(parts.mime, "audio/mpeg");
        assert_eq!(parts.fields.get("model").unwrap(), "whisper-1");
        assert_eq!(parts.fields.get("language").unwrap(), "en");
        assert_eq!(parts.fields.get("response_format").unwrap(), "verbose_json");
        assert_eq!(parts.fields.get("temperature").unwrap(), "0.25");
        assert!(!parts.fields.contains_key("messages"));
        assert!(!parts.fields.contains_key("stream"));
    }

    #[test]
    fn transcription_parts_maps_single_rendered_text_to_prompt() {
        let audio_b64 = BASE64_STANDARD.encode(b"audio bytes");
        let properties = props(&[("model", json!("gpt-4o-transcribe"))]);
        let messages = vec![
            text_message("Use the product spelling from the clip."),
            audio_message(audio_b64, "audio/wav"),
        ];

        let parts = build_transcription_parts(&properties, either::Right(messages.as_slice()))
            .expect("single rendered text should map to prompt");

        assert_eq!(
            parts.fields.get("prompt").map(String::as_str),
            Some("Use the product spelling from the clip.")
        );
    }

    #[test]
    fn transcription_parts_joins_text_from_multi_part_chat_messages() {
        let audio_b64 = BASE64_STANDARD.encode(b"audio bytes");
        let properties = props(&[("model", json!("gpt-transcribe"))]);
        let messages = vec![
            text_message("Use the product spelling from the clip."),
            RenderedChatMessage {
                role: "user".to_string(),
                allow_duplicate_role: false,
                parts: vec![
                    ChatMessagePart::Text("Transcribe this recording.".to_string()),
                    ChatMessagePart::Media(BamlMedia::base64(
                        BamlMediaType::Audio,
                        audio_b64,
                        Some("audio/wav".to_string()),
                    )),
                    ChatMessagePart::Text("Preserve punctuation.".to_string()),
                ],
            },
        ];

        let parts = build_transcription_parts(&properties, either::Right(messages.as_slice()))
            .expect("multi-part chat text should map to one transcription prompt");

        assert_eq!(
            parts.fields.get("prompt").map(String::as_str),
            Some(
                "Use the product spelling from the clip.\nTranscribe this recording.\nPreserve punctuation."
            )
        );
    }

    #[test]
    fn transcription_parts_rejects_required_audio_and_model_errors() {
        let audio_b64 = BASE64_STANDARD.encode(b"audio bytes");
        let missing_model = props(&[]);
        let non_string_model = props(&[("model", json!(123))]);
        let valid_props = props(&[("model", json!("gpt-4o-transcribe"))]);

        let cases = [
            (
                missing_model,
                either::Right(vec![audio_message(audio_b64.clone(), "audio/wav")]),
                "model",
            ),
            (
                non_string_model,
                either::Right(vec![audio_message(audio_b64.clone(), "audio/wav")]),
                "model",
            ),
            (valid_props.clone(), either::Right(vec![]), "audio"),
            (
                valid_props.clone(),
                either::Right(vec![
                    audio_message(audio_b64.clone(), "audio/wav"),
                    audio_message(audio_b64.clone(), "audio/wav"),
                ]),
                "exactly one",
            ),
            (
                valid_props,
                either::Left("completion prompt".to_string()),
                "chat messages",
            ),
        ];

        for (properties, prompt, expected_message) in cases {
            let err = match prompt {
                either::Left(prompt) => {
                    build_transcription_parts(&properties, either::Left(&prompt)).unwrap_err()
                }
                either::Right(messages) => {
                    build_transcription_parts(&properties, either::Right(messages.as_slice()))
                        .unwrap_err()
                }
            };
            assert!(
                err.to_string().contains(expected_message),
                "expected error containing {expected_message:?}, got {err:?}"
            );
        }
    }

    #[test]
    fn transcription_parts_rejects_invalid_base64_and_prompt_conflicts() {
        let audio_b64 = BASE64_STANDARD.encode(b"audio bytes");
        let properties_with_prompt = props(&[
            ("model", json!("gpt-4o-transcribe")),
            ("prompt", json!("from properties")),
        ]);
        let valid_props = props(&[("model", json!("gpt-4o-transcribe"))]);

        let cases = [
            (
                valid_props.clone(),
                vec![audio_message("not base64".to_string(), "audio/wav")],
                "base64",
            ),
            (
                valid_props.clone(),
                vec![audio_message(
                    "data:audio/wav;base64,AAAA".to_string(),
                    "audio/wav",
                )],
                "base64",
            ),
            (
                properties_with_prompt,
                vec![
                    text_message("from rendered prompt"),
                    audio_message(audio_b64.clone(), "audio/wav"),
                ],
                "prompt",
            ),
        ];

        for (properties, messages, expected_message) in cases {
            let err = build_transcription_parts(&properties, either::Right(messages.as_slice()))
                .unwrap_err();
            assert!(
                err.to_string().contains(expected_message),
                "expected error containing {expected_message:?}, got {err:?}"
            );
        }
    }

    #[test]
    fn transcription_parts_rejects_invalid_field_values() {
        let audio_b64 = BASE64_STANDARD.encode(b"audio bytes");

        let cases = [
            (("prompt", json!(true)), "prompt"),
            (("prompt", json!({ "text": "hello" })), "prompt"),
            (("language", json!(["en"])), "language"),
            (("temperature", json!(false)), "temperature"),
            (("temperature", serde_json::Value::Null), "temperature"),
            (("response_format", json!("text")), "response_format"),
            (("response_format", json!("srt")), "response_format"),
            (("response_format", json!("vtt")), "response_format"),
            (
                ("response_format", json!("verbose_json")),
                "response_format",
            ),
            (("messages", json!([])), "messages"),
            (("stream", json!(false)), "stream"),
        ];

        for ((key, value), expected_message) in cases {
            let mut properties = props(&[("model", json!("gpt-4o-transcribe"))]);
            properties.insert(key.to_string(), value);
            let messages = vec![audio_message(audio_b64.clone(), "audio/wav")];

            let err = build_transcription_parts(&properties, either::Right(messages.as_slice()))
                .unwrap_err();
            assert!(
                err.to_string().contains(expected_message),
                "expected error containing {expected_message:?}, got {err:?}"
            );
        }
    }

    #[test]
    fn transcription_parts_rejects_unresolved_or_non_audio_media() {
        let audio_b64 = BASE64_STANDARD.encode(b"audio bytes");
        let properties = props(&[("model", json!("gpt-4o-transcribe"))]);
        let cases = [
            (
                BamlMedia::base64(
                    BamlMediaType::Image,
                    audio_b64,
                    Some("image/png".to_string()),
                ),
                "audio",
            ),
            (
                BamlMedia::url(
                    BamlMediaType::Audio,
                    "https://example.com/clip.mp3".to_string(),
                    Some("audio/mpeg".to_string()),
                ),
                "base64",
            ),
            (
                BamlMedia::file(
                    BamlMediaType::Audio,
                    PathBuf::from("/tmp/test.baml"),
                    "clip.wav".to_string(),
                    Some("audio/wav".to_string()),
                ),
                "base64",
            ),
        ];

        for (media, expected_message) in cases {
            let messages = vec![RenderedChatMessage {
                role: "user".to_string(),
                allow_duplicate_role: false,
                parts: vec![ChatMessagePart::Media(media)],
            }];

            let err = build_transcription_parts(&properties, either::Right(messages.as_slice()))
                .unwrap_err();
            assert!(
                err.to_string().contains(expected_message),
                "expected error containing {expected_message:?}, got {err:?}"
            );
        }
    }

    #[test]
    fn transcription_parts_accepts_json_response_format_and_string_temperature() {
        let audio_b64 = BASE64_STANDARD.encode(b"audio bytes");
        let properties = props(&[
            ("model", json!("gpt-4o-transcribe")),
            ("response_format", json!("json")),
            ("temperature", json!("0.4")),
        ]);
        let messages = vec![audio_message(audio_b64, "audio/wav")];

        let parts = build_transcription_parts(&properties, either::Right(messages.as_slice()))
            .expect("json response_format and string temperature are valid");

        assert_eq!(parts.fields.get("response_format").unwrap(), "json");
        assert_eq!(parts.fields.get("temperature").unwrap(), "0.4");
    }
}
