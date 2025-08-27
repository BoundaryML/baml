use serde::{
    de::{self, Deserializer},
    Deserialize, Serialize,
};

pub type CompletionResponse = ChatCompletionGeneric<CompletionChoice>;
pub type ChatCompletionResponse = ChatCompletionGeneric<ChatCompletionChoice>;

/// OpenAI Responses API response structure
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ResponsesApiResponse {
    pub id: String,
    pub object: String,
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
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ResponseOutput {
    #[serde(rename = "type")]
    pub output_type: ResponseOutputType,
    pub id: Option<String>,
    pub status: Option<String>,
    pub role: Option<String>,
    #[serde(default)]
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
    pub created_at: u32,
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
    pub input_tokens_details: Option<serde_json::Value>,
    pub output_tokens_details: Option<serde_json::Value>,
}

/// Represents a tool call in a chat completion response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionMessageToolCall {
    /// The ID of the tool call
    pub id: String,
    /// The type of the tool. Currently, only "function" is supported.
    #[serde(rename = "type")]
    pub call_type: String,
    /// The function that the model called.
    pub function: ToolCallFunction,
}

/// Function details in a tool call
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallFunction {
    /// The name of the function to call.
    pub name: String,
    /// The arguments to call the function with, as generated by the model in JSON format.
    pub arguments: String,
}

/// A chat completion message generated by the model.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatCompletionResponseMessage {
    /// The contents of the message.
    pub content: Option<String>,

    /// The tool calls generated by the model, such as function calls.
    pub tool_calls: Option<Vec<ChatCompletionMessageToolCall>>,

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
    /// Tool calls during streaming
    pub tool_calls: Option<Vec<ChatCompletionMessageToolCallDelta>>,
    // The name of the user in a multi-user chat
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub name: Option<String>,
    // The function that ChatGPT called
    //
    // [API Reference](https://platform.openai.com/docs/api-reference/chat/create#chat/create-function_call)
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub function_call: Option<ChatCompletionFunctionCallDelta>,
}

/// Delta tool call in streaming responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionMessageToolCallDelta {
    /// Index of the tool call in the array
    pub index: u32,
    /// The ID of the tool call (only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The type of the tool (only in first chunk)
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub call_type: Option<String>,
    /// Function delta information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<ToolCallFunctionDelta>,
}

/// Function delta in streaming tool calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunctionDelta {
    /// The name of the function (only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The arguments chunk (streamed incrementally)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
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

/// Tool definition for OpenAI API requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// The type of the tool. Currently, only "function" is supported.
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function definition
    pub function: ToolFunction,
}

/// Function definition within a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    /// The name of the function to be called.
    pub name: String,
    /// A description of what the function does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The parameters the function accepts, described as a JSON Schema object.
    pub parameters: serde_json::Value,
    /// Whether to enable strict schema adherence for structured outputs (2024 feature)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Tool choice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoiceOption {
    /// String values: "auto", "none", "required"
    String(String),
    /// Specific tool selection
    Object(ToolChoiceSpecific),
}

/// Specific tool choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceSpecific {
    /// The type of the tool. Currently, only "function" is supported.
    #[serde(rename = "type")]
    pub choice_type: String,
    /// Function name specification
    pub function: ToolChoiceFunctionName,
}

/// Function name for specific tool choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceFunctionName {
    /// The name of the function to call.
    pub name: String,
}

#[cfg(test)]
mod test_openai_types {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_call_serialization() {
        let tool_call = ChatCompletionMessageToolCall {
            id: "call_123".to_string(),
            call_type: "function".to_string(),
            function: ToolCallFunction {
                name: "get_weather".to_string(),
                arguments: r#"{"location": "Paris", "unit": "celsius"}"#.to_string(),
            },
        };

        let serialized = serde_json::to_string(&tool_call).unwrap();
        let deserialized: ChatCompletionMessageToolCall = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(deserialized.id, "call_123");
        assert_eq!(deserialized.call_type, "function");
        assert_eq!(deserialized.function.name, "get_weather");
        assert!(deserialized.function.arguments.contains("Paris"));
    }

    #[test]
    fn test_tool_definition_serialization() {
        let tool = Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "get_weather".to_string(),
                description: Some("Get weather information".to_string()),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "location": {"type": "string"},
                        "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}
                    },
                    "required": ["location"]
                }),
                strict: Some(false),
            },
        };

        let serialized = serde_json::to_value(&tool).unwrap();
        assert_eq!(serialized["type"], "function");
        assert_eq!(serialized["function"]["name"], "get_weather");
        assert_eq!(serialized["function"]["description"], "Get weather information");
        assert!(serialized["function"]["parameters"].is_object());
        
        // Test deserialization
        let deserialized: Tool = serde_json::from_value(serialized).unwrap();
        assert_eq!(deserialized.function.name, "get_weather");
    }

    #[test]
    fn test_tool_choice_serialization() {
        // Test string variant
        let auto_choice = ToolChoiceOption::String("auto".to_string());
        let serialized = serde_json::to_value(&auto_choice).unwrap();
        assert_eq!(serialized, "auto");

        // Test object variant
        let specific_choice = ToolChoiceOption::Object(ToolChoiceSpecific {
            choice_type: "function".to_string(),
            function: ToolChoiceFunctionName {
                name: "get_weather".to_string(),
            },
        });
        let serialized = serde_json::to_value(&specific_choice).unwrap();
        assert_eq!(serialized["type"], "function");
        assert_eq!(serialized["function"]["name"], "get_weather");
    }

    #[test]
    fn test_streaming_tool_call_delta() {
        let delta = ChatCompletionMessageToolCallDelta {
            index: 0,
            id: Some("call_456".to_string()),
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("search".to_string()),
                arguments: Some(r#"{"query": "#.to_string()),
            }),
        };

        let serialized = serde_json::to_value(&delta).unwrap();
        assert_eq!(serialized["index"], 0);
        assert_eq!(serialized["id"], "call_456");
        assert_eq!(serialized["type"], "function");
        assert_eq!(serialized["function"]["name"], "search");

        // Test partial delta (continuation)
        let partial_delta = ChatCompletionMessageToolCallDelta {
            index: 0,
            id: None,
            call_type: None,
            function: Some(ToolCallFunctionDelta {
                name: None,
                arguments: Some(r#"OpenAI tool calling"}"#.to_string()),
            }),
        };

        let serialized = serde_json::to_value(&partial_delta).unwrap();
        assert!(!serialized.as_object().unwrap().contains_key("id"));
        assert!(!serialized.as_object().unwrap().contains_key("type"));
        assert!(serialized["function"]["arguments"].is_string());
    }

    #[test]
    fn test_message_with_tool_calls() {
        let message_json = json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {
                    "id": "call_abc123",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"location\": \"Paris\"}"
                    }
                }
            ]
        });

        let message: ChatCompletionResponseMessage = serde_json::from_value(message_json).unwrap();
        assert_eq!(message.role, ChatCompletionMessageRole::Assistant);
        assert_eq!(message.content, None);
        assert!(message.tool_calls.is_some());
        
        let tool_calls = message.tool_calls.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_abc123");
        assert_eq!(tool_calls[0].function.name, "get_weather");
    }
}
