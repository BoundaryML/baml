//! Provider-specific request transformations.
//!
//! This module contains transformations that convert a `PromptAst` into an
//! `HttpRequest` for specific LLM providers.

pub mod anthropic;
pub mod openai;

use bex_llm_types::{HttpRequest, PromptAst, ResolvedClient};

/// Errors that can occur during provider request building.
#[derive(Debug, Clone)]
pub enum ProviderError {
    /// The provider is not supported.
    UnsupportedProvider(String),
    /// A required option is missing.
    MissingOption(String),
    /// An invalid option value was provided.
    InvalidOption { name: String, message: String },
    /// The prompt structure is invalid for this provider.
    InvalidPrompt(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::UnsupportedProvider(p) => write!(f, "unsupported provider: {}", p),
            ProviderError::MissingOption(opt) => write!(f, "missing required option: {}", opt),
            ProviderError::InvalidOption { name, message } => {
                write!(f, "invalid option '{}': {}", name, message)
            }
            ProviderError::InvalidPrompt(msg) => write!(f, "invalid prompt: {}", msg),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Build an HTTP request for the given provider.
///
/// This is the main entry point for converting a `PromptAst` into an `HttpRequest`.
/// It dispatches to the appropriate provider-specific builder based on
/// `client.provider`.
pub fn build_request(
    prompt: &PromptAst,
    client: &ResolvedClient,
) -> Result<HttpRequest, ProviderError> {
    match client.provider.as_str() {
        "openai" | "openai-generic" => openai::build_request(prompt, client),
        "anthropic" => anthropic::build_request(prompt, client),
        other => Err(ProviderError::UnsupportedProvider(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bex_llm_types::{ModelFeatures, PromptAstNode, RoleConfig};
    use indexmap::IndexMap;

    fn make_client(provider: &str, model: &str) -> ResolvedClient {
        let mut options = IndexMap::new();
        options.insert("model".to_string(), serde_json::json!(model));
        options.insert("api_key".to_string(), serde_json::json!("test-key"));

        ResolvedClient {
            name: "test-client".to_string(),
            provider: provider.to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options,
            request_config: Default::default(),
        }
    }

    #[test]
    fn test_dispatch_to_openai() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));
        let client = make_client("openai", "gpt-4");

        let request = build_request(&prompt, &client).unwrap();
        assert!(request.url.contains("api.openai.com"));
        assert!(request.url.contains("chat/completions"));
    }

    #[test]
    fn test_dispatch_to_openai_generic() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));
        let client = make_client("openai-generic", "custom-model");

        let request = build_request(&prompt, &client).unwrap();
        // openai-generic uses the same endpoint format as openai
        assert!(request.url.contains("api.openai.com"));
        assert!(request.url.contains("chat/completions"));
    }

    #[test]
    fn test_dispatch_to_anthropic() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));
        let client = make_client("anthropic", "claude-3-opus-20240229");

        let request = build_request(&prompt, &client).unwrap();
        assert!(request.url.contains("api.anthropic.com"));
        assert!(request.url.contains("messages"));
    }

    #[test]
    fn test_unsupported_provider() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));
        let client = make_client("unknown-provider", "some-model");

        let result = build_request(&prompt, &client);
        assert!(matches!(result, Err(ProviderError::UnsupportedProvider(_))));

        if let Err(ProviderError::UnsupportedProvider(provider)) = result {
            assert_eq!(provider, "unknown-provider");
        }
    }

    #[test]
    fn test_provider_error_display() {
        let err = ProviderError::UnsupportedProvider("test-provider".to_string());
        assert_eq!(format!("{}", err), "unsupported provider: test-provider");

        let err = ProviderError::MissingOption("model".to_string());
        assert_eq!(format!("{}", err), "missing required option: model");

        let err = ProviderError::InvalidOption {
            name: "temperature".to_string(),
            message: "must be between 0 and 1".to_string(),
        };
        assert_eq!(
            format!("{}", err),
            "invalid option 'temperature': must be between 0 and 1"
        );

        let err = ProviderError::InvalidPrompt("nested messages not allowed".to_string());
        assert_eq!(
            format!("{}", err),
            "invalid prompt: nested messages not allowed"
        );
    }

    #[test]
    fn test_openai_and_anthropic_have_different_auth() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        // OpenAI uses Bearer auth
        let openai_client = make_client("openai", "gpt-4");
        let openai_request = build_request(&prompt, &openai_client).unwrap();
        assert!(openai_request.headers.get("Authorization").is_some());
        assert!(openai_request
            .headers
            .get("Authorization")
            .unwrap()
            .starts_with("Bearer"));

        // Anthropic uses x-api-key header
        let anthropic_client = make_client("anthropic", "claude-3-opus-20240229");
        let anthropic_request = build_request(&prompt, &anthropic_client).unwrap();
        assert!(anthropic_request.headers.get("x-api-key").is_some());
        assert!(anthropic_request.headers.get("Authorization").is_none());
    }

    #[test]
    fn test_anthropic_has_version_header() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));
        let client = make_client("anthropic", "claude-3-opus-20240229");

        let request = build_request(&prompt, &client).unwrap();
        assert!(request.headers.get("anthropic-version").is_some());
    }

    #[test]
    fn test_anthropic_has_max_tokens_default() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));
        let client = make_client("anthropic", "claude-3-opus-20240229");

        let request = build_request(&prompt, &client).unwrap();
        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                assert!(body.get("max_tokens").is_some());
            }
            _ => panic!("expected JSON body"),
        }
    }
}
