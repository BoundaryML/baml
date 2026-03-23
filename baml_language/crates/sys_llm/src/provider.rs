/// Known LLM providers.
///
/// Parsed from `LlmPrimitiveClient.provider` using strum's `EnumString`.
/// Unknown provider strings fall through to parse errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum LlmProvider {
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

    /// Anthropic API (api.anthropic.com)
    #[strum(serialize = "anthropic")]
    Anthropic,

    /// Vertex AI -- uses `GenerateContentRequest` serialization
    #[strum(serialize = "vertex-ai")]
    VertexAi,

    /// Google AI (Gemini) -- uses `GenerateContentRequest` serialization
    #[strum(serialize = "google-ai")]
    GoogleAi,

    /// AWS Bedrock — uses Converse API with `SigV4` signing
    #[strum(serialize = "aws-bedrock")]
    AwsBedrock,

    // Strategy providers (not LLM providers — handled upstream)
    #[strum(serialize = "baml-fallback")]
    BamlFallback,
    #[strum(serialize = "baml-round-robin")]
    BamlRoundRobin,
}
