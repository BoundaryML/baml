use serde::{de::Deserializer, Deserialize, Serialize};

pub type CompletionResponse = ChatCompletionGeneric<CompletionChoice>;
pub type ChatCompletionResponse = ChatCompletionGeneric<ChatCompletionChoice>;

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
