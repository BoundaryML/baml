// Trailing comments after string arguments in a client constructor call.
client OpenRouterMistralSmall3_1_24b = openai.ResponsesClient.new(
    base_url = "https://openrouter.ai/api/v1", // Optional
    api_key = "test-key", // Optional
    model = "mistralai/mistral-small-3.1-24b-instruct", // Optional
);

//----
//- diagnostics
// <no-diagnostics-expected>
