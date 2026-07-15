/// Known LLM providers.
///
/// Parsed from `LlmPrimitiveClient.provider` using strum's `EnumString`.
/// Unknown provider strings fall through to parse errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum LlmProvider {
    /// `OpenAI` API (api.openai.com)
    #[strum(serialize = "openai")]
    OpenAi,

    /// OpenAI-compatible generic endpoint (custom `base_url`)
    #[strum(serialize = "openai-generic")]
    OpenAiGeneric,

    /// Azure `OpenAI` Service
    #[strum(serialize = "azure-openai")]
    AzureOpenAi,

    /// Ollama (local OpenAI-compatible)
    #[strum(serialize = "ollama")]
    Ollama,

    /// `OpenRouter` (OpenAI-compatible)
    #[strum(serialize = "openrouter")]
    OpenRouter,

    /// `OpenAI` Responses API
    #[strum(serialize = "openai-responses")]
    OpenAiResponses,

    /// Vercel AI Gateway image generation endpoint
    #[strum(serialize = "ai-gateway-images")]
    AiGatewayImages,

    /// Anthropic API (api.anthropic.com)
    #[strum(serialize = "anthropic")]
    Anthropic,

    // --- Providers not yet supported by build_request ---
    /// Google AI (Gemini) — deferred
    #[strum(serialize = "google-ai")]
    GoogleAi,

    /// Vertex AI — deferred
    #[strum(serialize = "vertex-ai")]
    VertexAi,

    /// AWS Bedrock — uses Converse API with `SigV4` signing
    #[strum(serialize = "aws-bedrock")]
    AwsBedrock,

    // Strategy providers (not LLM providers — handled upstream)
    #[strum(serialize = "baml-fallback")]
    BamlFallback,
    #[strum(serialize = "baml-round-robin")]
    BamlRoundRobin,
}

impl LlmProvider {
    /// Conventional API-key environment variable for providers that define one.
    pub(crate) fn default_api_key_env_var(self) -> Option<&'static str> {
        match self {
            Self::OpenAi | Self::OpenAiResponses => Some("OPENAI_API_KEY"),
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            _ => None,
        }
    }
}
