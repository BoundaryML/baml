//! Google Gemini API body builder.
//!
//! Builds the JSON body for the `generateContent` endpoint used by both
//! Google AI (generativelanguage.googleapis.com) and Vertex AI
//! (aiplatform.googleapis.com). The request body format is identical for both
//! providers; only URL construction and authentication differ.
//!
//! System messages are extracted to a top-level `"systemInstruction"` field as
//! required by the Gemini API. The role `"assistant"` is remapped to `"model"`.

use std::sync::Arc;

use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
use serde::Serialize;

use crate::LlmProvider;

// ============================================================================
// Serde types for the Gemini generateContent request
// ============================================================================

/// A content part within a message. Uses `#[serde(flatten)]` to emit whichever
/// variant fields are present, matching the Gemini `Part` union.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_data: Option<InlineData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_data: Option<FileData>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileData {
    mime_type: String,
    file_uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<Part>,
}

// ============================================================================
// Request body
// ============================================================================

/// Full request body for `generateContent`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestBody {
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<Content>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

// ============================================================================
// Request builder
// ============================================================================

pub(crate) fn build_request(
    client: &crate::baml_std::PrimitiveClient,
    prompt: &bex_vm_types::PromptAst,
    provider: LlmProvider,
) -> Result<crate::baml_std::HttpRequest, super::BuildRequestError> {
    let mut headers = indexmap::IndexMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());

    // Body
    let (system_instruction, contents) = extract_system_and_contents(prompt)?;

    let body = RequestBody {
        contents,
        system_instruction,
        extra: client.extra_body.clone(),
    };
    let body_str = serde_json::to_string(&body)?;

    Ok(crate::baml_std::HttpRequest {
        method: "POST".to_string(),
        url: resolve_url(client, provider)?,
        headers,
        body: body_str,
    })
}

// ============================================================================
// URL construction
// ============================================================================

fn resolve_url(
    client: &crate::baml_std::PrimitiveClient,
    provider: LlmProvider,
) -> Result<String, super::BuildRequestError> {
    match provider {
        // Google AI: {base_url}/models/{model}:generateContent
        LlmProvider::GoogleAi => {
            let base = client
                .options
                .base_url
                .as_deref()
                .unwrap_or("https://generativelanguage.googleapis.com/v1beta");
            Ok(format!("{base}/models/{}:generateContent", client.model))
        }
        // Vertex AI: {base_url}/{model}:generateContent
        // base_url is either explicit, or constructed from location + project_id.
        // If project_id is not yet known, a placeholder is used and resolved
        // during auth (see auth_request/vertex.rs).
        LlmProvider::VertexAi => {
            let base = match client.options.base_url.as_deref() {
                Some(url) => url.to_string(),
                None => resolve_vertex_base_url(client)?,
            };
            Ok(format!("{base}/{}:generateContent", client.model))
        }
        _ => unreachable!("resolve_url called with non-Google provider"),
    }
}

/// Placeholder used when `project_id` is not yet known at URL construction time.
/// Resolved during auth when credentials are available.
pub(crate) const VERTEX_PROJECT_ID_PLACEHOLDER: &str = "__BAML_VERTEX_PROJECT_ID__";

/// Extract Vertex AI URL components: `(domain, location, project_id)`.
///
/// `location` is required (the old engine errors with "must specify a GCP region").
/// `project_id` may use a placeholder that gets resolved during auth.
fn vertex_url_components(
    client: &crate::baml_std::PrimitiveClient,
) -> Result<(String, String, String), super::BuildRequestError> {
    let vertex_opts = match &client.provider_options {
        Some(crate::baml_std::ProviderOptions::VertexAi(opts)) => Some(opts),
        _ => None,
    };

    let location = vertex_opts
        .and_then(|o| o.location.as_deref())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| super::BuildRequestError::InvalidOption {
            key: "location".to_string(),
            reason: "vertex-ai requires either base_url or location (e.g. us-central1)".to_string(),
        })?;

    let project_id = vertex_opts
        .and_then(|o| o.project_id.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or(VERTEX_PROJECT_ID_PLACEHOLDER);

    let domain = if location == "global" {
        "aiplatform.googleapis.com".to_string()
    } else {
        format!("{location}-aiplatform.googleapis.com")
    };

    Ok((domain, location.to_string(), project_id.to_string()))
}

/// Build the Vertex AI base URL from `location` and `project_id` in provider options.
fn resolve_vertex_base_url(
    client: &crate::baml_std::PrimitiveClient,
) -> Result<String, super::BuildRequestError> {
    let (domain, location, project_id) = vertex_url_components(client)?;
    Ok(format!(
        "https://{domain}/v1/projects/{project_id}/locations/{location}/publishers/google/models"
    ))
}

/// Build the Vertex AI URL for Anthropic `rawPredict` requests.
///
/// Anthropic models on Vertex AI use a different publisher path:
/// `publishers/anthropic/models` instead of `publishers/google/models`.
pub(super) fn resolve_vertex_raw_predict_url(
    client: &crate::baml_std::PrimitiveClient,
) -> Result<String, super::BuildRequestError> {
    // If user provides a base_url, use it as-is (they're responsible for the path)
    if let Some(url) = client.options.base_url.as_deref() {
        return Ok(format!("{url}/{}:rawPredict", client.model));
    }

    let (domain, location, project_id) = vertex_url_components(client)?;
    Ok(format!(
        "https://{domain}/v1/projects/{project_id}/locations/{location}/publishers/anthropic/models/{}:rawPredict",
        client.model
    ))
}

// ============================================================================
// Prompt conversion
// ============================================================================

/// Split the prompt into an optional system instruction and message contents.
///
/// The Gemini API requires system content in a top-level `"systemInstruction"`
/// field, not inside the `"contents"` array.
fn extract_system_and_contents(
    prompt: &bex_vm_types::PromptAst,
) -> Result<(Option<Content>, Vec<Content>), super::BuildRequestError> {
    let items = match prompt.as_ref() {
        PromptAst::Vec(v) => v.clone(),
        _ => vec![prompt.clone()],
    };

    let mut system_parts = Vec::new();
    let mut contents = Vec::new();

    for item in &items {
        match item.as_ref() {
            PromptAst::Message { role, content, .. } if role == "system" => {
                system_parts.extend(gemini_parts(content.as_ref())?);
            }
            PromptAst::Message { role, content, .. } => {
                let parts = gemini_parts(content.as_ref())?;
                contents.push(Content {
                    role: Some(role.clone()),
                    parts,
                });
            }
            _ => {} // Skip non-message nodes
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

fn gemini_parts(content: &PromptAstSimple) -> Result<Vec<Part>, super::BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => Ok(vec![Part {
            text: Some(s.clone()),
            inline_data: None,
            file_data: None,
        }]),
        PromptAstSimple::Media(media) => gemini_media_part(media).map(|p| vec![p]),
        PromptAstSimple::Multiple(items) => {
            let mut parts = Vec::new();
            for item in items {
                parts.extend(gemini_parts(item)?);
            }
            Ok(parts)
        }
    }
}

fn gemini_media_part(media: &Arc<MediaValue>) -> Result<Part, super::BuildRequestError> {
    let mime = super::mime_type_as_ok(media)?;

    media.read_content(|c| {
        // Prefer inline base64 data when available (covers Base64, Url+prefetched, File+resolved).
        if let Some(b64) = c.base64_data() {
            return Ok(Part {
                text: None,
                inline_data: Some(InlineData {
                    mime_type: mime.clone(),
                    data: b64.to_string(),
                }),
                file_data: None,
            });
        }

        match c {
            MediaContent::Url { url, .. } => Ok(Part {
                text: None,
                inline_data: None,
                file_data: Some(FileData {
                    mime_type: mime.clone(),
                    file_uri: url.clone(),
                }),
            }),
            MediaContent::File { file, .. } => {
                Err(super::BuildRequestError::FileNotResolved(file.clone()))
            }
            MediaContent::Base64 { .. } => unreachable!("base64_data() returned None for Base64"),
        }
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_base::MediaKind;
    use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
    use bex_external_types::BexExternalValue;
    use indexmap::IndexMap;

    use super::*;

    fn make_media(kind: MediaKind, content: MediaContent, mime: Option<&str>) -> Arc<MediaValue> {
        Arc::new(MediaValue::new(kind, content, mime.map(String::from)))
    }

    fn msg(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(PromptAstSimple::String(text.to_string())),
            metadata: serde_json::Value::Null,
        })
    }

    fn msg_with_content(role: &str, content: PromptAstSimple) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(content),
            metadata: serde_json::Value::Null,
        })
    }

    fn make_client(
        provider: &str,
        options: crate::baml_std::PrimitiveClientOptions,
    ) -> crate::baml_std::PrimitiveClient {
        crate::baml_std::PrimitiveClient::new("test".to_string(), provider.to_string(), options)
            .unwrap()
    }

    fn parse_body(req: &crate::baml_std::HttpRequest) -> serde_json::Value {
        serde_json::from_str(&req.body).unwrap()
    }

    // ========================================================================
    // URL tests
    // ========================================================================

    #[test]
    fn google_ai_url_construction() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        assert_eq!(
            result.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent"
        );
    }

    #[test]
    fn vertex_ai_url_construction() {
        let client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some(
                    "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models"
                        .to_string(),
                ),
                ..Default::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, &prompt, LlmProvider::VertexAi).unwrap();
        assert_eq!(
            result.url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models/gemini-2.0-flash:generateContent"
        );
    }

    #[test]
    fn vertex_ai_url_from_location_and_project_id() {
        let mut client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                ..Default::default()
            },
        );
        client.provider_options = Some(crate::baml_std::ProviderOptions::VertexAi(
            crate::baml_std::VertexAiOptions {
                location: Some("europe-west4".to_string()),
                project_id: Some("my-project".to_string()),
                credentials: None,
                credentials_content: None,
            },
        ));
        let prompt = msg("user", "hello");
        let result = build_request(&client, &prompt, LlmProvider::VertexAi).unwrap();
        assert_eq!(
            result.url,
            "https://europe-west4-aiplatform.googleapis.com/v1/projects/my-project/locations/europe-west4/publishers/google/models/gemini-2.0-flash:generateContent"
        );
    }

    #[test]
    fn vertex_anthropic_url_uses_anthropic_publisher() {
        // When no base_url is provided, the URL should use publishers/anthropic, not publishers/google
        let mut client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("claude-sonnet-4-20250514".to_string()),
                // No base_url - URL is built from location + project_id
                ..Default::default()
            },
        );
        client.provider_options = Some(crate::baml_std::ProviderOptions::VertexAi(
            crate::baml_std::VertexAiOptions {
                location: Some("us-central1".to_string()),
                project_id: Some("my-project".to_string()),
                credentials: None,
                credentials_content: None,
            },
        ));

        // Test the URL building directly (no auth required)
        let url = resolve_vertex_raw_predict_url(&client).unwrap();

        assert!(
            url.contains("publishers/anthropic/models"),
            "Claude on Vertex should use publishers/anthropic, got: {url}"
        );
        assert!(
            !url.contains("publishers/google/models"),
            "Claude on Vertex should NOT use publishers/google, got: {url}"
        );
        assert_eq!(
            url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/anthropic/models/claude-sonnet-4-20250514:rawPredict"
        );
    }

    #[test]
    fn vertex_anthropic_url_respects_explicit_base_url() {
        let client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("claude-sonnet-4-20250514".to_string()),
                base_url: Some(
                    "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/anthropic/models"
                        .to_string(),
                ),
                ..Default::default()
            },
        );

        let url = resolve_vertex_raw_predict_url(&client).unwrap();

        assert_eq!(
            url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/anthropic/models/claude-sonnet-4-20250514:rawPredict"
        );
    }

    // ========================================================================
    // Basic message tests
    // ========================================================================

    #[test]
    fn google_ai_single_user_message() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let prompt = msg("user", "Hello");
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [{"text": "Hello"}]
                    }
                ]
            })
        );
    }

    #[test]
    fn google_ai_system_extracted_to_system_instruction() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
        ]));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "Hi"}]}
                ],
                "systemInstruction": {
                    "parts": [{"text": "You are helpful."}]
                }
            })
        );
    }

    /// Roles are remapped at compile time via `remap_roles` in `lower_cst.rs`,
    /// so the builder receives `"model"` directly (not `"assistant"`).
    #[test]
    fn google_ai_model_role_passed_through() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("user", "Hi"),
            msg("model", "Hello!"),
            msg("user", "How are you?"),
        ]));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "Hi"}]},
                    {"role": "model", "parts": [{"text": "Hello!"}]},
                    {"role": "user", "parts": [{"text": "How are you?"}]}
                ]
            })
        );
    }

    #[test]
    fn google_ai_multi_turn_with_system() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
            msg("model", "Hello!"),
            msg("user", "How are you?"),
            msg("model", "I'm well."),
        ]));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "Hi"}]},
                    {"role": "model", "parts": [{"text": "Hello!"}]},
                    {"role": "user", "parts": [{"text": "How are you?"}]},
                    {"role": "model", "parts": [{"text": "I'm well."}]}
                ],
                "systemInstruction": {
                    "parts": [{"text": "You are helpful."}]
                }
            })
        );
    }

    #[test]
    fn google_ai_multiple_system_messages_combined() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("system", "Be concise."),
            msg("user", "Hi"),
        ]));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "Hi"}]}
                ],
                "systemInstruction": {
                    "parts": [
                        {"text": "You are helpful."},
                        {"text": "Be concise."}
                    ]
                }
            })
        );
    }

    #[test]
    fn google_ai_no_system_message() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let prompt = msg("user", "Hello");
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        // systemInstruction should be absent, not null
        assert!(body.get("systemInstruction").is_none());
    }

    // ========================================================================
    // Extra body / request_body forwarding
    // ========================================================================

    #[test]
    fn google_ai_forwards_request_body_options() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                request_body: IndexMap::from([(
                    "generationConfig".to_string(),
                    BexExternalValue::Map {
                        key_type: baml_type::Ty::string(),
                        value_type: baml_type::Ty::unknown(),
                        entries: IndexMap::from([
                            ("temperature".to_string(), BexExternalValue::Float(0.7)),
                            ("maxOutputTokens".to_string(), BexExternalValue::Int(1024)),
                        ]),
                    },
                )]),
                ..Default::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "hello"}]}
                ],
                "generationConfig": {
                    "temperature": 0.7,
                    "maxOutputTokens": 1024
                }
            })
        );
    }

    // ========================================================================
    // Media tests
    // ========================================================================

    #[test]
    fn google_ai_image_base64() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let media = make_media(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "abc123".into(),
            },
            Some("image/png"),
        );
        let prompt = msg_with_content(
            "user",
            PromptAstSimple::Multiple(vec![
                Arc::new(PromptAstSimple::String("Look at this:".into())),
                Arc::new(PromptAstSimple::Media(media)),
            ]),
        );
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            {"text": "Look at this:"},
                            {"inlineData": {"mimeType": "image/png", "data": "abc123"}}
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn google_ai_image_url() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/cat.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let prompt = msg_with_content("user", PromptAstSimple::Media(media));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            {"fileData": {"mimeType": "image/png", "fileUri": "https://example.com/cat.png"}}
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn google_ai_image_url_with_prefetched_base64() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/cat.png".into(),
                base64_data: Some("prefetched_data".into()),
            },
            Some("image/png"),
        );
        let prompt = msg_with_content("user", PromptAstSimple::Media(media));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            {"inlineData": {"mimeType": "image/png", "data": "prefetched_data"}}
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn google_ai_image_file_with_base64() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "test.png".into(),
                base64_data: Some("resolved_data".into()),
            },
            Some("image/png"),
        );
        let prompt = msg_with_content("user", PromptAstSimple::Media(media));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            {"inlineData": {"mimeType": "image/png", "data": "resolved_data"}}
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn google_ai_image_file_not_resolved_error() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "test.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let result = gemini_media_part(&media);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test.png"));
    }

    #[test]
    fn google_ai_audio_base64() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Base64 {
                base64_data: "audiodata".into(),
            },
            Some("audio/wav"),
        );
        let prompt = msg_with_content("user", PromptAstSimple::Media(media));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            {"inlineData": {"mimeType": "audio/wav", "data": "audiodata"}}
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn google_ai_audio_url() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Url {
                url: "https://example.com/audio.wav".into(),
                base64_data: None,
            },
            Some("audio/wav"),
        );
        let prompt = msg_with_content("user", PromptAstSimple::Media(media));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            {"fileData": {"mimeType": "audio/wav", "fileUri": "https://example.com/audio.wav"}}
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn google_ai_pdf_base64() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Base64 {
                base64_data: "pdfdata".into(),
            },
            Some("application/pdf"),
        );
        let prompt = msg_with_content("user", PromptAstSimple::Media(media));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            {"inlineData": {"mimeType": "application/pdf", "data": "pdfdata"}}
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn google_ai_pdf_url() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Url {
                url: "gs://my-bucket/doc.pdf".into(),
                base64_data: None,
            },
            Some("application/pdf"),
        );
        let prompt = msg_with_content("user", PromptAstSimple::Media(media));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            {"fileData": {"mimeType": "application/pdf", "fileUri": "gs://my-bucket/doc.pdf"}}
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn google_ai_video_url() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let media = make_media(
            MediaKind::Video,
            MediaContent::Url {
                url: "gs://my-bucket/video.mp4".into(),
                base64_data: None,
            },
            Some("video/mp4"),
        );
        let prompt = msg_with_content("user", PromptAstSimple::Media(media));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            {"fileData": {"mimeType": "video/mp4", "fileUri": "gs://my-bucket/video.mp4"}}
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn google_ai_video_base64() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let media = make_media(
            MediaKind::Video,
            MediaContent::Base64 {
                base64_data: "videodata".into(),
            },
            Some("video/mp4"),
        );
        let prompt = msg_with_content("user", PromptAstSimple::Media(media));
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [
                            {"inlineData": {"mimeType": "video/mp4", "data": "videodata"}}
                        ]
                    }
                ]
            })
        );
    }

    // ========================================================================
    // Envelope tests
    // ========================================================================

    #[test]
    fn google_ai_default_headers() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        assert_eq!(
            result.headers.get("content-type").unwrap(),
            "application/json"
        );
    }

    #[test]
    fn google_ai_method_is_post() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, &prompt, LlmProvider::GoogleAi).unwrap();
        assert_eq!(result.method, "POST");
    }
}
