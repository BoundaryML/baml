// Clients for the streaming comparison, picked per call from Python.

type Provider = "anthropic" | "openai";

// Built in BAML: a Python-held client handle can't be passed as `ai.Client` yet.
function make_client(provider: Provider, model: string) -> ai.Client {
    match (provider) {
        "anthropic" => anthropic.Client.new(model = model, max_tokens = 32000),
        "openai" => openai.ResponsesClient.new(model = model),
    }
}
