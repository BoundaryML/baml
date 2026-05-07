//! Provider-specific authentication for LLM HTTP requests.
//!
//! Adds auth headers (API keys, bearer tokens, `SigV4` signatures, etc.) to a
//! fully-built HTTP request.
//!
//! Today this is called as a final step inside `build_request::build_request`.
//! Eventually it can be promoted to a standalone operation in the BAML LLM
//! function pipeline (llm.baml) so that auth can be resolved, cached, or
//! refreshed independently of request building (e.g. short-lived STS tokens,
//! OAuth token refresh, or vault-based secret retrieval).

pub(crate) mod bedrock;
pub(crate) mod vertex;

use std::sync::Arc;

use crate::{
    LlmProvider,
    baml_std::{HttpRequest, PrimitiveClient},
    build_request::BuildRequestError,
};

/// Central auth dispatch. Mutates `request` in place to add auth headers.
pub(crate) async fn auth_request(
    provider: LlmProvider,
    request: &mut HttpRequest,
    client: &PrimitiveClient,
    io: Arc<dyn ::sys_types::runtime_io::RuntimeIo>,
) -> Result<(), BuildRequestError> {
    match provider {
        LlmProvider::Anthropic => auth_anthropic(request, client),
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter
        | LlmProvider::OpenAiResponses
        | LlmProvider::AiGatewayImages => auth_openai(request, client, provider),
        LlmProvider::AwsBedrock => {
            return bedrock::auth_bedrock(request, client, io).await;
        }
        LlmProvider::VertexAi => {
            return vertex::auth_vertex(request, client, io).await;
        }
        LlmProvider::GoogleAi => auth_google_ai(request, client),
        LlmProvider::BamlFallback | LlmProvider::BamlRoundRobin => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Google AI (Gemini)
// ---------------------------------------------------------------------------

fn auth_google_ai(request: &mut HttpRequest, client: &PrimitiveClient) {
    if let Some(api_key) = &client.options.api_key {
        request
            .headers
            .insert("x-goog-api-key".to_string(), api_key.clone());
    }
}

// ---------------------------------------------------------------------------
// Anthropic
// ---------------------------------------------------------------------------

fn auth_anthropic(request: &mut HttpRequest, client: &PrimitiveClient) {
    if let Some(api_key) = &client.options.api_key {
        request
            .headers
            .insert("x-api-key".to_string(), api_key.clone());
    }
}

// ---------------------------------------------------------------------------
// OpenAI (Chat Completions + Responses, including Azure)
// ---------------------------------------------------------------------------

fn auth_openai(request: &mut HttpRequest, client: &PrimitiveClient, provider: LlmProvider) {
    if let Some(api_key) = &client.options.api_key {
        if provider == LlmProvider::AzureOpenAi {
            request
                .headers
                .insert("api-key".to_string(), api_key.clone());
        } else {
            request
                .headers
                .insert("authorization".to_string(), format!("Bearer {api_key}"));
            if provider == LlmProvider::AiGatewayImages {
                request
                    .headers
                    .insert("ai-gateway-auth-method".to_string(), "api-key".to_string());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use bex_external_types::AsBexExternalValue;

    use super::*;
    use crate::baml_std::PrimitiveClientOptions;

    fn make_client(provider: &str, mut options: PrimitiveClientOptions) -> PrimitiveClient {
        if options.model.is_none() {
            options.model = Some("test-model".to_string());
        }
        PrimitiveClient::new("test".to_string(), provider.to_string(), options).unwrap()
    }

    fn fake_request() -> HttpRequest {
        HttpRequest {
            method: "POST".to_string(),
            url: "https://example.com/v1/chat/completions".to_string(),
            headers: indexmap::IndexMap::new(),
            body: r#"{"messages":[]}"#.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // Anthropic
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn anthropic_sets_api_key_header() {
        let client = make_client(
            "anthropic",
            PrimitiveClientOptions {
                api_key: Some("sk-ant-test".to_string()),
                ..Default::default()
            },
        );
        let mut req = fake_request();
        auth_request(
            LlmProvider::Anthropic,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(req.headers.get("x-api-key").unwrap(), "sk-ant-test");
    }

    #[tokio::test]
    async fn anthropic_no_api_key_omits_header() {
        let client = make_client("anthropic", PrimitiveClientOptions::default());
        let mut req = fake_request();
        auth_request(
            LlmProvider::Anthropic,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert!(!req.headers.contains_key("x-api-key"));
    }

    // -----------------------------------------------------------------------
    // OpenAI
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn openai_sets_bearer_token() {
        let client = make_client(
            "openai",
            PrimitiveClientOptions {
                api_key: Some("sk-test-key".to_string()),
                ..Default::default()
            },
        );
        let mut req = fake_request();
        auth_request(
            LlmProvider::OpenAi,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(
            req.headers.get("authorization").unwrap(),
            "Bearer sk-test-key",
        );
    }

    #[tokio::test]
    async fn openai_no_api_key_omits_header() {
        let client = make_client("openai", PrimitiveClientOptions::default());
        let mut req = fake_request();
        auth_request(
            LlmProvider::OpenAi,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert!(!req.headers.contains_key("authorization"));
    }

    #[tokio::test]
    async fn ai_gateway_images_sets_gateway_auth_method_header() {
        let client = make_client(
            "ai-gateway-images",
            PrimitiveClientOptions {
                api_key: Some("gateway-test-key".to_string()),
                ..Default::default()
            },
        );
        let mut req = fake_request();
        auth_request(
            LlmProvider::AiGatewayImages,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(
            req.headers.get("authorization").unwrap(),
            "Bearer gateway-test-key",
        );
        assert_eq!(
            req.headers.get("ai-gateway-auth-method").unwrap(),
            "api-key",
        );
    }

    #[tokio::test]
    async fn azure_uses_api_key_header() {
        let client = make_client(
            "azure-openai",
            PrimitiveClientOptions {
                api_key: Some("az-key".to_string()),
                provider_options: crate::baml_std::AzureOpenAiOptions {
                    resource_name: Some("my-resource".to_string()),
                    deployment_id: Some("gpt-4o".to_string()),
                    api_version: "2024-02-15-preview".to_string(),
                    max_tokens: Some(4096),
                }
                .into_bex_external_value(),
                ..Default::default()
            },
        );
        let mut req = fake_request();
        auth_request(
            LlmProvider::AzureOpenAi,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(req.headers.get("api-key").unwrap(), "az-key");
        assert!(!req.headers.contains_key("authorization"));
    }

    #[tokio::test]
    async fn openai_generic_uses_bearer() {
        let client = make_client(
            "openai-generic",
            PrimitiveClientOptions {
                api_key: Some("sk-gen".to_string()),
                base_url: Some("https://custom.api.com".to_string()),
                ..Default::default()
            },
        );
        let mut req = fake_request();
        auth_request(
            LlmProvider::OpenAiGeneric,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(req.headers.get("authorization").unwrap(), "Bearer sk-gen",);
    }

    #[tokio::test]
    async fn openrouter_uses_bearer() {
        let client = make_client(
            "openrouter",
            PrimitiveClientOptions {
                api_key: Some("sk-or".to_string()),
                ..Default::default()
            },
        );
        let mut req = fake_request();
        auth_request(
            LlmProvider::OpenRouter,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(req.headers.get("authorization").unwrap(), "Bearer sk-or");
    }

    #[tokio::test]
    async fn ollama_uses_bearer() {
        let client = make_client(
            "ollama",
            PrimitiveClientOptions {
                api_key: Some("ollama-key".to_string()),
                ..Default::default()
            },
        );
        let mut req = fake_request();
        auth_request(
            LlmProvider::Ollama,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(
            req.headers.get("authorization").unwrap(),
            "Bearer ollama-key",
        );
    }

    // -----------------------------------------------------------------------
    // Google AI
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn google_ai_sets_x_goog_api_key_header() {
        let client = make_client(
            "google-ai",
            PrimitiveClientOptions {
                api_key: Some("gemini-key".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let mut req = fake_request();
        auth_request(
            LlmProvider::GoogleAi,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(req.headers.get("x-goog-api-key").unwrap(), "gemini-key");
    }

    #[tokio::test]
    async fn google_ai_no_api_key_omits_header() {
        let client = make_client(
            "google-ai",
            PrimitiveClientOptions {
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let mut req = fake_request();
        auth_request(
            LlmProvider::GoogleAi,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert!(!req.headers.contains_key("x-goog-api-key"));
    }

    #[tokio::test]
    async fn google_ai_preserves_existing_headers() {
        let client = make_client(
            "google-ai",
            PrimitiveClientOptions {
                api_key: Some("gemini-key".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..Default::default()
            },
        );
        let mut req = fake_request();
        req.headers
            .insert("content-type".to_string(), "application/json".to_string());
        auth_request(
            LlmProvider::GoogleAi,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(req.headers.get("content-type").unwrap(), "application/json");
        assert_eq!(req.headers.get("x-goog-api-key").unwrap(), "gemini-key");
    }

    // -----------------------------------------------------------------------
    // General
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn authorize_preserves_existing_headers() {
        let client = make_client(
            "openai",
            PrimitiveClientOptions {
                api_key: Some("sk-test".to_string()),
                ..Default::default()
            },
        );
        let mut req = fake_request();
        req.headers
            .insert("content-type".to_string(), "application/json".to_string());
        req.headers
            .insert("x-custom".to_string(), "value".to_string());
        auth_request(
            LlmProvider::OpenAi,
            &mut req,
            &client,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(req.headers.get("content-type").unwrap(), "application/json",);
        assert_eq!(req.headers.get("x-custom").unwrap(), "value");
        assert_eq!(req.headers.get("authorization").unwrap(), "Bearer sk-test",);
    }
}
