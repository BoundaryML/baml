// Root namespace: references a type from a nested namespace
class AppConfig {
    api_key string
}

function CallOpenAI(cfg: AppConfig) -> root.llm.openai.OpenAIResponse {
    root.llm.openai.OpenAIResponse { text: "response", model: "gpt-4" }
}
