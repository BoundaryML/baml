//! Shared Vertex AI / Google AI (Gemini) HTTP request builder.
//!
//! Both providers use the same `GenerateContentRequest` body format. Only the
//! URL and auth differ:
//!   - Vertex AI: `https://{location}-aiplatform.googleapis.com/v1/...`  (`OAuth2`)
//!   - Google AI: `https://generativelanguage.googleapis.com/v1beta/...` (API key)
//!
//! Body serialization mirrors the `GenerateContentRequest` schema from the
//! Google Cloud AI Platform v1 REST API. The types here are local serde structs
//! (not the `google-cloud-aiplatform-v1` crate) so the builder works on wasm32.
//!
//! Auth is NOT handled here -- `auth_request` will be responsible for adding
//! `OAuth2` bearer tokens or API-key query params.

use baml_base::MediaKind;
use baml_builtins::{MediaContent, PromptAst, PromptAstSimple};
use indexmap::IndexMap;
use serde::Serialize;

use super::{
    BuildRequestCallbacks, BuildRequestError, LlmPrimitiveClient, LlmRequestBuilder,
    RawHttpRequest, build_headers, forward_options, get_string_option, mime_type_as_ok,
};

// ---------------------------------------------------------------------------
// GenerateContentRequest serde types (camelCase, matching the REST API)
// ---------------------------------------------------------------------------

/// Top-level request body for `generateContent`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentRequest {
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<Content>,
}

/// A message in the conversation.
#[derive(Debug, Serialize)]
struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<Part>,
}

/// A single part within a `Content` message.
///
/// Serializes as exactly one of `{ "text": "..." }`,
/// `{ "inlineData": { ... } }`, or `{ "fileData": { ... } }`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum Part {
    Text(String),
    InlineData(Blob),
    FileData(FileData),
}

/// Inline binary data (base64-encoded by serde via the REST API convention).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Blob {
    mime_type: String,
    /// Base64-encoded data -- passed through as-is since BAML already stores it
    /// in base64 form.
    data: String,
}

/// Reference to a file by URI (e.g. `gs://` or `https://`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileData {
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
    file_uri: String,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builder for Google Cloud Vertex AI and Google AI (Gemini) providers.
pub(crate) struct GoogleBuilder;

/// Option keys consumed by the builder that should not be forwarded to the body.
const GOOGLE_SKIP_KEYS: &[&str] = &[
    "project_id",
    "location",
    "credentials",
    "credentials_content",
];

impl LlmRequestBuilder for GoogleBuilder {
    async fn build_request(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
        stream: bool,
        _callbacks: &BuildRequestCallbacks<'_>,
    ) -> Result<RawHttpRequest, BuildRequestError> {
        let model = get_string_option(client, "model")
            .ok_or_else(|| BuildRequestError::MissingOption("model".into()))?;

        let url = build_url(client, &model, stream)?;
        let headers = build_headers(IndexMap::new(), client);

        let (system_instruction, contents) =
            extract_system_and_contents(prompt, &client.default_role)?;

        let req = GenerateContentRequest {
            contents,
            system_instruction,
        };

        // Serialize the typed request to a JSON map, then merge forwarded options.
        let mut body: serde_json::Map<String, serde_json::Value> = serde_json::to_value(&req)
            .map_err(|e| BuildRequestError::BodySerialization(e.to_string()))?
            .as_object()
            .cloned()
            .unwrap_or_default();

        forward_options(GOOGLE_SKIP_KEYS, client, &mut body);

        let body_str = serde_json::to_string(&body)
            .map_err(|e| BuildRequestError::BodySerialization(e.to_string()))?;

        Ok(RawHttpRequest {
            method: "POST".to_string(),
            url,
            headers,
            body: body_str,
        })
    }
}

// ---------------------------------------------------------------------------
// URL construction
// ---------------------------------------------------------------------------

/// Build the Vertex AI URL for Anthropic-on-Vertex (rawPredict/streamRawPredict).
///
/// Claude models on Vertex use a different RPC than Gemini models.
pub(super) fn vertex_anthropic_url(
    client: &LlmPrimitiveClient,
    stream: bool,
) -> Result<String, BuildRequestError> {
    let model = get_string_option(client, "model")
        .ok_or_else(|| BuildRequestError::MissingOption("model".into()))?;
    let rpc = if stream {
        "streamRawPredict"
    } else {
        "rawPredict"
    };

    if let Some(base_url) = get_string_option(client, "base_url") {
        let base = base_url.trim_end_matches('/');
        return Ok(format!("{base}/models/{model}:{rpc}"));
    }

    let location = get_string_option(client, "location")
        .ok_or_else(|| BuildRequestError::MissingOption("location".into()))?;
    let project_id = get_string_option(client, "project_id")
        .ok_or_else(|| BuildRequestError::MissingOption("project_id".into()))?;

    let domain = if location == "global" {
        "aiplatform.googleapis.com".to_string()
    } else {
        format!("{location}-aiplatform.googleapis.com")
    };

    Ok(format!(
        "https://{domain}/v1/projects/{project_id}/locations/{location}/publishers/google/models/{model}:{rpc}"
    ))
}

/// Build the request URL for Vertex AI or Google AI.
///
/// - Vertex AI: `https://{location}-aiplatform.googleapis.com/v1/projects/{project}/locations/{location}/publishers/google/models/{model}:{rpc}`
/// - Google AI: `https://generativelanguage.googleapis.com/v1beta/models/{model}:{rpc}`
fn build_url(
    client: &LlmPrimitiveClient,
    model: &str,
    stream: bool,
) -> Result<String, BuildRequestError> {
    let rpc = if stream {
        "streamGenerateContent?alt=sse"
    } else {
        "generateContent"
    };

    // If base_url is set explicitly, use it directly (works for both providers).
    if let Some(base_url) = get_string_option(client, "base_url") {
        let base = base_url.trim_end_matches('/');
        return Ok(format!("{base}/models/{model}:{rpc}"));
    }

    // Google AI uses a flat URL with just the model name.
    if client.provider == "google-ai" {
        return Ok(format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:{rpc}"
        ));
    }

    // Vertex AI needs location + project_id.
    let location = get_string_option(client, "location")
        .ok_or_else(|| BuildRequestError::MissingOption("location".into()))?;

    let project_id = get_string_option(client, "project_id")
        .ok_or_else(|| BuildRequestError::MissingOption("project_id".into()))?;

    let domain = if location == "global" {
        "aiplatform.googleapis.com".to_string()
    } else {
        format!("{location}-aiplatform.googleapis.com")
    };

    Ok(format!(
        "https://{domain}/v1/projects/{project_id}/locations/{location}/publishers/google/models/{model}:{rpc}"
    ))
}

// ---------------------------------------------------------------------------
// BAML PromptAst -> GenerateContentRequest conversion
// ---------------------------------------------------------------------------

/// Extract system instruction and contents from a `PromptAst`.
///
/// System messages are collected into a single `Content` for `systemInstruction`.
/// Non-system messages become entries in the `contents` array.
///
/// Roles are passed through as-is. The upstream compiler is responsible for
/// remapping "assistant" -> "model" via `allowed_roles` / `default_role` config.
/// This will be handled by compiler2's role remapping.
fn extract_system_and_contents(
    prompt: bex_vm_types::PromptAst,
    default_role: &str,
) -> Result<(Option<Content>, Vec<Content>), BuildRequestError> {
    let mut system_parts: Vec<Part> = Vec::new();
    let mut contents: Vec<Content> = Vec::new();

    let items = match prompt.as_ref() {
        PromptAst::Vec(v) => v.clone(),
        _ => vec![prompt],
    };

    for item in &items {
        match item.as_ref() {
            PromptAst::Message {
                role,
                content,
                metadata: _,
            } if role == "system" => {
                system_parts.extend(content_to_parts(content)?);
            }
            PromptAst::Message {
                role,
                content,
                metadata: _,
            } => {
                let parts = content_to_parts(content)?;
                contents.push(Content {
                    role: Some(role.clone()),
                    parts,
                });
            }
            PromptAst::Simple(content) => {
                let parts = content_to_parts(content)?;
                contents.push(Content {
                    role: Some(default_role.to_string()),
                    parts,
                });
            }
            PromptAst::Vec(_) => unreachable!(),
        }
    }

    let system_instruction = if system_parts.is_empty() {
        None
    } else {
        Some(Content {
            role: None,
            parts: system_parts,
        })
    };

    Ok((system_instruction, contents))
}

/// Convert a `PromptAstSimple` content node into `Part` values.
fn content_to_parts(content: &PromptAstSimple) -> Result<Vec<Part>, BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => Ok(vec![Part::Text(s.clone())]),
        PromptAstSimple::Media(media) => media.read_content(|c| media_to_part(media, c)),
        PromptAstSimple::Multiple(items) => {
            let mut parts = Vec::new();
            for item in items {
                parts.extend(content_to_parts(item)?);
            }
            Ok(parts)
        }
    }
}

/// Convert a media value into a `Part` (`InlineData` or `FileData`).
fn media_to_part(
    media: &baml_builtins::MediaValue,
    content: &MediaContent,
) -> Result<Vec<Part>, BuildRequestError> {
    match content {
        MediaContent::Url { url, .. } => {
            let mime_type = media.mime_type.clone();
            Ok(vec![Part::FileData(FileData {
                mime_type,
                file_uri: url.clone(),
            })])
        }
        MediaContent::Base64 { base64_data, .. }
        | MediaContent::File {
            base64_data: Some(base64_data),
            ..
        } => {
            let mime_type = mime_type_as_ok(media)?.to_string();
            Ok(vec![Part::InlineData(Blob {
                mime_type,
                data: base64_data.clone(),
            })])
        }
        MediaContent::File {
            base64_data: None, ..
        } => {
            let label = match media.kind {
                MediaKind::Image => "image",
                MediaKind::Audio => "audio",
                MediaKind::Video => "video",
                MediaKind::Pdf => "pdf",
                MediaKind::Generic => "media",
            };
            Err(BuildRequestError::FileNotResolved(format!(
                "{label} file content was not resolved properly"
            )))
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_base::MediaKind;
    use baml_builtins::{MediaContent, MediaValue, PromptAst};
    use bex_external_types::BexExternalValue;
    use indexmap::IndexMap;

    use super::*;
    use crate::build_request::{LlmPrimitiveClient, LlmRequestBuilder};

    // -- helpers --

    fn make_media(kind: MediaKind, content: MediaContent, mime: Option<&str>) -> MediaValue {
        MediaValue::new(kind, content, mime.map(String::from))
    }

    fn make_vertex_client(options: Vec<(&str, BexExternalValue)>) -> LlmPrimitiveClient {
        let mut opts = IndexMap::new();
        for (k, v) in options {
            opts.insert(k.to_string(), v);
        }
        LlmPrimitiveClient {
            name: "test".to_string(),
            provider: "vertex-ai".to_string(),
            default_role: "user".to_string(),
            allowed_roles: vec!["system".into(), "user".into(), "assistant".into()],
            options: opts,
        }
    }

    fn make_google_ai_client(options: Vec<(&str, BexExternalValue)>) -> LlmPrimitiveClient {
        let mut opts = IndexMap::new();
        for (k, v) in options {
            opts.insert(k.to_string(), v);
        }
        LlmPrimitiveClient {
            name: "test".to_string(),
            provider: "google-ai".to_string(),
            default_role: "user".to_string(),
            allowed_roles: vec!["system".into(), "user".into(), "assistant".into()],
            options: opts,
        }
    }

    fn msg(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(text.to_string().into()),
            metadata: serde_json::Value::Null,
        })
    }

    fn parse_body(body: &str) -> serde_json::Value {
        serde_json::from_str(body).unwrap()
    }

    async fn build_raw(
        client: &LlmPrimitiveClient,
        prompt: Arc<PromptAst>,
        stream: bool,
    ) -> Result<RawHttpRequest, BuildRequestError> {
        let (h, e, f) = crate::build_request::stub_callbacks();
        let callbacks = crate::build_request::BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        GoogleBuilder
            .build_request(client, prompt, stream, &callbacks)
            .await
    }

    fn vertex_opts() -> Vec<(&'static str, BexExternalValue)> {
        vec![
            ("model", BexExternalValue::String("gemini-1.5-pro".into())),
            ("location", BexExternalValue::String("us-central1".into())),
            ("project_id", BexExternalValue::String("my-project".into())),
        ]
    }

    // ========================================================================
    // URL construction
    // ========================================================================

    #[tokio::test]
    async fn vertex_url_with_location_and_project() {
        let client = make_vertex_client(vertex_opts());
        let prompt = msg("user", "Hello");
        let result = build_raw(&client, prompt, false).await.unwrap();
        assert_eq!(result.method, "POST");
        assert_eq!(
            result.url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models/gemini-1.5-pro:generateContent"
        );
    }

    #[tokio::test]
    async fn vertex_url_global_location() {
        let client = make_vertex_client(vec![
            ("model", BexExternalValue::String("gemini-1.5-pro".into())),
            ("location", BexExternalValue::String("global".into())),
            ("project_id", BexExternalValue::String("my-project".into())),
        ]);
        let prompt = msg("user", "Hello");
        let result = build_raw(&client, prompt, false).await.unwrap();
        assert_eq!(
            result.url,
            "https://aiplatform.googleapis.com/v1/projects/my-project/locations/global/publishers/google/models/gemini-1.5-pro:generateContent"
        );
    }

    #[tokio::test]
    async fn vertex_stream_url() {
        let client = make_vertex_client(vertex_opts());
        let prompt = msg("user", "Hello");
        let result = build_raw(&client, prompt, true).await.unwrap();
        assert!(result.url.ends_with(":streamGenerateContent?alt=sse"));
    }

    #[tokio::test]
    async fn vertex_custom_base_url() {
        let client = make_vertex_client(vec![
            ("model", BexExternalValue::String("gemini-1.5-pro".into())),
            (
                "base_url",
                BexExternalValue::String("https://custom.endpoint.com/v1".into()),
            ),
        ]);
        let prompt = msg("user", "Hello");
        let result = build_raw(&client, prompt, false).await.unwrap();
        assert_eq!(
            result.url,
            "https://custom.endpoint.com/v1/models/gemini-1.5-pro:generateContent"
        );
    }

    #[tokio::test]
    async fn vertex_missing_location_error() {
        let client = make_vertex_client(vec![(
            "model",
            BexExternalValue::String("gemini-1.5-pro".into()),
        )]);
        let prompt = msg("user", "Hello");
        let err = build_raw(&client, prompt, false).await.unwrap_err();
        assert!(matches!(err, BuildRequestError::MissingOption(_)));
        assert!(err.to_string().contains("location"));
    }

    #[tokio::test]
    async fn google_ai_default_url() {
        let client = make_google_ai_client(vec![(
            "model",
            BexExternalValue::String("gemini-1.5-flash".into()),
        )]);
        let prompt = msg("user", "Hello");
        let result = build_raw(&client, prompt, false).await.unwrap();
        assert_eq!(
            result.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-flash:generateContent"
        );
    }

    #[tokio::test]
    async fn google_ai_single_user_message() {
        let client = make_google_ai_client(vec![(
            "model",
            BexExternalValue::String("gemini-1.5-flash".into()),
        )]);
        let prompt = msg("user", "Hello");
        let result = build_raw(&client, prompt, false).await.unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [{ "text": "Hello" }]
                    }
                ]
            })
        );
    }

    // ========================================================================
    // Body: single user message
    // ========================================================================

    #[tokio::test]
    async fn vertex_single_user_message() {
        let client = make_vertex_client(vertex_opts());
        let prompt = msg("user", "Hello");
        let result = build_raw(&client, prompt, false).await.unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [{ "text": "Hello" }]
                    }
                ]
            })
        );
    }

    // ========================================================================
    // Body: system instruction extracted
    // ========================================================================

    #[tokio::test]
    async fn vertex_system_instruction_extracted() {
        let client = make_vertex_client(vertex_opts());
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "What is 2+2?"),
        ]));
        let result = build_raw(&client, prompt, false).await.unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "systemInstruction": {
                    "parts": [{ "text": "You are a helpful assistant." }]
                },
                "contents": [
                    {
                        "role": "user",
                        "parts": [{ "text": "What is 2+2?" }]
                    }
                ]
            })
        );
    }

    // ========================================================================
    // Body: roles passed through as-is
    // ========================================================================

    // Roles are passed through as-is. The upstream compiler is responsible for
    // remapping "assistant" -> "model" via allowed_roles / default_role config.
    // This will be handled by compiler2's role remapping.
    #[tokio::test]
    async fn vertex_roles_passed_through() {
        let client = make_vertex_client(vertex_opts());
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("user", "Hello"),
            msg("assistant", "Hi!"),
            msg("user", "How are you?"),
        ]));
        let result = build_raw(&client, prompt, false).await.unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    { "role": "user", "parts": [{ "text": "Hello" }] },
                    { "role": "assistant", "parts": [{ "text": "Hi!" }] },
                    { "role": "user", "parts": [{ "text": "How are you?" }] }
                ]
            })
        );
    }

    // ========================================================================
    // Body: multiple system messages combined
    // ========================================================================

    #[tokio::test]
    async fn vertex_multiple_system_messages() {
        let client = make_vertex_client(vertex_opts());
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "First instruction."),
            msg("system", "Second instruction."),
            msg("user", "Hello"),
        ]));
        let result = build_raw(&client, prompt, false).await.unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "systemInstruction": {
                    "parts": [
                        { "text": "First instruction." },
                        { "text": "Second instruction." }
                    ]
                },
                "contents": [
                    { "role": "user", "parts": [{ "text": "Hello" }] }
                ]
            })
        );
    }

    // ========================================================================
    // Body: options forwarded, internal options skipped
    // ========================================================================

    #[tokio::test]
    async fn vertex_forwards_options() {
        let mut opts = vertex_opts();
        opts.push(("temperature", BexExternalValue::Float(0.7)));
        let client = make_vertex_client(opts);
        let prompt = msg("user", "Hello");
        let result = build_raw(&client, prompt, false).await.unwrap();
        let body = parse_body(&result.body);
        assert_eq!(body["temperature"], 0.7);
    }

    #[tokio::test]
    async fn vertex_skips_internal_options() {
        let client = make_vertex_client(vertex_opts());
        let prompt = msg("user", "Hello");
        let result = build_raw(&client, prompt, false).await.unwrap();
        let body = parse_body(&result.body);
        assert!(body.get("location").is_none());
        assert!(body.get("project_id").is_none());
        assert!(body.get("model").is_none());
        assert!(body.get("base_url").is_none());
    }

    // ========================================================================
    // Media: inline data (base64)
    // ========================================================================

    #[tokio::test]
    async fn vertex_image_base64() {
        let client = make_vertex_client(vertex_opts());
        let media = make_media(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "iVBORw0KGgo=".into(),
            },
            Some("image/png"),
        );
        let prompt = Arc::new(PromptAst::Message {
            role: "user".to_string(),
            content: Arc::new(baml_builtins::PromptAstSimple::Multiple(vec![
                Arc::new(baml_builtins::PromptAstSimple::String(
                    "What is in this image?".into(),
                )),
                Arc::new(baml_builtins::PromptAstSimple::Media(Arc::new(media))),
            ])),
            metadata: serde_json::Value::Null,
        });
        let result = build_raw(&client, prompt, false).await.unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            { "text": "What is in this image?" },
                            { "inlineData": { "mimeType": "image/png", "data": "iVBORw0KGgo=" } }
                        ]
                    }
                ]
            })
        );
    }

    // ========================================================================
    // Media: file URI
    // ========================================================================

    #[tokio::test]
    async fn vertex_image_url() {
        let client = make_vertex_client(vertex_opts());
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "gs://bucket/image.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let prompt = Arc::new(PromptAst::Message {
            role: "user".to_string(),
            content: Arc::new(baml_builtins::PromptAstSimple::Media(Arc::new(media))),
            metadata: serde_json::Value::Null,
        });
        let result = build_raw(&client, prompt, false).await.unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            { "fileData": { "mimeType": "image/png", "fileUri": "gs://bucket/image.png" } }
                        ]
                    }
                ]
            })
        );
    }

    // ========================================================================
    // Media: file not resolved error
    // ========================================================================

    #[tokio::test]
    async fn vertex_file_not_resolved_error() {
        let client = make_vertex_client(vertex_opts());
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "cat.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let prompt = Arc::new(PromptAst::Message {
            role: "user".to_string(),
            content: Arc::new(baml_builtins::PromptAstSimple::Media(Arc::new(media))),
            metadata: serde_json::Value::Null,
        });
        let err = build_raw(&client, prompt, false).await.unwrap_err();
        assert!(matches!(err, BuildRequestError::FileNotResolved(_)));
    }

    // ========================================================================
    // Media: video supported (unlike Anthropic)
    // ========================================================================

    #[tokio::test]
    async fn vertex_video_url() {
        let client = make_vertex_client(vertex_opts());
        let media = make_media(
            MediaKind::Video,
            MediaContent::Url {
                url: "gs://bucket/clip.mp4".into(),
                base64_data: None,
            },
            Some("video/mp4"),
        );
        let prompt = Arc::new(PromptAst::Message {
            role: "user".to_string(),
            content: Arc::new(baml_builtins::PromptAstSimple::Media(Arc::new(media))),
            metadata: serde_json::Value::Null,
        });
        let result = build_raw(&client, prompt, false).await.unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body["contents"][0]["parts"][0]["fileData"]["fileUri"],
            "gs://bucket/clip.mp4"
        );
    }
}
