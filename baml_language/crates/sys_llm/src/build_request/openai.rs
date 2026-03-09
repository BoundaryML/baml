//! OpenAI-format HTTP request builder.
//!
//! Supports: `OpenAi`, `OpenAiGeneric`, `AzureOpenAi`, Ollama, `OpenRouter`,
//! and `OpenAiResponses` (Responses API).

use baml_builtins::{PromptAst, PromptAstSimple};
use indexmap::IndexMap;
use serde::Serialize;

use super::{BuildRequestError, LlmPrimitiveClient, LlmRequestBuilder, get_string_option};
use crate::LlmProvider;

/// A single chat message in the `OpenAI` Chat Completions format.
#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: Vec<ContentPart>,
}

/// A content part within a Chat Completions message.
///
/// Serializes with `{"type": "<variant>", ...}` via `#[serde(tag = "type")]`.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
    InputAudio { input_audio: InputAudio },
    File { file: FileRef },
}

/// URL wrapper for image content parts.
#[derive(Serialize)]
struct ImageUrl {
    url: String,
}

/// Base64-encoded audio data with its format (e.g. "mp3", "wav").
#[derive(Serialize)]
struct InputAudio {
    data: String,
    format: String,
}

/// A file reference that can be specified by URL, inline base64 data, or file ID.
#[derive(Serialize)]
struct FileRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    file_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
}

/// Builder for OpenAI-compatible providers.
pub(crate) struct OpenAiBuilder<'a> {
    provider: &'a LlmProvider,
}

impl<'a> OpenAiBuilder<'a> {
    pub(crate) fn new(provider: &'a LlmProvider) -> Self {
        Self { provider }
    }
}

impl LlmRequestBuilder for OpenAiBuilder<'_> {
    fn provider_skip_keys(&self) -> &'static [&'static str] {
        &["resource_name", "api_version"]
    }

    fn build_url(&self, client: &LlmPrimitiveClient) -> Result<String, BuildRequestError> {
        let base_url = get_string_option(client, "base_url")
            .unwrap_or_else(|| "https://api.openai.com".to_string());

        // Azure uses a different URL pattern
        if *self.provider == LlmProvider::AzureOpenAi {
            let deployment = get_string_option(client, "resource_name")
                .ok_or_else(|| BuildRequestError::MissingOption("resource_name".into()))?;
            let model = get_string_option(client, "model")
                .ok_or_else(|| BuildRequestError::MissingOption("model".into()))?;
            let api_version = get_string_option(client, "api_version")
                .unwrap_or_else(|| "2024-02-15-preview".to_string());
            return Ok(format!(
                "https://{deployment}.openai.azure.com/openai/deployments/{model}/chat/completions?api-version={api_version}"
            ));
        }

        Ok(format!("{base_url}/v1/chat/completions"))
    }

    fn build_auth_headers(&self, client: &LlmPrimitiveClient) -> IndexMap<String, String> {
        let mut headers = IndexMap::new();
        if let Some(api_key) = get_string_option(client, "api_key") {
            if *self.provider == LlmProvider::AzureOpenAi {
                headers.insert("api-key".to_string(), api_key);
            } else {
                headers.insert("authorization".to_string(), format!("Bearer {api_key}"));
            }
        }
        headers
    }

    fn build_body(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
    ) -> Result<String, BuildRequestError> {
        let mut body = serde_json::Map::new();
        if let Some(model) = get_string_option(client, "model") {
            body.insert("model".to_string(), serde_json::Value::String(model));
        }
        body.extend(self.build_prompt_body(client, prompt));
        self.forward_options(client, &mut body);

        // Azure OpenAI: default max_tokens to 4096 if neither max_tokens nor
        // max_completion_tokens is set.
        if *self.provider == LlmProvider::AzureOpenAi {
            if !body.contains_key("max_completion_tokens") && !body.contains_key("max_tokens") {
                body.insert("max_tokens".to_string(), serde_json::json!(4096));
            } else if body
                .get("max_tokens")
                .is_some_and(serde_json::Value::is_null)
            {
                body.remove("max_tokens");
            }
        }

        serde_json::to_string(&body).map_err(|e| BuildRequestError::InvalidOption {
            key: "body".into(),
            reason: e.to_string(),
        })
    }

    fn build_prompt_body(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        let messages = prompt_to_openai_messages(&prompt, &client.default_role);
        map.insert(
            "messages".to_string(),
            serde_json::to_value(messages).expect("infallible"),
        );
        map
    }
}

/// Converts a top-level [`PromptAst`] into a list of `OpenAI` Chat Completions messages.
fn prompt_to_openai_messages(
    prompt: &bex_vm_types::PromptAst,
    default_role: &str,
) -> Vec<serde_json::Value> {
    match prompt.as_ref() {
        PromptAst::Vec(items) => items
            .iter()
            .map(|node| prompt_node_to_message(node, default_role))
            .collect(),
        _ => vec![prompt_node_to_message(prompt, default_role)],
    }
}

/// Converts a single [`PromptAst`] node into an `OpenAI` Chat Completions message JSON value.
///
/// Metadata (e.g. `cache_control`) is merged into the last content part.
fn prompt_node_to_message(node: &bex_vm_types::PromptAst, default_role: &str) -> serde_json::Value {
    match node.as_ref() {
        PromptAst::Message {
            role,
            content,
            metadata,
        } => {
            let parts = openai_content_parts(content.as_ref());
            let mut value = serde_json::to_value(ChatMessage {
                role: role.clone(),
                content: parts,
            })
            .expect("infallible");

            // Apply metadata (e.g., cache_control) to the last content part,
            // matching the engine's WithMeta behavior.
            if let serde_json::Value::Object(meta_map) = metadata {
                if !meta_map.is_empty() {
                    if let Some(content_arr) =
                        value.get_mut("content").and_then(|c| c.as_array_mut())
                    {
                        if let Some(serde_json::Value::Object(last_part)) = content_arr.last_mut() {
                            for (k, v) in meta_map {
                                last_part.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
            }

            value
        }
        PromptAst::Simple(content) => {
            let parts = openai_content_parts(content.as_ref());
            serde_json::to_value(ChatMessage {
                role: default_role.to_string(),
                content: parts,
            })
            .expect("infallible")
        }
        PromptAst::Vec(_) => unreachable!("Nested vecs should not appear after specialization"),
    }
}

/// Converts a [`PromptAstSimple`] content node into Chat Completions content parts.
fn openai_content_parts(content: &PromptAstSimple) -> Vec<ContentPart> {
    match content {
        PromptAstSimple::String(s) => {
            vec![ContentPart::Text { text: s.clone() }]
        }
        PromptAstSimple::Media(media) => media.read_content(|c| openai_media_part(media, c)),
        PromptAstSimple::Multiple(multiple) => multiple
            .iter()
            .flat_map(|i| openai_content_parts(i))
            .collect(),
    }
}

/// Converts a media value into Chat Completions content parts (image, audio, file, etc.).
fn openai_media_part(
    media: &baml_builtins::MediaValue,
    content: &baml_builtins::MediaContent,
) -> Vec<ContentPart> {
    use baml_base::MediaKind;
    use baml_builtins::MediaContent;

    match media.kind {
        MediaKind::Image | MediaKind::Generic => match content {
            MediaContent::Url { url, .. } => {
                vec![ContentPart::ImageUrl {
                    image_url: ImageUrl { url: url.clone() },
                }]
            }
            MediaContent::Base64 { base64_data, .. } => {
                let data_url = format!(
                    "data:{};base64,{}",
                    media.mime_type.as_deref().unwrap_or("image/png"),
                    base64_data
                );
                vec![ContentPart::ImageUrl {
                    image_url: ImageUrl { url: data_url },
                }]
            }
            MediaContent::File { file, .. } => {
                vec![ContentPart::File {
                    file: FileRef {
                        file_id: Some(file.clone()),
                        file_url: None,
                        file_data: None,
                        filename: None,
                    },
                }]
            }
        },
        MediaKind::Audio => match content {
            MediaContent::Base64 { base64_data, .. } => {
                let format = media
                    .mime_type
                    .as_deref()
                    .and_then(|m| m.strip_prefix("audio/"))
                    .map(|ext| if ext == "mpeg" { "mp3" } else { ext })
                    .unwrap_or("mp3");
                vec![ContentPart::InputAudio {
                    input_audio: InputAudio {
                        data: base64_data.clone(),
                        format: format.to_string(),
                    },
                }]
            }
            MediaContent::Url { url, .. } => {
                let extension = url
                    .rsplit('.')
                    .next()
                    .map(|ext| if ext == "mpeg" { "mp3" } else { ext })
                    .or(media
                        .mime_type
                        .as_deref()
                        .and_then(|m| m.strip_prefix("audio/")))
                    .unwrap_or("mp3");
                vec![ContentPart::InputAudio {
                    input_audio: InputAudio {
                        data: url.clone(),
                        format: extension.to_string(),
                    },
                }]
            }
            MediaContent::File { file, .. } => {
                vec![ContentPart::File {
                    file: FileRef {
                        file_id: Some(file.clone()),
                        file_url: None,
                        file_data: None,
                        filename: None,
                    },
                }]
            }
        },
        MediaKind::Pdf => match content {
            MediaContent::Url { url, .. } => {
                vec![ContentPart::File {
                    file: FileRef {
                        file_url: Some(url.clone()),
                        filename: Some("document.pdf".to_string()),
                        file_data: None,
                        file_id: None,
                    },
                }]
            }
            MediaContent::Base64 { base64_data, .. } => {
                let data_url = format!(
                    "data:{};base64,{}",
                    media.mime_type.as_deref().unwrap_or("application/pdf"),
                    base64_data
                );
                vec![ContentPart::File {
                    file: FileRef {
                        file_data: Some(data_url),
                        filename: Some("document.pdf".to_string()),
                        file_url: None,
                        file_id: None,
                    },
                }]
            }
            MediaContent::File { file, .. } => {
                vec![ContentPart::File {
                    file: FileRef {
                        file_id: Some(file.clone()),
                        file_url: None,
                        file_data: None,
                        filename: None,
                    },
                }]
            }
        },
        MediaKind::Video => {
            vec![ContentPart::Text {
                text: "[unsupported: video input is not supported on OpenAI chat completions]"
                    .to_string(),
            }]
        }
    }
}

/// A single message in the `OpenAI` Responses API format.
#[derive(Serialize)]
struct ResponsesMessage {
    role: String,
    content: Vec<ResponsesContentPart>,
}

/// A content part within a Responses API message.
///
/// Uses `input_text`/`output_text` instead of just `text`, and `input_image`/`input_audio`/`input_file`
/// instead of `image_url`/`input_audio`/`file`.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ResponsesContentPart {
    InputText {
        text: String,
    },
    OutputText {
        text: String,
    },
    InputImage {
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        image_url: String,
    },
    InputAudio {
        input_audio: InputAudio,
    },
    InputFile {
        #[serde(skip_serializing_if = "Option::is_none")]
        file_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
    },
}

/// Builder for the `OpenAI` Responses API (`/v1/responses`).
///
/// Uses `"input"` instead of `"messages"` and different content part types:
/// `input_text`, `output_text`, `input_image`, `input_audio`, `input_file`.
pub(crate) struct OpenAiResponsesBuilder;

impl LlmRequestBuilder for OpenAiResponsesBuilder {
    fn provider_skip_keys(&self) -> &'static [&'static str] {
        &[]
    }

    fn build_url(&self, client: &LlmPrimitiveClient) -> Result<String, BuildRequestError> {
        let base_url = get_string_option(client, "base_url")
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        Ok(format!("{base_url}/responses"))
    }

    fn build_auth_headers(&self, client: &LlmPrimitiveClient) -> IndexMap<String, String> {
        let mut headers = IndexMap::new();
        if let Some(api_key) = get_string_option(client, "api_key") {
            headers.insert("authorization".to_string(), format!("Bearer {api_key}"));
        }
        headers
    }

    fn build_prompt_body(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        let input = prompt_to_responses_input(&prompt, &client.default_role);
        map.insert(
            "input".to_string(),
            serde_json::to_value(input).expect("infallible"),
        );
        map
    }
}

/// Converts a top-level [`PromptAst`] into a list of Responses API input messages.
fn prompt_to_responses_input(
    prompt: &bex_vm_types::PromptAst,
    default_role: &str,
) -> Vec<ResponsesMessage> {
    match prompt.as_ref() {
        PromptAst::Vec(items) => items
            .iter()
            .map(|node| responses_node_to_message(node, default_role))
            .collect(),
        _ => vec![responses_node_to_message(prompt, default_role)],
    }
}

/// Converts a single [`PromptAst`] node into a Responses API input message.
fn responses_node_to_message(
    node: &bex_vm_types::PromptAst,
    default_role: &str,
) -> ResponsesMessage {
    match node.as_ref() {
        PromptAst::Message { role, content, .. } => {
            let parts = responses_content_parts(content.as_ref(), role);
            ResponsesMessage {
                role: role.clone(),
                content: parts,
            }
        }
        PromptAst::Simple(content) => {
            let parts = responses_content_parts(content.as_ref(), default_role);
            ResponsesMessage {
                role: default_role.to_string(),
                content: parts,
            }
        }
        PromptAst::Vec(_) => unreachable!("Nested vecs should not appear after specialization"),
    }
}

/// Converts a [`PromptAstSimple`] content node into Responses API content parts.
///
/// Assistant-role text uses `output_text`; all other roles use `input_text`.
fn responses_content_parts(content: &PromptAstSimple, role: &str) -> Vec<ResponsesContentPart> {
    match content {
        PromptAstSimple::String(s) => {
            if role == "assistant" {
                vec![ResponsesContentPart::OutputText { text: s.clone() }]
            } else {
                vec![ResponsesContentPart::InputText { text: s.clone() }]
            }
        }
        PromptAstSimple::Media(media) => media.read_content(|c| responses_media_part(media, c)),
        PromptAstSimple::Multiple(multiple) => multiple
            .iter()
            .flat_map(|i| responses_content_parts(i, role))
            .collect(),
    }
}

/// Converts a media value into Responses API content parts (`input_image`, `input_audio`, `input_file`).
fn responses_media_part(
    media: &baml_builtins::MediaValue,
    content: &baml_builtins::MediaContent,
) -> Vec<ResponsesContentPart> {
    use baml_base::MediaKind;
    use baml_builtins::MediaContent;

    match media.kind {
        MediaKind::Image | MediaKind::Generic => {
            let image_url = match content {
                MediaContent::Url { url, .. } => url.clone(),
                MediaContent::Base64 { base64_data, .. } => {
                    format!(
                        "data:{};base64,{}",
                        media.mime_type.as_deref().unwrap_or("image/png"),
                        base64_data
                    )
                }
                MediaContent::File { file, .. } => file.clone(),
            };
            vec![ResponsesContentPart::InputImage {
                detail: Some("auto".to_string()),
                image_url,
            }]
        }
        MediaKind::Audio => match content {
            MediaContent::Base64 { base64_data, .. } => {
                let format = media
                    .mime_type
                    .as_deref()
                    .and_then(|m| m.strip_prefix("audio/"))
                    .map(|ext| if ext == "mpeg" { "mp3" } else { ext })
                    .unwrap_or("mp3");
                vec![ResponsesContentPart::InputAudio {
                    input_audio: InputAudio {
                        data: base64_data.clone(),
                        format: format.to_string(),
                    },
                }]
            }
            MediaContent::Url { url, .. } => {
                let format = url
                    .rsplit('.')
                    .next()
                    .map(|ext| if ext == "mpeg" { "mp3" } else { ext })
                    .or(media
                        .mime_type
                        .as_deref()
                        .and_then(|m| m.strip_prefix("audio/")))
                    .unwrap_or("mp3");
                vec![ResponsesContentPart::InputAudio {
                    input_audio: InputAudio {
                        data: url.clone(),
                        format: format.to_string(),
                    },
                }]
            }
            MediaContent::File { file, .. } => {
                vec![ResponsesContentPart::InputFile {
                    file_id: Some(file.clone()),
                    file_url: None,
                    file_data: None,
                    filename: None,
                }]
            }
        },
        MediaKind::Pdf => match content {
            MediaContent::Url { url, .. } => {
                vec![ResponsesContentPart::InputFile {
                    file_url: Some(url.clone()),
                    filename: Some("document.pdf".to_string()),
                    file_data: None,
                    file_id: None,
                }]
            }
            MediaContent::Base64 { base64_data, .. } => {
                let data_url = format!(
                    "data:{};base64,{}",
                    media.mime_type.as_deref().unwrap_or("application/pdf"),
                    base64_data
                );
                vec![ResponsesContentPart::InputFile {
                    file_data: Some(data_url),
                    filename: Some("document.pdf".to_string()),
                    file_url: None,
                    file_id: None,
                }]
            }
            MediaContent::File { file, .. } => {
                vec![ResponsesContentPart::InputFile {
                    file_id: Some(file.clone()),
                    file_url: None,
                    file_data: None,
                    filename: None,
                }]
            }
        },
        MediaKind::Video => {
            vec![ResponsesContentPart::InputText {
                text: "[unsupported: video input is not supported on OpenAI Responses API]"
                    .to_string(),
            }]
        }
    }
}
