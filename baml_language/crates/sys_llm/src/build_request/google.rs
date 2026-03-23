//! Shared Vertex AI / Google AI (Gemini) HTTP request builder.
//!
//! Both providers use the same `GenerateContentRequest` body format. Only the
//! URL and auth differ:
//!   - Vertex AI: `https://{location}-aiplatform.googleapis.com/v1/...`  (`OAuth2`)
//!   - Google AI: `https://generativelanguage.googleapis.com/v1beta/...` (API key)
//!
//! Body serialization uses the official `google-cloud-aiplatform-v1` crate's
//! typed `GenerateContentRequest` struct.
//!
//! Auth is NOT handled here -- `auth_request` will be responsible for adding
//! `OAuth2` bearer tokens or API-key query params.

use baml_base::MediaKind;
use baml_builtins::{MediaContent, PromptAst, PromptAstSimple};
use google_cloud_aiplatform_v1::model::{self as gcp, part};
use indexmap::IndexMap;

use super::{
    BuildRequestCallbacks, BuildRequestError, LlmPrimitiveClient, LlmRequestBuilder,
    RawHttpRequest, build_headers, forward_options, get_string_option, mime_type_as_ok,
};

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

        // Build typed GenerateContentRequest from the prompt.
        let (system_instruction, contents) = prompt_to_gcp_types(prompt, &client.default_role)?;

        let mut req = gcp::GenerateContentRequest::new()
            .set_model(model)
            .set_contents(contents);

        if let Some(si) = system_instruction {
            req = req.set_system_instruction(si);
        }

        // Serialize the typed request to a JSON map, then merge forwarded options.
        let mut body: serde_json::Map<String, serde_json::Value> = serde_json::to_value(&req)
            .map_err(|e| BuildRequestError::BodySerialization(e.to_string()))?
            .as_object()
            .cloned()
            .unwrap_or_default();

        // The SDK always emits "model" in the body but the REST API takes it
        // in the URL, not the body. Remove it to match what the API expects.
        body.remove("model");

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
// BAML PromptAst -> GCP SDK type conversions
// ---------------------------------------------------------------------------

/// Convert a BAML `PromptAst` into GCP `Content` types for the request.
///
/// Returns `(system_instruction, contents)` where system messages are collected
/// into a single `Content` for `systemInstruction` and non-system messages
/// become the `contents` array.
fn prompt_to_gcp_types(
    prompt: bex_vm_types::PromptAst,
    default_role: &str,
) -> Result<(Option<gcp::Content>, Vec<gcp::Content>), BuildRequestError> {
    let mut system_parts: Vec<gcp::Part> = Vec::new();
    let mut contents: Vec<gcp::Content> = Vec::new();

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
                contents.push(gcp::Content::new().set_role(role.clone()).set_parts(parts));
            }
            PromptAst::Simple(content) => {
                let parts = content_to_parts(content)?;
                contents.push(
                    gcp::Content::new()
                        .set_role(default_role.to_string())
                        .set_parts(parts),
                );
            }
            PromptAst::Vec(_) => unreachable!(),
        }
    }

    let system_instruction = if system_parts.is_empty() {
        None
    } else {
        Some(gcp::Content::new().set_parts(system_parts))
    };

    Ok((system_instruction, contents))
}

/// Convert a `PromptAstSimple` content node into GCP `Part` values.
fn content_to_parts(content: &PromptAstSimple) -> Result<Vec<gcp::Part>, BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => {
            Ok(vec![gcp::Part::new().set_data(part::Data::Text(s.clone()))])
        }
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

/// Convert a media value into a GCP `Part` (`InlineData` or `FileData`).
fn media_to_part(
    media: &baml_builtins::MediaValue,
    content: &MediaContent,
) -> Result<Vec<gcp::Part>, BuildRequestError> {
    match content {
        MediaContent::Url { url, .. } => {
            let mut fd = gcp::FileData::new().set_file_uri(url.clone());
            if let Some(mime) = &media.mime_type {
                fd = fd.set_mime_type(mime.clone());
            }
            Ok(vec![
                gcp::Part::new().set_data(part::Data::FileData(Box::new(fd))),
            ])
        }
        MediaContent::Base64 { base64_data, .. }
        | MediaContent::File {
            base64_data: Some(base64_data),
            ..
        } => {
            let mime = mime_type_as_ok(media)?.to_string();
            let raw_bytes =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64_data)
                    .map_err(|e| BuildRequestError::InvalidOption {
                        key: "media".into(),
                        reason: format!("invalid base64: {e}"),
                    })?;
            let blob = gcp::Blob::new().set_mime_type(mime).set_data(raw_bytes);
            Ok(vec![
                gcp::Part::new().set_data(part::Data::InlineData(Box::new(blob))),
            ])
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
    // Body: assistant role remapped to model
    // ========================================================================

    #[tokio::test]
    // Roles are passed through as-is. The upstream compiler is responsible for
    // remapping "assistant" -> "model" via allowed_roles / default_role config.
    // This will be handled by compiler2's role remapping.
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
