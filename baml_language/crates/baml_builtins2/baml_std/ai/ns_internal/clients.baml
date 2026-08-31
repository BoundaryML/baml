// Internal helpers behind `ai.clients.resolve`: the runtime half of the
// `"provider/model"` shorthand map. Nothing here is part of the public ai
// surface.

/// (internal) The `"provider/model"` prefixes `ai.clients.resolve` understands, for error
/// messages.
function _shorthand_prefixes() -> string {
    [
        "openai",
        "openai-chat",
        "openai-images",
        "azure",
        "ollama",
        "openrouter",
        "anthropic",
        "google",
        "vertex",
        "bedrock",
        "ai-gateway-images",
        "claude-code",
    ]
        .join(", ")
}

/// (internal) !!! DRIFT HAZARD !!!
/// This is the RUNTIME half of the "provider/model" shorthand map. Its
/// compile-time twin is `spec_client_provider` in
/// `crates/baml_compiler2_ast/src/lower_cst.rs`, which lowers a literal
/// `client "openai/gpt-4o-mini"` straight to a constructor call. The two maps
/// must list the SAME prefixes and pick the SAME class for each one — a
/// `client:` written as a literal and the identical string arriving here at
/// runtime have to select the same client. Change one, change the other, and
/// update `_shorthand_prefixes` above.
function _from_shorthand(shorthand: string) -> root.Client {
    let separator = shorthand.index_of("/");
    if let index: int = separator {
        let prefix = shorthand.slice(0, index);
        let model = shorthand.slice(index + 1, shorthand.length());
        let selected: root.Client? = match (prefix) {
            "openai" => openai.ResponsesClient.new(model = model),
            "openai-chat" => openai.ChatClient.new(model = model),
            "openai-images" => openai.ImageClient.new(model = model),
            "azure" => openai.AzureClient.new(model = model),
            "ollama" => openai.OllamaClient.new(model = model),
            "openrouter" => openai.OpenRouterClient.new(model = model),
            "anthropic" => anthropic.AnthropicClient.new(model = model),
            "google" => google.GoogleClient.new(model = model),
            "vertex" => google.VertexClient.new(model = model),
            "bedrock" => aws.BedrockClient.new(model = model),
            "ai-gateway-images" => vercel.AiGatewayImageClient.new(model = model),
            "claude-code" => claude_code.ClaudeCodeClient.new(model = model),
            _ => null,
        };
        if let built: root.Client = selected {
            built
        } else {
            throw root.errors.InvalidRequest {
                provider: "ai.clients",
                status_code: null,
                detail: `no builtin provider for prefix "${prefix}" in client "${shorthand}"; valid prefixes are ${_shorthand_prefixes()} (for any other OpenAI-compatible endpoint, build openai.GenericClient.new(base_url = ..., model = "${model}"))`,
                raw_body: null,
            }
        }
    } else {
        throw root.errors.InvalidRequest {
            provider: "ai.clients",
            status_code: null,
            detail: `client "${shorthand}" is not a "provider/model" string (no "/"); valid prefixes are ${_shorthand_prefixes()}`,
            raw_body: null,
        }
    }
}
