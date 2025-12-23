use std::collections::{HashMap, HashSet};

use anyhow::Result;
use baml_types::{GetEnvVar, StringOr};

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum ClientSpec {
    Named(String),
    /// Shorthand for "<provider>/<model>"
    Shorthand(ClientProvider, String),
}

impl ClientSpec {
    pub fn dependencies(&self) -> HashSet<String> {
        match self {
            ClientSpec::Named(name) => HashSet::from([name.clone()]),
            ClientSpec::Shorthand(..) => Default::default(),
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            ClientSpec::Named(n) => n.clone(),
            ClientSpec::Shorthand(provider, model) => format!("{provider}/{model}"),
        }
    }

    pub fn new_from_id(arg: &str) -> Result<Self, anyhow::Error> {
        if arg.contains("/") {
            let (provider, model) = arg.split_once("/").expect("Already checked for '/'");
            Ok(ClientSpec::Shorthand(provider.parse()?, model.to_string()))
        } else {
            Ok(ClientSpec::Named(arg.into()))
        }
    }
}

/// The provider for the client, e.g. baml-openai-chat
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ClientProvider {
    /// The OpenAI client provider variant
    OpenAI(OpenAIClientProviderVariant),
    /// The Anthropic client provider variant
    Anthropic,
    /// The AWS Bedrock client provider variant
    AwsBedrock,
    /// The Google AI client provider variant
    GoogleAi,
    /// The Vertex client provider variant
    Vertex,
    /// The strategy client provider variant
    Strategy(StrategyClientProvider),
}

/// The OpenAI client provider variant
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OpenAIClientProviderVariant {
    /// The base OpenAI client provider variant
    Base,
    /// The Ollama client provider variant
    Ollama,
    /// The Azure client provider variant
    Azure,
    /// The OpenAI Responses API variant
    Responses,
    /// The generic client provider variant
    Generic,
    /// The OpenRouter client provider variant
    OpenRouter,
}

/// The strategy client provider variant
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StrategyClientProvider {
    /// The round-robin strategy client provider variant
    RoundRobin,
    /// The fallback strategy client provider variant
    Fallback,
}

impl std::fmt::Display for ClientProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientProvider::OpenAI(variant) => write!(f, "{variant}"),
            ClientProvider::Anthropic => write!(f, "anthropic"),
            ClientProvider::AwsBedrock => write!(f, "aws-bedrock"),
            ClientProvider::GoogleAi => write!(f, "google-ai"),
            ClientProvider::Vertex => write!(f, "vertex-ai"),
            ClientProvider::Strategy(variant) => write!(f, "{variant}"),
        }
    }
}

impl std::fmt::Display for OpenAIClientProviderVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenAIClientProviderVariant::Base => write!(f, "openai"),
            OpenAIClientProviderVariant::Ollama => write!(f, "ollama"),
            OpenAIClientProviderVariant::Azure => write!(f, "azure-openai"),
            OpenAIClientProviderVariant::Responses => write!(f, "openai-responses"),
            OpenAIClientProviderVariant::Generic => write!(f, "openai-generic"),
            OpenAIClientProviderVariant::OpenRouter => write!(f, "openrouter"),
        }
    }
}

impl std::fmt::Display for StrategyClientProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StrategyClientProvider::RoundRobin => write!(f, "round-robin"),
            StrategyClientProvider::Fallback => write!(f, "fallback"),
        }
    }
}

impl std::str::FromStr for ClientProvider {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "openai" => Ok(ClientProvider::OpenAI(OpenAIClientProviderVariant::Base)),
            "baml-openai-chat" => Ok(ClientProvider::OpenAI(OpenAIClientProviderVariant::Base)),
            "openai-generic" => Ok(ClientProvider::OpenAI(OpenAIClientProviderVariant::Generic)),
            "azure-openai" => Ok(ClientProvider::OpenAI(OpenAIClientProviderVariant::Azure)),
            "baml-azure-chat" => Ok(ClientProvider::OpenAI(OpenAIClientProviderVariant::Azure)),
            "openai-responses" => Ok(ClientProvider::OpenAI(
                OpenAIClientProviderVariant::Responses,
            )),
            "baml-ollama-chat" => Ok(ClientProvider::OpenAI(OpenAIClientProviderVariant::Ollama)),
            "ollama" => Ok(ClientProvider::OpenAI(OpenAIClientProviderVariant::Ollama)),
            "openrouter" => Ok(ClientProvider::OpenAI(
                OpenAIClientProviderVariant::OpenRouter,
            )),
            "anthropic" => Ok(ClientProvider::Anthropic),
            "baml-anthropic-chat" => Ok(ClientProvider::Anthropic),
            "aws-bedrock" => Ok(ClientProvider::AwsBedrock),
            "google-ai" => Ok(ClientProvider::GoogleAi),
            "vertex-ai" => Ok(ClientProvider::Vertex),
            "fallback" => Ok(ClientProvider::Strategy(StrategyClientProvider::Fallback)),
            "baml-fallback" => Ok(ClientProvider::Strategy(StrategyClientProvider::Fallback)),
            "round-robin" => Ok(ClientProvider::Strategy(StrategyClientProvider::RoundRobin)),
            "baml-round-robin" => Ok(ClientProvider::Strategy(StrategyClientProvider::RoundRobin)),
            _ => Err(anyhow::anyhow!("Invalid client provider: {}", s)),
        }
    }
}

impl std::str::FromStr for OpenAIClientProviderVariant {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "openai" => Ok(OpenAIClientProviderVariant::Base),
            "ollama" => Ok(OpenAIClientProviderVariant::Ollama),
            "azure-openai" => Ok(OpenAIClientProviderVariant::Azure),
            "openai-responses" => Ok(OpenAIClientProviderVariant::Responses),
            "openai-generic" => Ok(OpenAIClientProviderVariant::Generic),
            "openrouter" => Ok(OpenAIClientProviderVariant::OpenRouter),
            _ => Err(anyhow::anyhow!(
                "Invalid OpenAI client provider variant: {}",
                s
            )),
        }
    }
}

impl std::str::FromStr for StrategyClientProvider {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "round-robin" => Ok(StrategyClientProvider::RoundRobin),
            "fallback" => Ok(StrategyClientProvider::Fallback),
            _ => Err(anyhow::anyhow!(
                "Invalid strategy client provider variant: {}",
                s
            )),
        }
    }
}

impl ClientProvider {
    pub fn allowed_providers() -> &'static [&'static str] {
        &[
            "openai",
            "openai-generic",
            "azure-openai",
            "openai-responses",
            "anthropic",
            "ollama",
            "openrouter",
            "round-robin",
            "fallback",
            "google-ai",
            "vertex-ai",
            "aws-bedrock",
        ]
    }
}

impl std::fmt::Display for ClientSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientSpec::Named(n) => write!(f, "{n}"),
            ClientSpec::Shorthand(provider, model) => write!(f, "{provider}/{model}"),
        }
    }
}

#[derive(Clone, Debug, Hash, Default)]
pub struct SupportedRequestModes {
    // If unset, treat as auto
    pub stream: Option<bool>,
}

impl SupportedRequestModes {
    pub fn required_env_vars(&self) -> HashSet<String> {
        HashSet::new()
    }
}

#[derive(Clone, Debug, Hash)]
pub enum UnresolvedFinishReasonFilter {
    All,
    AllowList(Vec<StringOr>),
    DenyList(Vec<StringOr>),
}

#[derive(Clone, Debug, Hash)]
pub enum FinishReasonFilter {
    All,
    AllowList(Vec<String>),
    DenyList(Vec<String>),
}

impl UnresolvedFinishReasonFilter {
    pub fn required_env_vars(&self) -> HashSet<String> {
        match self {
            Self::AllowList(allow) => allow.iter().flat_map(StringOr::required_env_vars).collect(),
            Self::DenyList(deny) => deny.iter().flat_map(StringOr::required_env_vars).collect(),
            _ => HashSet::new(),
        }
    }

    pub fn resolve(&self, ctx: &impl GetEnvVar) -> Result<FinishReasonFilter> {
        match self {
            Self::AllowList(allow) => Ok(FinishReasonFilter::AllowList(
                allow
                    .iter()
                    .map(|s| s.resolve(ctx))
                    .collect::<Result<Vec<_>>>()?,
            )),
            Self::DenyList(deny) => Ok(FinishReasonFilter::DenyList(
                deny.iter()
                    .map(|s| s.resolve(ctx))
                    .collect::<Result<Vec<_>>>()?,
            )),
            Self::All => Ok(FinishReasonFilter::All),
        }
    }
}

impl FinishReasonFilter {
    pub fn is_allowed(&self, reason: Option<impl AsRef<str>>) -> bool {
        match self {
            Self::AllowList(allow) => {
                let Some(reason) = reason.map(|r| r.as_ref().to_string()) else {
                    // if no reason is provided, allow all
                    return true;
                };
                // check case insensitive
                allow.iter().any(|r| r.eq_ignore_ascii_case(&reason))
            }
            Self::DenyList(deny) => {
                let Some(reason) = reason.map(|r| r.as_ref().to_string()) else {
                    // if no reason is provided, allow all
                    return true;
                };
                !deny.iter().any(|r| r.eq_ignore_ascii_case(&reason))
            }
            Self::All => true,
        }
    }
}

#[derive(Clone, Debug, Hash)]
pub(crate) struct UnresolvedRolesSelection {
    pub allowed: Option<Vec<StringOr>>,
    pub default: Option<StringOr>,
    pub remap: Option<Vec<(String, StringOr)>>,
}

impl UnresolvedRolesSelection {
    pub fn new(
        allowed: Option<Vec<StringOr>>,
        default: Option<StringOr>,
        remap: Option<Vec<(String, StringOr)>>,
    ) -> Self {
        Self {
            allowed,
            default,
            remap,
        }
    }

    pub fn required_env_vars(&self) -> HashSet<String> {
        let mut env_vars = HashSet::new();
        if let Some(allowed) = &self.allowed {
            env_vars.extend(allowed.iter().flat_map(StringOr::required_env_vars));
        }
        if let Some(default) = &self.default {
            env_vars.extend(default.required_env_vars());
        }
        env_vars
    }

    pub fn resolve(&self, ctx: &impl GetEnvVar) -> Result<RolesSelection> {
        let allowed = self
            .allowed
            .as_ref()
            .map(|allowed| {
                allowed
                    .iter()
                    .map(|s| s.resolve(ctx))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?;

        let default = self
            .default
            .as_ref()
            .map(|default| default.resolve(ctx))
            .transpose()?;

        let remap = self
            .remap
            .as_ref()
            .map(|remap| {
                remap
                    .iter()
                    .map(|(k, v)| Ok((k.to_string(), v.resolve(ctx)?)))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?;

        let remap: Option<HashMap<String, String>> = remap.map(|remap| {
            remap
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        });

        match (&allowed, &default) {
            (Some(allowed), Some(default)) => {
                if !allowed.contains(default) {
                    return Err(anyhow::anyhow!("default_role must be in allowed_roles: {}. Not found in {:?}", default, allowed));
                }
            }
            (None, Some(default)) => {
                match default.as_str() {
                    "system" | "user" | "assistant" => {}
                    _ => return Err(anyhow::anyhow!("default_role must be one of 'system', 'user' or 'assistant': {}. Please specify \"allowed_roles\" if you want to use other custom default role.", default)),
                }
            }
            _ => {}
        }

        match (&allowed, &remap) {
            (Some(allowed), Some(remap)) => {
                for (k, _) in remap.iter() {
                    if !allowed.contains(k) {
                        return Err(anyhow::anyhow!(
                            "remap_role must be in allowed_roles: {}. Not found in {:?}",
                            k,
                            allowed
                        ));
                    }
                }
            }
            (None, Some(remap)) => {
                let allowed = ["system", "user", "assistant"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>();
                for (k, _) in remap.iter() {
                    if !allowed.contains(k) {
                        return Err(anyhow::anyhow!(
                            "remap_role must be in allowed_roles: {}. Not found in {:?}",
                            k,
                            allowed
                        ));
                    }
                }
            }
            _ => {}
        }
        Ok(RolesSelection {
            allowed,
            default,
            remap,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct RolesSelection {
    allowed: Option<Vec<String>>,
    default: Option<String>,
    // key must be in allowed_roles.
    // target is what the HTTP request will use.
    remap: Option<HashMap<String, String>>,
}

impl RolesSelection {
    pub fn allowed_or_else(&self, f: impl FnOnce() -> Vec<String>) -> Vec<String> {
        match self.allowed.as_ref() {
            Some(allowed) => allowed.clone(),
            None => f(),
        }
    }

    pub fn remap(&self) -> Option<HashMap<String, String>> {
        self.remap.clone()
    }

    pub fn default_or_else(&self, f: impl FnOnce() -> String) -> String {
        match self.default.as_ref() {
            Some(default) => default.clone(),
            None => f(),
        }
    }
}

#[derive(Clone, Debug, Hash)]
pub enum UnresolvedAllowedRoleMetadata {
    Value(StringOr),
    All,
    None,
    Only(Vec<StringOr>),
}

#[derive(Clone, Debug, Hash)]
pub enum AllowedRoleMetadata {
    All,
    None,
    Only(Vec<String>),
}

impl UnresolvedAllowedRoleMetadata {
    pub fn required_env_vars(&self) -> HashSet<String> {
        match self {
            Self::Value(role) => role.required_env_vars(),
            Self::Only(roles) => roles
                .iter()
                .flat_map(|role| role.required_env_vars())
                .collect(),
            _ => HashSet::new(),
        }
    }

    pub fn resolve(&self, ctx: &impl GetEnvVar) -> Result<AllowedRoleMetadata> {
        match self {
            Self::Value(role) => {
                let role = role.resolve(ctx)?;
                match role.as_str() {
                    "all" => Ok(AllowedRoleMetadata::All),
                    "none" => Ok(AllowedRoleMetadata::None),
                    _ => Err(anyhow::anyhow!("Invalid allowed role metadata: {}. Allowed values are 'all' or 'none' or an array of roles.", role)),
                }
            }
            Self::All => Ok(AllowedRoleMetadata::All),
            Self::None => Ok(AllowedRoleMetadata::None),
            Self::Only(roles) => Ok(AllowedRoleMetadata::Only(
                roles
                    .iter()
                    .map(|role| role.resolve(ctx))
                    .collect::<Result<Vec<_>>>()?,
            )),
        }
    }
}

impl AllowedRoleMetadata {
    pub fn is_allowed(&self, key: &str) -> bool {
        match self {
            Self::All => true,
            Self::None => false,
            Self::Only(allowed) => allowed.contains(&key.to_string()),
        }
    }
}

#[derive(Clone, Debug, Hash)]
pub enum UnresolvedResponseType {
    OpenAI,
    OpenAIResponses,
    Anthropic,
    Google,
    Vertex,
}

#[derive(Clone, Debug, Hash)]
pub enum ResponseType {
    OpenAI,
    OpenAIResponses,
    Anthropic,
    Google,
    Vertex,
}

impl UnresolvedResponseType {
    pub fn required_env_vars(&self) -> HashSet<String> {
        HashSet::new()
    }

    pub fn resolve(&self, _: &impl GetEnvVar) -> Result<ResponseType> {
        match self {
            Self::OpenAI => Ok(ResponseType::OpenAI),
            Self::OpenAIResponses => Ok(ResponseType::OpenAIResponses),
            Self::Anthropic => Ok(ResponseType::Anthropic),
            Self::Google => Ok(ResponseType::Google),
            Self::Vertex => Ok(ResponseType::Vertex),
        }
    }
}

// Duplicate of the ResolveMediaUrls enum from baml-runtime
// This will be properly resolved when the runtime imports from llm-client
#[derive(Clone, Copy, Debug, Hash, PartialEq)]
pub enum ResolveMediaUrls {
    SendBase64,
    SendBase64UnlessGoogleUrl,
    SendUrlAddMimeType,
    SendUrl,
}

/// Controls how media URLs are processed before sending to LLM providers
///
/// # Variants
///
/// * `SendBase64` - Always download URLs and convert to base64
/// * `SendUrl` - Pass URLs through unchanged
/// * `SendUrlAddMimeType` - Ensure MIME type is present (may require download)
/// * `SendBase64UnlessGoogleUrl` - Only process non-gs:// URLs
#[derive(Clone, Debug, Hash)]
pub enum UnresolvedResolveMediaUrls {
    SendBase64,
    SendUrl,
    SendUrlAddMimeType,
    SendBase64UnlessGoogleUrl,
}

impl UnresolvedResolveMediaUrls {
    pub fn required_env_vars(&self) -> HashSet<String> {
        HashSet::new()
    }

    pub fn resolve(&self, _: &impl GetEnvVar) -> Result<ResolveMediaUrls> {
        Ok(match self {
            Self::SendBase64 => ResolveMediaUrls::SendBase64,
            Self::SendUrl => ResolveMediaUrls::SendUrl,
            Self::SendUrlAddMimeType => ResolveMediaUrls::SendUrlAddMimeType,
            Self::SendBase64UnlessGoogleUrl => ResolveMediaUrls::SendBase64UnlessGoogleUrl,
        })
    }
}

/// Configuration for media URL handling behavior
///
/// # Example
///
/// ```baml
/// client<llm> MyClient {
///   provider openai
///   options {
///     media_url_handler {
///       image "send_base64"           // Convert image URLs to base64
///       audio "send_url"              // Pass audio URLs through
///       pdf "send_url_add_mime_type"  // Add MIME type if missing
///       video "send_url"              // Pass video URLs through
///     }
///   }
/// }
/// ```
#[derive(Clone, Debug, Default, Hash)]
pub struct UnresolvedMediaUrlHandler {
    pub images: Option<UnresolvedResolveMediaUrls>,
    pub audio: Option<UnresolvedResolveMediaUrls>,
    pub pdf: Option<UnresolvedResolveMediaUrls>,
    pub video: Option<UnresolvedResolveMediaUrls>,
}

impl UnresolvedMediaUrlHandler {
    pub fn required_env_vars(&self) -> HashSet<String> {
        HashSet::new()
    }

    pub fn resolve(&self, ctx: &impl GetEnvVar) -> Result<MediaUrlHandler> {
        Ok(MediaUrlHandler {
            images: self.images.as_ref().map(|u| u.resolve(ctx)).transpose()?,
            audio: self.audio.as_ref().map(|u| u.resolve(ctx)).transpose()?,
            pdf: self.pdf.as_ref().map(|u| u.resolve(ctx)).transpose()?,
            video: self.video.as_ref().map(|u| u.resolve(ctx)).transpose()?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct MediaUrlHandler {
    pub images: Option<ResolveMediaUrls>,
    pub audio: Option<ResolveMediaUrls>,
    pub pdf: Option<ResolveMediaUrls>,
    pub video: Option<ResolveMediaUrls>,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_client_provider_parsing() {
        // Test parsing of openai-responses provider
        let provider = ClientProvider::from_str("openai-responses");
        assert!(provider.is_ok());

        let provider = provider.unwrap();
        match provider {
            ClientProvider::OpenAI(OpenAIClientProviderVariant::Responses) => {
                // Success!
            }
            _ => panic!("Expected OpenAI Responses variant, got {provider:?}"),
        }
    }

    #[test]
    fn test_openai_client_provider_variant_parsing() {
        let variant = OpenAIClientProviderVariant::from_str("openai-responses");
        assert!(variant.is_ok());
        assert_eq!(variant.unwrap(), OpenAIClientProviderVariant::Responses);
    }

    #[test]
    fn test_openai_responses_display() {
        let variant = OpenAIClientProviderVariant::Responses;
        assert_eq!(variant.to_string(), "openai-responses");
    }

    #[test]
    fn test_openai_responses_in_allowed_providers() {
        let allowed = ClientProvider::allowed_providers();
        assert!(allowed.contains(&"openai-responses"));
    }

    #[test]
    fn test_response_type_resolution() {
        use baml_types::GetEnvVar;

        struct MockEnvContext;
        impl GetEnvVar for MockEnvContext {
            fn get_env_var(&self, _name: &str) -> Result<String, anyhow::Error> {
                Err(anyhow::anyhow!("No env var"))
            }

            fn set_allow_missing_env_var(&self, _: bool) -> Self {
                MockEnvContext
            }
        }

        let unresolved = UnresolvedResponseType::OpenAIResponses;
        let ctx = MockEnvContext;
        let resolved = unresolved.resolve(&ctx);

        assert!(resolved.is_ok());
        assert!(matches!(resolved.unwrap(), ResponseType::OpenAIResponses));
    }

    #[test]
    fn test_provider_roundtrip() {
        // Test that we can convert to string and back
        let original = ClientProvider::OpenAI(OpenAIClientProviderVariant::Responses);
        let string_repr = match &original {
            ClientProvider::OpenAI(variant) => variant.to_string(),
            _ => panic!("Expected OpenAI provider"),
        };

        assert_eq!(string_repr, "openai-responses");

        let parsed_back = ClientProvider::from_str(&string_repr).unwrap();
        assert_eq!(original, parsed_back);
    }

    #[test]
    fn test_invalid_provider_parsing() {
        let result = ClientProvider::from_str("invalid-provider");
        assert!(result.is_err());

        let result = OpenAIClientProviderVariant::from_str("invalid-variant");
        assert!(result.is_err());
    }

    #[test]
    fn test_openrouter_provider_parsing() {
        let provider = ClientProvider::from_str("openrouter");
        assert!(provider.is_ok());

        let provider = provider.unwrap();
        match provider {
            ClientProvider::OpenAI(OpenAIClientProviderVariant::OpenRouter) => {
                // Success!
            }
            _ => panic!("Expected OpenRouter variant, got {provider:?}"),
        }
    }

    #[test]
    fn test_openrouter_variant_parsing() {
        let variant = OpenAIClientProviderVariant::from_str("openrouter");
        assert!(variant.is_ok());
        assert_eq!(variant.unwrap(), OpenAIClientProviderVariant::OpenRouter);
    }

    #[test]
    fn test_openrouter_display() {
        let variant = OpenAIClientProviderVariant::OpenRouter;
        assert_eq!(variant.to_string(), "openrouter");
    }

    #[test]
    fn test_openrouter_in_allowed_providers() {
        let allowed = ClientProvider::allowed_providers();
        assert!(allowed.contains(&"openrouter"));
    }

    #[test]
    fn test_openrouter_roundtrip() {
        let original = ClientProvider::OpenAI(OpenAIClientProviderVariant::OpenRouter);
        let string_repr = match &original {
            ClientProvider::OpenAI(variant) => variant.to_string(),
            _ => panic!("Expected OpenAI provider"),
        };

        assert_eq!(string_repr, "openrouter");

        let parsed_back = ClientProvider::from_str(&string_repr).unwrap();
        assert_eq!(original, parsed_back);
    }
}
